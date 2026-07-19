mod config;
mod cli;
mod logging;

use std::error::Error;
use reqwest::blocking::Client;

use clap::Parser;
use dotenvy::dotenv;

use config::Config;
use cli::Cli;

fn fetch(client: &Client, url: &str) -> Result<String, Box<dyn Error>> {
    let response = client.get(url).send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    dotenv().ok();
    let _guard = logging::init(cli.verbose);
    let config = Config::load(cli.config_path())?;
    tracing::info!("Loaded {} suppliers", config.sources.len());

    println!("{:#?}", cli);
    println!("{:#?}", config);

    Ok(())
}
