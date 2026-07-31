use anyhow::Result;
use std::collections::HashMap;

use super::{FormatParser, Record};
use crate::config::{FieldMapping, XmlConfig};

impl FormatParser for XmlConfig {
    fn parse(
        &self,
        _content: &str,
        _fields: &HashMap<String, FieldMapping>,
    ) -> Result<Vec<Record>> {
        todo!("walk self.xml.record_tag elements, map child tags to fields via mapping.selector")
    }
}
