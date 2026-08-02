use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use std::collections::HashMap;

use super::{FormatParser, Record, RecordValue, coerce};
use crate::config::{FieldMapping, XmlConfig};

impl FormatParser for XmlConfig {
    fn parse(
        &self,
        content: &str,
        fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        let mut reader = Reader::from_str(content);
        reader.config_mut().trim_text(true);

        let record_tag = self.xml.record_tag.as_bytes();

        let mut in_record = false;
        let mut sku: Option<String> = None;
        let mut value: Option<RecordValue> = None;
        let mut current_tag: Vec<u8> = Vec::new();

        loop {
            match reader.read_event().context("failed to read xml event")? {
                Event::Eof => break,

                Event::Start(e) => {
                    let tag = e.name().into_inner();
                    if tag == record_tag {
                        in_record = true;
                        sku = None;
                        value = None;
                    }
                    current_tag = tag.to_vec();
                }

                Event::Text(e) => {
                    if !in_record {
                        continue;
                    }

                    let decoded = e.decode().context("invalid xml text encoding")?;
                    let text = unescape(&decoded).context("invalid xml entity")?;
                    apply_field(&text, &current_tag, fields, &mut sku, &mut value)?;
                }

                Event::CData(e) => {
                    if !in_record {
                        continue;
                    }

                    // CDATA is raw by definition - no entity unescaping, unlike Text.
                    let text = e.decode().context("invalid xml cdata encoding")?;
                    apply_field(&text, &current_tag, fields, &mut sku, &mut value)?;
                }

                Event::End(e) => {
                    if e.name().into_inner() == record_tag {
                        in_record = false;
                        match (sku.take(), value.take()) {
                            (Some(sku), Some(value)) => on_record(Record { sku, value })?,
                            _ => tracing::warn!(
                                "skipping <{}> element missing sku and/or value",
                                self.xml.record_tag
                            ),
                        }
                    }
                }

                _ => {}
            }
        }

        Ok(())
    }
}

/// Shared by both Event::Text and Event::CData: look up which configured
/// field the current tag maps to, coerce the decoded text, and store it.
fn apply_field(
    text: &str,
    current_tag: &[u8],
    fields: &HashMap<String, FieldMapping>,
    sku: &mut Option<String>,
    value: &mut Option<RecordValue>,
) -> Result<()> {
    let field = fields
        .iter()
        .find(|(_, mapping)| mapping.selector.as_bytes() == current_tag);
    let Some((field_name, mapping)) = field else {
        return Ok(());
    };

    let coerced = coerce(text, mapping.r#type)?;

    match field_name.as_str() {
        "sku" => *sku = Some(coerced.into_string()),
        "stock" => *value = Some(RecordValue::Stock(coerced.into_i64()?)),
        "price" => *value = Some(RecordValue::Price(coerced.into_f64()?)),
        _ => {}
    }

    Ok(())
}
