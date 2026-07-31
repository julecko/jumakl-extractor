use anyhow::Result;
use tracing::debug;

use super::Handler;
use crate::sources::Record;

pub struct PriceHandler;

impl Handler for PriceHandler {
    fn handle(&self, source_name: &str, records: Vec<Record>) -> Result<()> {
        debug!(
            "Handling price for {source_name} ({} records)",
            records.len()
        );
        todo!("pull sku/price out of each record, build price struct, analyze")
    }
}
