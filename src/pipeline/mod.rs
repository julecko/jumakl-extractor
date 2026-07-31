mod price;
mod stock;

use crate::cli::ExtractKind;
use crate::config::Config;

pub fn run(kind: ExtractKind, config: &Config) {
    tracing::debug!("Starting extraction");

    // One big loop iterating over sources, each loop downloads content and passes it to handler
    for source in &config.sources {
        match kind {
            ExtractKind::Stock => stock::handle(),
            ExtractKind::Price => price::handle(),
        }
    }
    //}
    // Send data to server to store for analytics and also send email with price data which to update
}

// Both handlers handle formats like xml, csv, etc...
