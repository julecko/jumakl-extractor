use anyhow::Result;
use tracing::debug;

use super::Handler;
use crate::sources::Record;

pub struct StockHandler;

impl Handler for StockHandler {
    fn handle(&self, source_name: &str, records: Vec<Record>) -> Result<()> {
        debug!(
            "Handling stock for {source_name} ({} records)",
            records.len()
        );
        todo!("pull sku/quantity out of each record, build stock struct")
    }
}
