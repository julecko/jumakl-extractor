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
///
/// `on_record` fires once per parsed record instead of the parser collecting
/// a Vec<Record> - the caller decides what to keep, so nothing forces the
/// whole file to sit in memory at once.
pub trait FormatParser {
    fn parse(
        &self,
        content: &str,
        fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()>;
}

pub fn parse_source(
    content: &str,
    source: &SourceConfig,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
) -> Result<()> {
    source
        .format_config
        .parser()
        .parse(content, &source.fields, on_record)
}
