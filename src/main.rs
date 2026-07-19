mod config;
mod cli;

use std::error::Error;
use reqwest::blocking::Client;

use clap::Parser;

use config::Config;
use cli::Cli;

fn fetch(client: &Client, url: &str) -> Result<String, Box<dyn Error>> {
    let response = client.get(url).send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::load(cli.config_path())?;

    println!("{:#?}", cli);
    println!("{:#?}", config);

    Ok(())
}
