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
    /// config/sources.stock.toml for stock, config/sources.price.toml for
    /// price. Ignored if --extract is omitted, since both modes would run.
    #[arg(long)]
    pub source_config: Option<PathBuf>,

    /// Path to the whole-program config file. Defaults to config/program.toml.
    #[arg(long)]
    pub program_config: Option<PathBuf>,

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
    /// The config `fields` key this kind requires alongside "sku".
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

        self.source_config.clone().unwrap_or(default)
    }

    /// Whole-program config, independent of --extract - always this one file
    /// (or --program-config's override), unlike config_path(kind) which
    /// varies per mode.
    pub fn program_config_path(&self) -> PathBuf {
        self.program_config
            .clone()
            .unwrap_or_else(|| config_folder_path().join("program.toml"))
    }
}
