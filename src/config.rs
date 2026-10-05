use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::cli::ExtractKind;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub sources: Vec<SourceConfig>,
}

/// Settings for the whole program, independent of --extract kind - kept
/// deliberately separate from Config/SourceConfig, which are always loaded
/// per-kind from sources.stock.toml / sources.price.toml. Loaded once, from
/// its own file (config/program.toml), not per mode.
#[derive(Debug, Deserialize, Default)]
pub struct ProgramConfig {
    /// Regex rules (not exact SKUs) - any record whose sku matches ANY of
    /// these is skipped entirely, before it reaches a Handler or a writer.
    /// Raw pattern strings here; compiled once via compiled_sku_exclusions().
    #[serde(default)]
    pub excluded_sku_patterns: Vec<String>,
    /// Our own reference prices, used by --extract price. Optional so stock
    /// runs don't need it; a price run without it reports a program error.
    #[serde(default)]
    pub pricebook: Option<PriceBookConfig>,
    // Add more whole-program settings here as needed.
}

#[derive(Debug, Deserialize)]
pub struct PriceBookConfig {
    pub url: String,
}

impl ProgramConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read program config file: {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("failed to parse program config file: {}", path.display()))
    }

    /// Compiles excluded_sku_patterns once at startup, so run_source only
    /// ever matches against already-compiled Regex, never recompiling per record.
    pub fn compiled_sku_exclusions(&self) -> Result<Vec<Regex>> {
        self.excluded_sku_patterns
            .iter()
            .map(|pattern| {
                Regex::new(pattern)
                    .with_context(|| format!("invalid excluded_sku_patterns regex: '{pattern}'"))
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
pub struct SourceConfig {
    pub name: String,
    pub shortname: String,
    pub prefix: String,
    pub url: String,

    pub auth: Option<AuthConfig>,
    pub fields: HashMap<String, FieldMapping>,

    /// Rules applied to each record, in the order listed. Only used by
    /// --extract price.
    #[serde(default)]
    pub rules: Vec<RuleConfig>,

    #[serde(flatten)]
    pub format_config: FormatConfig,
}

/// One rule. The `kind` names a generic operation; field names, labels and
/// factors come from the rule's own config, never from code.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleConfig {
    /// Divides the price by 1.23 and rounds to two decimals.
    RemoveVat,
    /// Multiplies the price by `factor` when the extra field `field` equals `when`.
    DiscountMultiplier {
        field: String,
        when: String,
        factor: f64,
    },
}

impl RuleConfig {
    /// Price rules can't be applied to stock records.
    pub fn is_price_only(&self) -> bool {
        matches!(
            self,
            RuleConfig::RemoveVat | RuleConfig::DiscountMultiplier { .. }
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum FormatConfig {
    Xml(XmlConfig),
    Csv(CsvConfig),
}

impl FormatConfig {
    // Only place that matches on format: picks which impl's vtable to hand
    // back. Everything downstream calls the trait method, never this enum.
    pub fn parser(&self) -> &dyn crate::sources::FormatParser {
        match self {
            FormatConfig::Xml(cfg) => cfg,
            FormatConfig::Csv(cfg) => cfg,
        }
    }
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
    pub fn load(path: impl AsRef<Path>, kind: ExtractKind) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;

        config.validate(kind)?;
        Ok(config)
    }

    fn validate(&self, kind: ExtractKind) -> Result<()> {
        // "sku" is always required; which of "price"/"stock" is required
        // depends on which mode this config file is being loaded for.
        let kind_field = kind.value_field();

        for source in &self.sources {
            if !source.fields.contains_key("sku") {
                bail!("source '{}' is missing mandatory field 'sku'", source.name);
            }
            if !source.fields.contains_key(kind_field) {
                bail!(
                    "source '{}' is missing mandatory field '{kind_field}' for {kind:?} extraction",
                    source.name
                );
            }

            for rule in &source.rules {
                if kind == ExtractKind::Stock && rule.is_price_only() {
                    bail!(
                        "source '{}' uses a price rule, which only applies to price extraction",
                        source.name
                    );
                }
                if let RuleConfig::DiscountMultiplier { field, .. } = rule
                    && !source.fields.contains_key(field)
                {
                    bail!(
                        "source '{}' has a discount rule for field '{field}', which is not in its fields",
                        source.name
                    );
                }
            }
        }

        Ok(())
    }
}
