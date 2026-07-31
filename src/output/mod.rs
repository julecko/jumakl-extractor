mod csv;

use anyhow::Result;

use crate::cli::ExtractKind;

/// What a Handler has already derived from a Record and decided is ready to
/// export - not the raw parsed value. Writers never see business logic
/// (mismatch checks, buy/sell/fix math, ...), only the final shape to print.
pub enum WriteRow {
    Stock { n: i64 },
    Price { buy: f64, sell: f64, fix: f64 },
}

/// One implementor per output format (csv.rs, ...). Nothing outside this
/// module knows or cares which format is behind the trait object.
pub trait OutputWriter {
    fn write_record(
        &mut self,
        ean: &str,
        row: &WriteRow,
        shortname: &str,
        prefix: &str,
    ) -> Result<()>;
}

// Only place that matches on kind: picks which concrete writer to box up.
// Also the only place that picks a format - until a second output format
// exists, both concerns live in this one spot.
pub fn create_writer(kind: ExtractKind) -> Result<Box<dyn OutputWriter>> {
    match kind {
        ExtractKind::Stock => Ok(Box::new(csv::StockCsvWriter::create()?)),
        ExtractKind::Price => Ok(Box::new(csv::PriceCsvWriter::create()?)),
    }
}
