use anyhow::Result;
use std::collections::HashMap;

use super::{FormatParser, Record};
use crate::config::{FieldMapping, XmlConfig};

impl FormatParser for XmlConfig {
    fn parse(
        &self,
        _content: &str,
        _fields: &HashMap<String, FieldMapping>,
        _on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        todo!(
            "walk self.xml.record_tag elements, map child tags to fields via \
             mapping.selector, call on_record(record) at each closing record_tag \
             instead of collecting"
        );
        return Ok(());
    }
}
