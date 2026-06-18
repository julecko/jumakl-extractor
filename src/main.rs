#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url: &str = "http://example.com";

    let response = match reqwest::get(url).await {
        Ok(resp) => resp,
        Err(err) => {
            eprintln!("Request failed: {err}");
            return Err(Box::<dyn std::error::Error>::from(err));
        }
    };

    println!("Status: {}", response.status());
    let body: String = match response.text().await {
        Ok(body) => body,
        Err(err) => {
            eprintln!("Getting request content failed");
            return Err(Box::<dyn std::error::Error>::from(err));
        }
    };

    println!("Content:");
    println!("{body}");


    return Ok(());
}
