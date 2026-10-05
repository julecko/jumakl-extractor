mod price;
mod stock;

use anyhow::Result;
use regex::Regex;
use reqwest::blocking::Client;

use crate::cli::ExtractKind;
use crate::config::{Config, PriceBookConfig, SourceConfig};
use crate::output::{self, OutputWriter, WriteRow};
use crate::pricebook::{self, PriceBook};
use crate::sources::{self, Record};
use crate::webrequest;

/// One implementor per extract kind (stock.rs, price.rs, ...). Handler code
/// only ever sees the generic Record shape - it never knows the source format.
///
/// `on_record` is called once per record as the parser produces it: it reads
/// the raw Record (analyze / accumulate whatever's needed) and decides what,
/// if anything, should actually be written - `Ok(None)` means skip writing
/// this record entirely (e.g. dropped on purpose), `Ok(Some(row))` hands back
/// the derived, ready-to-export shape (this is where price editing - buy/sell/
/// fix math - happens, not in the writer). `finish` is called once after a
/// source's records are exhausted. Handlers hold state between calls, so each
/// source gets its own fresh instance rather than sharing one - see `new_handler`.
pub trait Handler {
    fn on_record(&mut self, record: &Record) -> Result<Option<WriteRow>>;
    fn finish(&mut self, source_name: &str) -> SourceReport;
}

/// Everything worth telling a human about one source's run, destined for the
/// eventual mail report. Never fails to exist - even a source whose fetch
/// failed outright still produces one of these, with the error recorded in
/// it, so the report covers every configured source, not just the healthy ones.
#[derive(Debug)]
pub struct SourceReport {
    pub supplier: String,
    pub records: usize,
    pub excluded: usize,
    pub errors: Vec<String>,
    /// Only filled by the price handler; empty for stock.
    pub price_fixes: Vec<PriceFix>,
}

/// A product whose sell price is more than 2% away from our original price.
/// Listed in the separate price-fix email.
#[derive(Debug)]
pub struct PriceFix {
    /// Full SKU with the supplier prefix, e.g. "PRE - 6285".
    pub sku: String,
    pub original_price: f64,
    pub new_price: f64,
}

/// `run`'s full result: one SourceReport per source, plus any failure that
/// isn't tied to a specific source at all (e.g. the output file itself
/// couldn't be created) and so can't live on a SourceReport. The program
/// always reports what happened - a run-level failure still has to reach
/// the eventual email, not just vanish into a log line.
pub struct RunResult {
    pub reports: Vec<SourceReport>,
    pub program_errors: Vec<String>,
}

// Only place that matches on kind: picks which concrete type to box up.
// Everything downstream calls the trait methods, never this enum.
fn new_handler<'a>(
    kind: ExtractKind,
    source: &'a SourceConfig,
    pricebook: Option<&'a PriceBook>,
) -> Result<Box<dyn Handler + 'a>> {
    match kind {
        ExtractKind::Stock => Ok(Box::new(stock::StockHandler::default())),
        ExtractKind::Price => {
            let Some(pricebook) = pricebook else {
                anyhow::bail!("no pricebook loaded for price extraction");
            };
            Ok(Box::new(price::PriceHandler::new(source, pricebook)))
        }
    }
}

pub fn run(
    kind: ExtractKind,
    config: &Config,
    sku_exclusions: &[Regex],
    pricebook_config: Option<&PriceBookConfig>,
) -> RunResult {
    tracing::debug!("Starting extraction");

    let client = match webrequest::build_client() {
        Ok(c) => c,
        Err(err) => {
            tracing::error!("failed to build web client: {err:#}");
            return RunResult {
                reports: Vec::new(),
                program_errors: vec![format!("failed to build web client: {err:#}")],
            };
        }
    };

    // Loaded before the output file is created, so a failed download leaves
    // the previous run's output in place.
    let pricebook = match (kind, pricebook_config) {
        (ExtractKind::Stock, _) => None,
        (ExtractKind::Price, None) => {
            return RunResult {
                reports: Vec::new(),
                program_errors: vec!["Price: no [pricebook] url in program.toml".to_string()],
            };
        }
        (ExtractKind::Price, Some(pricebook_config)) => {
            match pricebook::load(&client, pricebook_config) {
                Ok(book) => Some(book),
                Err(err) => {
                    tracing::error!("failed to load pricebook: {err:#}");
                    return RunResult {
                        reports: Vec::new(),
                        program_errors: vec![format!("Price: failed to load pricebook: {err:#}")],
                    };
                }
            }
        }
    };

    let mut writer = match output::create_writer(kind) {
        Ok(writer) => writer,
        Err(err) => {
            tracing::error!("failed to open output file: {err:#}");
            return RunResult {
                reports: Vec::new(),
                program_errors: vec![format!("{kind:?}: failed to open output file: {err:#}")],
            };
        }
    };

    let mut reports = Vec::with_capacity(config.sources.len());
    for source in &config.sources {
        reports.push(run_source(
            kind,
            &client,
            source,
            writer.as_mut(),
            sku_exclusions,
            pricebook.as_ref(),
        ));
    }

    for report in &reports {
        if !report.errors.is_empty() {
            tracing::warn!(
                "{}: {} record(s), {} error(s)",
                report.supplier,
                report.records,
                report.errors.len()
            );
        }
    }

    RunResult {
        reports,
        program_errors: Vec::new(),
    }
}

// No match here at all: format was resolved inside parse_source, kind was
// resolved by new_handler. Neither axis nests inside the other.
//
// Never returns Err - every failure (fetch, parse, per-record, finish) is
// recorded into the SourceReport instead of dropped, so one bad source still
// yields a report explaining what went wrong, rather than nothing at all.
fn run_source(
    kind: ExtractKind,
    client: &Client,
    source: &SourceConfig,
    writer: &mut dyn OutputWriter,
    sku_exclusions: &[Regex],
    pricebook: Option<&PriceBook>,
) -> SourceReport {
    let content = match webrequest::fetch(client, &source.url, source.auth.as_ref()) {
        Ok(content) => content,
        Err(err) => {
            tracing::error!("source {} failed to fetch: {err:#}", source.name);
            return SourceReport {
                supplier: source.name.clone(),
                records: 0,
                excluded: 0,
                errors: vec![format!("fetch failed: {err:#}")],
                price_fixes: Vec::new(),
            };
        }
    };

    let mut handler = match new_handler(kind, source, pricebook) {
        Ok(handler) => handler,
        Err(err) => {
            tracing::error!("source {} failed to set up: {err:#}", source.name);
            return SourceReport {
                supplier: source.name.clone(),
                records: 0,
                excluded: 0,
                errors: vec![format!("setup failed: {err:#}")],
                price_fixes: Vec::new(),
            };
        }
    };

    // Errors from outside the handler's own view (writing, or on_record
    // propagating one) - merged into the handler's report after finish(),
    // since the handler doesn't know about the writer or fetch/parse at all.
    let mut external_errors = Vec::new();
    let mut excluded_count = 0;

    let shortname = source.shortname.as_str();
    let prefix = source.prefix.as_str();
    let parse_result = sources::parse_source(&content, source, &mut |record| {
        if sku_exclusions
            .iter()
            .any(|pattern| pattern.is_match(&record.sku))
        {
            tracing::debug!("skipping excluded sku (sku={})", record.sku);
            excluded_count += 1;
            return Ok(());
        }

        match handler.on_record(&record) {
            Ok(Some(row)) => {
                if let Err(err) = writer.write_record(&record.sku, &row, shortname, prefix) {
                    tracing::warn!(
                        "failed to write record (source={}) (sku={}): {err:#}",
                        source.name,
                        record.sku
                    );
                    external_errors.push(format!(
                        "write failed (source={}) (sku={}): {err:#}",
                        source.name, record.sku
                    ));
                }
            }
            // The handler dropped this record on purpose (e.g. price: SKU not in
            // the pricebook). It has already counted that itself.
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("failed to handle record (sku={}): {err:#}", record.sku);
                external_errors.push(format!("handle failed (sku={}): {err:#}", record.sku));
            }
        }

        Ok(())
    });

    // Handler owns the record count (it already tracks it internally) and
    // builds the report; run_source only adds what it alone can see -
    // excluded records never reach the handler at all, so it can't count them.
    let mut report = handler.finish(&source.name);
    report.excluded = excluded_count;
    report.errors.extend(external_errors);

    if let Err(err) = parse_result {
        tracing::error!("source {} failed to parse: {err:#}", source.name);
        report.errors.push(format!("parse failed: {err:#}"));
    }

    report
}
