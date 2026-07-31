mod csv;

use anyhow::Result;

use crate::cli::ExtractKind;
use crate::sources::Record;

/// One implementor per output format (csv.rs, ...). Nothing outside this
/// module knows or cares which format is behind the trait object.
pub trait OutputWriter {
    fn write_record(&mut self, record: &Record, shortname: &str, prefix: &str) -> Result<()>;
}

// Only place that matches on kind for naming, and only place that picks a
// format - until a second output format exists, both live in this one spot.
pub fn create_writer(kind: ExtractKind) -> Result<Box<dyn OutputWriter>> {
    Ok(Box::new(csv::CsvWriter::create(kind)?))
}
