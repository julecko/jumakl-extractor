mod cli;
mod config;
mod logging;
mod output;
mod paths;
mod pipeline;
mod sources;
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

    for mode in cli.modes() {
        let config = Config::load(cli.config_path(mode), mode)?;
        tracing::info!("Loaded {} suppliers for {:?}", config.sources.len(), mode);
        pipeline::run(mode, &config);
    }

    tracing::info!("Program took {:?} seconds", start.elapsed());
    Ok(())
}
