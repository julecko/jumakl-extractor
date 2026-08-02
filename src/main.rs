mod cli;
mod config;
mod logging;
mod output;
mod paths;
mod pipeline;
mod report;
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

    let mut reports = Vec::new();
    for mode in cli.modes() {
        let config = Config::load(cli.config_path(mode), mode)?;
        tracing::info!("Loaded {} suppliers for {:?}", config.sources.len(), mode);
        reports.extend(pipeline::run(mode, &config));
    }

    let elapsed = start.elapsed();
    tracing::info!("Program took {elapsed:?}");

    let summary = report::RunSummary { elapsed, reports };
    match report::create_reporter() {
        Ok(reporter) => {
            if let Err(err) = reporter.send(&summary) {
                tracing::error!("failed to send report email: {err:#}");
            }
        }
        Err(err) => tracing::error!("failed to set up reporter: {err:#}"),
    }

    Ok(())
}
