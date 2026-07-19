use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SourceConfig {
    pub name: String,
    pub url: String,
    pub fields: HashMap<String, FieldMapping>,

    #[serde(flatten)]
    pub format_config: FormatConfig,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum FormatConfig {
    Xml(XmlConfig),
    Csv(CsvConfig),
}

#[derive(Debug, Deserialize)]
pub struct XmlConfig {
    pub xml: XmlOptions,
}

#[derive(Debug, Deserialize)]
pub struct XmlOptions {
    pub record_tag: String,
}

#[derive(Debug, Deserialize)]
pub struct CsvConfig {
    pub csv: CsvOptions,
}

#[derive(Debug, Deserialize)]
pub struct CsvOptions {
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
    #[serde(default = "default_true")]
    pub has_header: bool,
}

fn default_delimiter() -> String {
    ",".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct FieldMapping {
    pub selector: String,
    #[serde(default = "default_field_type")]
    pub r#type: FieldType,
}

fn default_field_type() -> FieldType {
    FieldType::String
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Integer,
    Decimal,
    Date,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;
        Ok(config)
    }
}
