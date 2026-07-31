use anyhow::Result;
use tracing::debug;

use super::Handler;
use crate::sources::Record;

#[derive(Default)]
pub struct PriceHandler {
    record_count: usize,
    // plus whatever else price analysis needs across records, e.g. a Vec<PriceMismatch>
}

impl Handler for PriceHandler {
    fn on_record(&mut self, _record: Record) -> Result<()> {
        self.record_count += 1;
        todo!("pull sku/price out of record, compare against own logic, fold mismatches into self")
    }

    fn finish(&mut self, source_name: &str) -> Result<()> {
        debug!("{source_name}: processed {} records", self.record_count);
        todo!("build report from accumulated price state, write result for source_name")
    }
}
