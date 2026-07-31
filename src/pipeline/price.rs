use anyhow::Result;
use tracing::debug;

use super::{Handler, SourceReport};
use crate::output::WriteRow;
use crate::sources::Record;

#[derive(Default)]
pub struct PriceHandler {
    record_count: usize,
    // plus whatever else price analysis needs across records, e.g. a Vec<PriceMismatch>
}

impl Handler for PriceHandler {
    fn on_record(&mut self, _record: &Record) -> Result<Option<WriteRow>> {
        self.record_count += 1;
        todo!(
            "pull price out of record, compare against own logic, fold mismatches into self, \
             derive buy/sell/fix and return Ok(Some(WriteRow::Price {{ buy, sell, fix }}))"
        )
    }

    fn finish(&mut self, source_name: &str) -> SourceReport {
        debug!("{source_name}: processed {} records", self.record_count);
        todo!("build report from accumulated price state (e.g. mismatches into errors), return it")
    }
}
