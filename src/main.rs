use std::error::Error;

use reqwest::blocking::Client;

fn fetch(client: &Client, url: &str) -> Result<String, Box<dyn Error>> {
    let response = client.get(url).send()?;
    let response = response.error_for_status()?;

    Ok(response.text()?)
}

fn main() {
    let client = Client::new();
    let url: &str = "https://www.auto-max.sk/media/export/produkty_sk.xml";
    match fetch(&client, url) {
        Ok(body) => {
            println!("Success {}", url);
            println!("{}", body);
        }

        Err(err) => {
            eprint!("Failed to fetch {}: {}", url, err);
        }
    }
}
