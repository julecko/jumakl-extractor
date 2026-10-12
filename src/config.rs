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

    /// When true, each written record is written twice: once with `prefix`,
    /// once with an empty prefix. Some downstream consumers key off the
    /// unprefixed SKU as well as the prefixed one.
    #[serde(default)]
    pub write_unprefixed_too: bool,

    #[serde(flatten)]
    pub format_config: FormatConfig,
}

/// One rule. The `kind` names a generic operation; field names, labels and
/// factors come from the rule's own config, never from code.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleConfig {
    /// Divides the price by 1.23 and rounds to two decimals. Price only.
    RemoveVat,
    /// Multiplies the price by `factor` when the extra field `field` equals `when`. Price only.
    DiscountMultiplier {
        field: String,
        when: String,
        factor: f64,
    },
    /// Keeps the part of the SKU from `start` up to `drop_end` characters from
    /// the end. Fails when the SKU is too short. Stock only.
    SkuSubstring { start: usize, drop_end: usize },
    /// Replaces a quantity by its entry in `map`. Values not in `map` become
    /// `default`, or stay as they are when there's no default. Stock only.
    QuantityMap {
        map: HashMap<String, String>,
        #[serde(default)]
        default: Option<String>,
    },
}

impl RuleConfig {
    /// The extract mode this rule belongs to. Rules can't run in the other mode.
    pub fn applies_to(&self) -> ExtractKind {
        match self {
            RuleConfig::RemoveVat | RuleConfig::DiscountMultiplier { .. } => ExtractKind::Price,
            RuleConfig::SkuSubstring { .. } | RuleConfig::QuantityMap { .. } => ExtractKind::Stock,
        }
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
    Attributes(AttributesConfig),
    HtmlListing1(Box<HtmlListing1Config>),
    AuthCrawl1(Box<AuthCrawl1Config>),
}

impl FormatConfig {
    // Only place that matches on format: picks which impl's vtable to hand
    // back. Everything downstream calls the trait method, never this enum.
    pub fn parser(&self) -> &dyn crate::sources::FormatParser {
        match self {
            FormatConfig::Xml(cfg) => cfg,
            FormatConfig::Csv(cfg) => cfg,
            FormatConfig::Attributes(cfg) => cfg,
            FormatConfig::HtmlListing1(cfg) => cfg.as_ref(),
            FormatConfig::AuthCrawl1(cfg) => cfg.as_ref(),
        }
    }
}

/// A paginated HTML listing of repeating blocks, plus optionally a handful
/// of standalone pages outside that listing. Every page-structure detail is
/// config - nothing supplier-specific lives in `sources::custom::html_listing1`.
#[derive(Debug, Deserialize)]
pub struct HtmlListing1Config {
    pub html_listing1: HtmlListing1Options,
}

#[derive(Debug, Deserialize)]
pub struct HtmlListing1Options {
    /// Literal substring marking the start of one record's block.
    pub record_start: String,
    /// Literal substring marking a nested element of the same kind the
    /// record block itself opens with (e.g. another opening tag of the same
    /// kind) - tracked so an inner element's close isn't mistaken for the
    /// record block's own close.
    pub nested_open: String,
    /// Literal substring that closes one level of nesting, and the record
    /// block itself once nesting returns to zero.
    pub record_end: String,
    /// When a freshly fetched page contains this substring, stop paginating
    /// (that page is fetched but not parsed). Omit to fetch only `url`, with
    /// no pagination.
    #[serde(default)]
    pub stop_marker: Option<String>,
    /// Query parameter to increment to get the next page's url. Required
    /// when `stop_marker` is set.
    #[serde(default)]
    pub page_param: Option<String>,
    /// Standalone pages outside the paginated listing, each treated as one
    /// implicit record via `extra_page_fields`.
    #[serde(default)]
    pub extra_pages: Vec<String>,
    /// How to find sku and stock/price inside each record block.
    pub fields: HashMap<String, HtmlField>,
    /// How to find fields on an `extra_pages` entry. Usually shaped
    /// differently than `fields`, since a standalone page isn't wrapped in
    /// the listing's repeating block.
    #[serde(default)]
    pub extra_page_fields: HashMap<String, HtmlField>,
}

/// Finds one field's raw text: starting right after the first `after`,
/// skipping `skip_closing_brackets` '>' characters first (to jump past a
/// tag's own attributes, which may vary per record and so can't be matched
/// literally), capturing up to `until`. When `value_prefix`/`value_suffix`
/// are set, a second pass extracts the text between them out of that raw
/// text (e.g. pulling a number out of a sentence). `default` is used when
/// the field - or, with prefix/suffix set, the inner value - isn't found.
#[derive(Debug, Deserialize, Clone)]
pub struct HtmlField {
    pub after: String,
    #[serde(default)]
    pub skip_closing_brackets: usize,
    pub until: String,
    #[serde(default)]
    pub value_prefix: Option<String>,
    #[serde(default)]
    pub value_suffix: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
}

/// A cookie-authenticated catalog: log in once (credentials from
/// `AUTH_CRAWL1_USERNAME`/`AUTH_CRAWL1_PASSWORD` in `.env` - secrets never
/// belong in a config file), then crawl every category link off the site's
/// nav menu, paginating each one until a page repeats a product already
/// seen. Every page-structure/markup detail is config - nothing
/// supplier-specific lives in `sources::custom::auth_crawl1`.
#[derive(Debug, Deserialize)]
pub struct AuthCrawl1Config {
    pub auth_crawl1: AuthCrawl1Options,
}

#[derive(Debug, Deserialize)]
pub struct AuthCrawl1Options {
    pub login_url: String,
    /// Sent as the login POST's `Origin`/`Referer` header.
    pub origin: String,
    /// The login form's `__EVENTTARGET` value.
    pub event_target: String,
    /// The login form's username/password field names.
    pub username_field: String,
    pub password_field: String,
    /// Literal substrings bounding the nav block that holds every category link.
    pub nav_start: String,
    pub nav_end: String,
    /// Literal substrings bounding one category page's product table.
    pub products_start: String,
    pub products_end: String,
    /// Regex with one capture group: a product's code. Searched for
    /// repeatedly within the products section.
    pub id_pattern: String,
    /// Regex with one capture group: a product's quantity. Searched for in
    /// the text right after each `id_pattern` match.
    pub quantity_pattern: String,
}

/// Feeds where each record is an element and its values are attributes, e.g.
/// `<Item Code="A1" Qty="5" />`. Field selectors name the attributes.
#[derive(Debug, Deserialize)]
pub struct AttributesConfig {
    pub attributes: AttributesOptions,
}

#[derive(Debug, Deserialize)]
pub struct AttributesOptions {
    pub record_tag: String,
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
    /// When true, field selectors are column positions (e.g. "3") rather
    /// than header column names - the header row is still skipped if
    /// `has_header` is true, just not used to resolve selectors. For feeds
    /// whose columns are only reliable by position, not by name.
    #[serde(default)]
    pub fields_by_position: bool,
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
                if rule.applies_to() != kind {
                    bail!(
                        "source '{}' has a {:?} rule, which doesn't apply to {kind:?} extraction",
                        source.name,
                        rule.applies_to()
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
