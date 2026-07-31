use anyhow::Result;
use std::collections::HashMap;

use super::{FormatParser, Record};
use crate::config::{CsvConfig, FieldMapping};

impl FormatParser for CsvConfig {
    fn parse(
        &self,
        _content: &str,
        _fields: &HashMap<String, FieldMapping>,
    ) -> Result<Vec<Record>> {
        todo!("split content on self.csv.delimiter, map columns to fields via mapping.selector")
    }
}
