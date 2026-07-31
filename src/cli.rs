use std::path::PathBuf;

use crate::paths::config_folder_path;
use clap::{Parser, ValueEnum};

/// Extract and compare product data from supplier feeds
#[derive(Parser, Debug)]
#[command(name = "data_extractor", version, about)]
pub struct Cli {
    /// What kind of data to extract. If omitted, extracts everything.
    #[arg(long, value_enum)]
    pub extract: Option<ExtractKind>,

    /// Only run these specific sources, comma-separated (e.g. automax,supplier_b).
    /// If omitted, all sources in the config are run.
    #[arg(long, value_delimiter = ',')]
    pub sources: Option<Vec<String>>,

    /// Path to the sources config file. Defaults depend on --extract:
    /// config/stock.toml for stock, config/price.toml for price,
    /// config/sources.toml if --extract is omitted.
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Print extra logging
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractKind {
    Stock,
    Price,
}

impl ExtractKind {
    /// The config `fields` key this kind requires alongside "ean".
    pub fn value_field(&self) -> &'static str {
        match self {
            ExtractKind::Stock => "stock",
            ExtractKind::Price => "price",
        }
    }
}

impl Cli {
    pub fn modes(&self) -> Vec<ExtractKind> {
        match self.extract {
            Some(kind) => vec![kind],
            None => vec![ExtractKind::Stock, ExtractKind::Price],
        }
    }
    pub fn config_path(&self, kind: ExtractKind) -> PathBuf {
        let default = match kind {
            ExtractKind::Stock => config_folder_path().join("sources.stock.toml"),
            ExtractKind::Price => config_folder_path().join("sources.price.toml"),
        };

        if self.extract.is_none() {
            return default;
        }

        self.config.clone().unwrap_or(default)
    }
}
