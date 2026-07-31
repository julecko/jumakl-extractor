use anyhow::Result;
use tracing::debug;

use super::Handler;
use crate::sources::Record;

#[derive(Default)]
pub struct StockHandler {
    record_count: usize,
    // plus whatever else stock analysis needs across records, e.g. a Vec<StockRow>
}

impl Handler for StockHandler {
    fn on_record(&mut self, _record: Record) -> Result<()> {
        self.record_count += 1;
        todo!("pull sku/quantity out of record, fold into self's accumulated state")
    }

    fn finish(&mut self, source_name: &str) -> Result<()> {
        debug!("{source_name}: processed {} records", self.record_count);
        todo!("analyze accumulated stock state, write result for source_name")
    }
}
