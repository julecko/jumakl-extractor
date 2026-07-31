use crate::config::AuthConfig;
use anyhow::Result;
use reqwest::blocking::Client;

pub fn fetch(client: &Client, url: &str, auth: Option<&AuthConfig>) -> Result<String> {
    let mut request = client.get(url);

    if let Some(auth) = auth {
        request = request.basic_auth(&auth.username, Some(&auth.password));
    }

    let response = request.send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}
