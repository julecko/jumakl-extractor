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
        let mut ean: Option<String> = None;
        let mut value: Option<RecordValue> = None;
        let mut current_tag: Vec<u8> = Vec::new();

        loop {
            match reader.read_event().context("failed to read xml event")? {
                Event::Eof => break,

                Event::Start(e) => {
                    let tag = e.name().into_inner();
                    if tag == record_tag {
                        in_record = true;
                        ean = None;
                        value = None;
                    }
                    current_tag = tag.to_vec();
                }

                Event::Text(e) => {
                    if !in_record {
                        continue;
                    }

                    let field = fields
                        .iter()
                        .find(|(_, mapping)| mapping.selector.as_bytes() == current_tag.as_slice());
                    let Some((field_name, mapping)) = field else {
                        continue;
                    };

                    let decoded = e.decode().context("invalid xml text encoding")?;
                    let text = unescape(&decoded).context("invalid xml entity")?;
                    let coerced = coerce(&text, mapping.r#type)?;

                    match field_name.as_str() {
                        "ean" => ean = Some(coerced.into_string()),
                        "stock" => value = Some(RecordValue::Stock(coerced.into_i64()?)),
                        "price" => value = Some(RecordValue::Price(coerced.into_f64()?)),
                        _ => {}
                    }
                }

                Event::End(e) => {
                    if e.name().into_inner() == record_tag {
                        in_record = false;
                        match (ean.take(), value.take()) {
                            (Some(ean), Some(value)) => on_record(Record { ean, value })?,
                            _ => tracing::warn!(
                                "skipping <{}> element missing ean and/or value",
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
