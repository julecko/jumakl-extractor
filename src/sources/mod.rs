mod csv;
mod xml;

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::config::{FieldMapping, FieldType, SourceConfig};

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Integer(i64),
    Decimal(f64),
    Date(String),
}

#[derive(Debug, Clone)]
pub struct Record {
    pub sku: String,
    pub value: RecordValue,
}

#[derive(Debug, Clone)]
pub enum RecordValue {
    Stock(i64),
    Price(f64),
}

impl RecordValue {
    pub fn as_stock(&self) -> Option<i64> {
        match self {
            RecordValue::Stock(n) => Some(*n),
            RecordValue::Price(_) => None,
        }
    }

    pub fn as_price(&self) -> Option<f64> {
        match self {
            RecordValue::Price(p) => Some(*p),
            RecordValue::Stock(_) => None,
        }
    }
}

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

/// Shared by every FormatParser impl: turns a raw selector match into the
/// typed Value the rest of the pipeline works with.
pub(crate) fn coerce(raw: &str, field_type: FieldType) -> Result<Value> {
    let raw = raw.trim();
    Ok(match field_type {
        FieldType::String => Value::String(raw.to_string()),
        FieldType::Integer => Value::Integer(raw.parse()?),
        FieldType::Decimal => Value::Decimal(raw.parse()?),
        FieldType::Date => Value::Date(raw.to_string()),
    })
}

impl Value {
    /// "sku" is always treated as a plain string regardless of configured type.
    pub(crate) fn into_string(self) -> String {
        match self {
            Value::String(s) | Value::Date(s) => s,
            Value::Integer(n) => n.to_string(),
            Value::Decimal(f) => f.to_string(),
        }
    }

    /// For "stock" - the config may declare it integer or decimal, RecordValue always wants i64.
    pub(crate) fn into_i64(self) -> Result<i64> {
        match self {
            Value::Integer(n) => Ok(n),
            Value::Decimal(f) => Ok(f as i64),
            Value::String(s) | Value::Date(s) => s
                .trim()
                .parse()
                .with_context(|| format!("'{s}' is not an integer")),
        }
    }

    /// For "price" - the config may declare it integer or decimal, RecordValue always wants f64.
    pub(crate) fn into_f64(self) -> Result<f64> {
        match self {
            Value::Decimal(f) => Ok(f),
            Value::Integer(n) => Ok(n as f64),
            Value::String(s) | Value::Date(s) => s
                .trim()
                .parse()
                .with_context(|| format!("'{s}' is not a decimal")),
        }
    }
}
