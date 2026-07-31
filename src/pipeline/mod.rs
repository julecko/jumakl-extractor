mod price;
mod stock;

use anyhow::Result;
use reqwest::blocking::Client;

use crate::cli::ExtractKind;
use crate::config::{Config, SourceConfig};
use crate::sources::{self, Record};
use crate::webrequest;

/// One implementor per extract kind (stock.rs, price.rs, ...). Handler code
/// only ever sees the generic Record shape - it never knows the source format.
pub trait Handler {
    fn handle(&self, source_name: &str, records: Vec<Record>) -> Result<()>;
}

// Only place that matches on kind: picks which impl's vtable to hand back.
// Everything downstream calls the trait method, never this enum.
fn handler_for(kind: ExtractKind) -> &'static dyn Handler {
    match kind {
        ExtractKind::Stock => &stock::StockHandler,
        ExtractKind::Price => &price::PriceHandler,
    }
}

pub fn run(kind: ExtractKind, config: &Config) {
    tracing::debug!("Starting extraction");

    let client = Client::new();
    let handler = handler_for(kind);

    for source in &config.sources {
        if let Err(err) = run_source(handler, &client, source) {
            tracing::error!("source {} failed: {err:#}", source.name);
        }
    }

    // Send data to server to store for analytics and also send email with price data which to update
}

// No match here at all: format was resolved inside parse_source, kind was
// resolved by the caller. Neither axis nests inside the other.
fn run_source(handler: &dyn Handler, client: &Client, source: &SourceConfig) -> Result<()> {
    let content = webrequest::fetch(client, &source.url)?;
    let records = sources::parse_source(&content, source)?;
    handler.handle(&source.name, records)
}
