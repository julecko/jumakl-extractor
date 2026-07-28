use anyhow::Result;
use reqwest::blocking::Client;

pub fn fetch(client: &Client, url: &str) -> Result<String> {
    let response = client.get(url).send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}
