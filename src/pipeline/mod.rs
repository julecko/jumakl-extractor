mod price;
mod stock;

use anyhow::Result;
use reqwest::blocking::Client;

use crate::cli::ExtractKind;
use crate::config::{Config, SourceConfig};
use crate::output::{self, OutputWriter};
use crate::sources::{self, Record};
use crate::webrequest;

/// One implementor per extract kind (stock.rs, price.rs, ...). Handler code
/// only ever sees the generic Record shape - it never knows the source format.
///
/// `on_record` is called once per record as the parser produces it (analyze /
/// accumulate whatever's needed), `finish` is called once after a source's
/// records are exhausted (write the result for that source). Handlers hold
/// state between calls, so each source gets its own fresh instance rather
/// than sharing one - see `new_handler`.
pub trait Handler {
    fn on_record(&mut self, record: Record) -> Result<()>;
    fn finish(&mut self, source_name: &str) -> Result<()>;
}

// Only place that matches on kind: picks which concrete type to box up.
// Everything downstream calls the trait methods, never this enum.
fn new_handler(kind: ExtractKind) -> Box<dyn Handler> {
    match kind {
        ExtractKind::Stock => Box::new(stock::StockHandler::default()),
        ExtractKind::Price => Box::new(price::PriceHandler::default()),
    }
}

pub fn run(kind: ExtractKind, config: &Config) {
    tracing::debug!("Starting extraction");

    let client = Client::new();

    let mut writer = match output::create_writer(kind) {
        Ok(writer) => writer,
        Err(err) => {
            tracing::error!("failed to open output file: {err:#}");
            return;
        }
    };

    for source in &config.sources {
        if let Err(err) = run_source(kind, &client, source, writer.as_mut()) {
            tracing::error!("source {} failed: {err:#}", source.name);
        }
    }

    // Send data to server to store for analytics and also send email with price data which to update
}

// No match here at all: format was resolved inside parse_source, kind was
// resolved by new_handler. Neither axis nests inside the other.
fn run_source(
    kind: ExtractKind,
    client: &Client,
    source: &SourceConfig,
    writer: &mut dyn OutputWriter,
) -> Result<()> {
    let content = webrequest::fetch(client, &source.url, source.auth.as_ref())?;

    let mut handler = new_handler(kind);

    let shortname = source.shortname.as_str();
    let prefix = source.prefix.as_str();
    sources::parse_source(&content, source, &mut |record| {
        writer.write_record(&record, shortname, prefix)?;
        handler.on_record(record)
    })?;

    handler.finish(&source.name)
}
