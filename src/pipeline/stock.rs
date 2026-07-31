use anyhow::Result;
use tracing::{debug, info};

use super::Handler;
use crate::sources::Record;

#[derive(Default)]
pub struct StockHandler {
    record_count: usize,
    // plus whatever else stock analysis needs across records, e.g. a Vec<StockRow>
}

impl Handler for StockHandler {
    fn on_record(&mut self, record: Record) -> Result<()> {
        self.record_count += 1;
        debug!("{:?}", record);
        Ok(())
    }

    fn finish(&mut self, source_name: &str) -> Result<()> {
        info!("{source_name}: processed {} records", self.record_count);
        Ok(())
    }
}
