use clap::{Parser, ValueEnum};

/// Extract and compare product data from supplier feeds
#[derive(Parser, Debug)]
#[command(name = "data_extractor", version, about)]
pub struct Cli {
    /// What kind of data to extract. If omitted, extracts everything.
    #[arg(long, value_enum)]
    pub extract: ExtractKind,

    /// Only run these specific sources, comma-separated (e.g. automax,supplier_b).
    /// If omitted, all sources in the config are run.
    #[arg(long, value_delimiter = ',')]
    pub sources: Option<Vec<String>>,

    /// Path to the sources config file. Defaults depend on --extract:
    /// config/stock.toml for stock, config/price.toml for price,
    /// config/sources.toml if --extract is omitted.
    #[arg(short, long)]
    pub config: Option<String>,

    /// Print extra logging
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractKind {
    Stock,
    Price,
}

impl Cli {
    pub fn config_path(&self) -> String {
        if let Some(path) = &self.config {
            return path.clone();
        }

        match self.extract {
            ExtractKind::Stock => "config/sources.stock.toml".to_string(),
            ExtractKind::Price => "config/sources.price.toml".to_string(),
        }
    }
}