mod cli;
mod config;
mod helpers;
mod logging;
mod pipeline;
mod webrequest;

use std::time::Instant;

use clap::Parser;
use dotenvy::dotenv;

use cli::Cli;
use config::Config;

fn main() -> anyhow::Result<()> {
    let start = Instant::now();

    let cli = Cli::parse();
    dotenv().ok();

    let _guard = logging::init(cli.verbose);
    let config = Config::load(cli.config_path())?;

    tracing::info!("Loaded {} suppliers", config.sources.len());

    pipeline::run(cli.extract, &config);

    tracing::info!("Program took {:?} seconds", start.elapsed());
    Ok(())
}
