mod config;

use std::error::Error;
use reqwest::blocking::Client;
use config::Config;

fn fetch(client: &Client, url: &str) -> Result<String, Box<dyn Error>> {
    let response = client.get(url).send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}

fn main() -> anyhow::Result<()> {
    let config = Config::load("config/sources.stock.toml")?;

    println!("{:#?}", config);

    for source in &config.sources {
        println!("\n--- Source: {} ---", source.name);
        println!("URL: {}", source.url);

        match &source.format_config {
            config::FormatConfig::Xml(xml) => {
                println!("Format: XML (record_tag = {})", xml.xml.record_tag);
            }
            config::FormatConfig::Csv(csv) => {
                println!(
                    "Format: CSV (delimiter = {:?}, has_header = {})",
                    csv.csv.delimiter, csv.csv.has_header
                );
            }
        }

        println!("Fields:");
        for (field_name, mapping) in &source.fields {
            println!(
                "  {} <- selector \"{}\" ({:?})",
                field_name, mapping.selector, mapping.r#type
            );
        }
    }

    Ok(())
}
