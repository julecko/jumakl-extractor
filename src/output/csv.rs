use anyhow::{Context, Result};
use chrono::Local;
use std::fs::File;
use std::io::{BufWriter, Write};

use super::{OutputWriter, WriteRow};
use crate::paths::output_folder_path;

fn create_file(filename: &str) -> Result<BufWriter<File>> {
    let path = output_folder_path().join(filename);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {:?}", parent))?;
    }

    let file = File::create(&path).with_context(|| format!("failed to create {:?}", path))?;
    Ok(BufWriter::new(file))
}

pub struct StockCsvWriter {
    file: BufWriter<File>,
    date: String,
}

impl StockCsvWriter {
    pub fn create() -> Result<Self> {
        let mut file = create_file("massDataStock.csv")?;
        writeln!(file, "SOURCE;EAN;STOCK;DATE")?;

        Ok(Self {
            file,
            date: Local::now().format("%d.%m.%Y").to_string(),
        })
    }
}

impl OutputWriter for StockCsvWriter {
    fn write_record(
        &mut self,
        sku: &str,
        row: &WriteRow,
        shortname: &str,
        prefix: &str,
    ) -> Result<()> {
        // Only ever constructed for ExtractKind::Stock, so row is
        // always the Stock variant - nothing to branch on here.
        let WriteRow::Stock { n } = row else {
            anyhow::bail!("StockCsvWriter received a non-stock row");
        };

        writeln!(
            self.file,
            "{};{} - {};{n};{}",
            shortname, prefix, sku, self.date
        )?;
        Ok(())
    }
}

pub struct PriceCsvWriter {
    file: BufWriter<File>,
    date: String,
}

impl PriceCsvWriter {
    pub fn create() -> Result<Self> {
        let mut file = create_file("massDataPrice.csv")?;
        writeln!(file, "EAN;BUY_PRICE;SELL_PRICE;FIX_PRICE")?;

        Ok(Self {
            file,
            date: Local::now().format("%d.%m.%Y").to_string(),
        })
    }
}

impl OutputWriter for PriceCsvWriter {
    fn write_record(
        &mut self,
        sku: &str,
        row: &WriteRow,
        _shortname: &str,
        _prefix: &str,
    ) -> Result<()> {
        // Only ever constructed for ExtractKind::Price, so row is
        // always the Price variant - nothing to branch on here.
        let WriteRow::Price { buy, sell, fix } = row else {
            anyhow::bail!("PriceCsvWriter received a non-price row");
        };

        writeln!(self.file, "{sku};{buy};{sell};{fix}")?;
        Ok(())
    }
}
