use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use std::collections::HashMap;

use super::{FormatParser, Record, RecordValue, assign_field};
use crate::config::{AttributesConfig, FieldMapping};

/// Each record is an element whose values are attributes. A field's selector
/// is the attribute name to read.
impl FormatParser for AttributesConfig {
    fn parse(
        &self,
        content: &str,
        fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        let mut reader = Reader::from_str(content);
        let record_tag = self.attributes.record_tag.as_bytes();
        let record_name = self.attributes.record_tag.as_str();

        let mut sku: Option<String> = None;
        let mut value: Option<RecordValue> = None;
        let mut extras: HashMap<String, String> = HashMap::new();

        loop {
            match reader.read_event().context("failed to read xml event")? {
                Event::Eof => break,

                // A record with child elements: values come from its start tag,
                // and the record is handed over at its end tag.
                Event::Start(e) if e.name().into_inner() == record_tag => {
                    sku = None;
                    value = None;
                    extras.clear();
                    read_attributes(&e, fields, &mut sku, &mut value, &mut extras)?;
                }
                Event::End(e) if e.name().into_inner() == record_tag => {
                    emit(&mut sku, &mut value, &mut extras, on_record, record_name)?;
                }

                // A self-closing record: all its values are on this one tag.
                Event::Empty(e) if e.name().into_inner() == record_tag => {
                    sku = None;
                    value = None;
                    extras.clear();
                    read_attributes(&e, fields, &mut sku, &mut value, &mut extras)?;
                    emit(&mut sku, &mut value, &mut extras, on_record, record_name)?;
                }

                _ => {}
            }
        }

        Ok(())
    }
}

/// Stores each attribute whose name is a field's selector.
fn read_attributes(
    element: &BytesStart,
    fields: &HashMap<String, FieldMapping>,
    sku: &mut Option<String>,
    value: &mut Option<RecordValue>,
    extras: &mut HashMap<String, String>,
) -> Result<()> {
    for attribute in element.attributes() {
        let attribute = attribute.context("invalid xml attribute")?;
        let name = attribute.key.into_inner();
        let text = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .context("invalid xml attribute value")?;

        for (field_name, mapping) in fields {
            if mapping.selector.as_bytes() == name {
                assign_field(field_name, &text, mapping.r#type, sku, value, extras)?;
            }
        }
    }
    Ok(())
}

/// Hands the finished record to the caller, or warns when it has no sku or value.
fn emit(
    sku: &mut Option<String>,
    value: &mut Option<RecordValue>,
    extras: &mut HashMap<String, String>,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
    record_name: &str,
) -> Result<()> {
    match (sku.take(), value.take()) {
        (Some(sku), Some(value)) => on_record(Record {
            sku,
            value,
            extras: std::mem::take(extras),
        }),
        _ => {
            tracing::warn!("skipping <{record_name}> element missing sku and/or value");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AttributesOptions, FieldType};

    fn fields(pairs: &[(&str, &str)]) -> HashMap<String, FieldMapping> {
        pairs
            .iter()
            .map(|(name, selector)| {
                (
                    name.to_string(),
                    FieldMapping {
                        selector: selector.to_string(),
                        r#type: FieldType::String,
                    },
                )
            })
            .collect()
    }

    fn skus_of(content: &str) -> Vec<String> {
        let config = AttributesConfig {
            attributes: AttributesOptions {
                record_tag: "Item".to_string(),
            },
        };
        let mut skus = Vec::new();
        config
            .parse(
                content,
                &fields(&[("sku", "Code"), ("stock", "Qty")]),
                &mut |record| {
                    skus.push(record.sku);
                    Ok(())
                },
            )
            .unwrap();
        skus
    }

    #[test]
    fn reads_self_closing_records() {
        let content =
            r#"<Result><Item Id="1" Code="A1" Qty="5" /><Item Code="B2" Qty="7" /></Result>"#;
        assert_eq!(skus_of(content), vec!["A1", "B2"]);
    }

    #[test]
    fn reads_records_with_open_and_close_tags() {
        let content = r#"<Result><Item Code="C3" Qty="1"></Item></Result>"#;
        assert_eq!(skus_of(content), vec!["C3"]);
    }
}
