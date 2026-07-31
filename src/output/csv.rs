use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Write};

use super::OutputWriter;
use crate::cli::ExtractKind;
use crate::sources::{Record, RecordValue};

pub struct CsvWriter {
    file: BufWriter<File>,
}

impl CsvWriter {
    pub fn create(kind: ExtractKind) -> Result<Self> {
        let path = match kind {
            ExtractKind::Stock => "massDataStock.csv",
            ExtractKind::Price => "massDataPrice.csv",
        };
        let file = File::create(path).with_context(|| format!("failed to create {path}"))?;
        Ok(Self {
            file: BufWriter::new(file),
        })
    }
}

impl OutputWriter for CsvWriter {
    fn write_record(&mut self, record: &Record) -> Result<()> {
        match record.value {
            RecordValue::Stock(n) => writeln!(self.file, "{},{n}", record.ean)?,
            RecordValue::Price(p) => writeln!(self.file, "{},{p}", record.ean)?,
        }
        Ok(())
    }
}
