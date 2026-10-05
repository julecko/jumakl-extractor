mod cli;
mod config;
mod logging;
mod output;
mod paths;
mod pipeline;
mod pricebook;
mod report;
mod sources;
mod webrequest;

use std::time::Instant;

use clap::Parser;
use dotenvy::dotenv;

use cli::Cli;
use config::{Config, ProgramConfig};

fn main() -> anyhow::Result<()> {
    let start = Instant::now();

    let cli = Cli::parse();
    dotenv().ok();

    let _guard = logging::init(cli.verbose);

    let mut reports = Vec::new();
    let mut program_errors = Vec::new();

    // Loaded once for the whole program, not per mode - see ProgramConfig.
    let program_config = match ProgramConfig::load(cli.program_config_path()) {
        Ok(program_config) => program_config,
        Err(err) => {
            tracing::error!("failed to load program config: {err:#}");
            program_errors.push(format!("failed to load program config: {err:#}"));
            ProgramConfig::default()
        }
    };

    // Compiled once here, not per record - see ProgramConfig::compiled_sku_exclusions.
    let sku_exclusions = match program_config.compiled_sku_exclusions() {
        Ok(patterns) => patterns,
        Err(err) => {
            tracing::error!("failed to compile excluded_sku_patterns: {err:#}");
            program_errors.push(format!("failed to compile excluded_sku_patterns: {err:#}"));
            Vec::new()
        }
    };

    // One reporter for both the price-fix mails and the status report.
    let reporter = match report::create_reporter() {
        Ok(reporter) => Some(reporter),
        Err(err) => {
            tracing::error!("failed to set up reporter: {err:#}");
            None
        }
    };

    for mode in cli.modes() {
        let config = match Config::load(cli.config_path(mode), mode) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("failed to load config for {mode:?}: {err:#}");
                program_errors.push(format!("{mode:?}: failed to load config: {err:#}"));
                continue;
            }
        };
        tracing::info!("Loaded {} suppliers for {:?}", config.sources.len(), mode);

        let result = pipeline::run(
            mode,
            &config,
            &sku_exclusions,
            program_config.pricebook.as_ref(),
        );
        // Separate mail, sent right after the price run and only when there
        // are fixes. A failure is recorded as a program error, so it can't
        // hide the rest of the run's results.
        if mode == cli::ExtractKind::Price
            && let Some(reporter) = &reporter
            && let Err(err) = reporter.send_price_fixes(&result.reports)
        {
            tracing::error!("failed to send price fix mail: {err:#}");
            program_errors.push(format!("Price: failed to send price fix mail: {err:#}"));
        }

        reports.extend(result.reports);
        program_errors.extend(result.program_errors);
    }

    let elapsed = start.elapsed();
    tracing::info!("Program took {elapsed:?}");

    let summary = report::RunSummary {
        elapsed,
        reports,
        program_errors,
    };
    if let Some(reporter) = &reporter
        && let Err(err) = reporter.send(&summary)
    {
        tracing::error!("failed to send report email: {err:#}");
    }

    Ok(())
}
