use crate::cli::ExtractKind;
use crate::config::Config;

pub fn run(kind: ExtractKind, config: &Config) {
    tracing::debug!("Starting extraction");

    // One big loop iterating over sources, each loop downloads content and passes it to handler
    //loop {
        match kind {
            ExtractKind::Stock => handle_stock(),
            ExtractKind::Price => handle_price(),
        }
    //}
    // Send data to server to store for analytics and also send email with price data which to update
}


// Both handlers handle formats like xml, csv, etc...

fn handle_stock() {
    // Extract stock from data, return struct
}

fn handle_price() {
    // Extract price from data, return struct
    // In data parsing also analyze data and pass it back 
}