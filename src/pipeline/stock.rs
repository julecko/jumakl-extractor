use anyhow::Result;
use tracing::{debug, info};

use super::{Handler, SourceReport};
use crate::output::WriteRow;
use crate::sources::Record;

#[derive(Default)]
pub struct StockHandler {
    record_count: usize,
    // plus whatever else stock analysis needs across records, e.g. a Vec<StockRow>
}

impl Handler for StockHandler {
    fn on_record(&mut self, record: &Record) -> Result<Option<WriteRow>> {
        self.record_count += 1;
        debug!("{:?}", record);

        let Some(n) = record.value.as_stock() else {
            anyhow::bail!("StockHandler received a non-stock record");
        };

        Ok(Some(WriteRow::Stock { n }))
    }

    fn finish(&mut self, source_name: &str) -> SourceReport {
        info!("{source_name}: processed {} records", self.record_count);
        SourceReport {
            supplier: source_name.to_string(),
            records: self.record_count,
            errors: Vec::new(),
        }
    }
}
