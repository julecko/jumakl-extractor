use anyhow::{Context, Result};
use chrono::Local;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::{panic, writeln};

use super::OutputWriter;
use crate::cli::ExtractKind;
use crate::paths::output_folder_path;
use crate::sources::{Record, RecordValue};

pub struct CsvWriter {
    file: BufWriter<File>,
    date: String,
}

impl CsvWriter {
    pub fn create(kind: ExtractKind) -> Result<Self> {
        let path = match kind {
            ExtractKind::Stock => output_folder_path().join("massDataStock.csv"),
            ExtractKind::Price => output_folder_path().join("massDataPrice.csv"),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {:?}", parent))?;
        }

        let file = File::create(&path).with_context(|| format!("failed to create {:?}", path))?;
        let mut file_handle = BufWriter::new(file);
        match kind {
            ExtractKind::Stock => writeln!(file_handle, "SOURCE;EAN;STOCK;DATE")?,
            ExtractKind::Price => writeln!(file_handle, "EAN;BUY_PRICE;SELL_PRICE;FIX_PRICE")?,
        };

        Ok(Self {
            file: file_handle,
            date: Local::now().format("%d.%m.%Y").to_string(),
        })
    }
}

impl OutputWriter for CsvWriter {
    fn write_record(&mut self, record: &Record, shortname: &str, prefix: &str) -> Result<()> {
        match record.value {
            RecordValue::Stock(n) => writeln!(
                self.file,
                "{};{} - {};{n};{}",
                shortname, prefix, record.ean, self.date
            )?,
            RecordValue::Price(p) => panic!("Not implemented yet"),
        }
        Ok(())
    }
}
