mod csv;
mod xml;

use anyhow::Result;
use std::collections::HashMap;

use crate::config::{FieldMapping, SourceConfig};

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Integer(i64),
    Decimal(f64),
    Date(String),
}

pub type Record = HashMap<String, Value>;

/// One implementor per file format (csv.rs, xml.rs, ...). Format code only
/// ever produces the generic Record shape - it never knows about stock/price.
pub trait FormatParser {
    fn parse(&self, content: &str, fields: &HashMap<String, FieldMapping>) -> Result<Vec<Record>>;
}

pub fn parse_source(content: &str, source: &SourceConfig) -> Result<Vec<Record>> {
    source.format_config.parser().parse(content, &source.fields)
}
