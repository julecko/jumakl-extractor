use crate::config::AuthConfig;
use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, HeaderMap, HeaderValue};

pub fn build_client() -> Result<Client, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

    Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .default_headers(headers)
        .build()
}

pub fn fetch(client: &Client, url: &str, auth: Option<&AuthConfig>) -> Result<String> {
    let mut request = client.get(url);

    if let Some(auth) = auth {
        request = request.basic_auth(&auth.username, Some(&auth.password));
    }

    let response = request.send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}
