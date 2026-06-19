use quick_xml::Reader;
use quick_xml::events::Event;

#[derive(Debug)]
struct Product {
    ean: String,
    count: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url: &str = "https://www.auto-max.sk/media/export/produkty_sk.xml";

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
    
    let mut reader = Reader::from_str(&body);

    let mut items: Vec<Product> = Vec::new();

    let mut in_product = false;
    let mut current_tag: Option<&'static [u8]> = None;

    let mut buf: Vec<u8> = Vec::new();

    let mut current_ean = String::new();
    let mut current_count = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                match e.name().as_ref() {
                    b"Product" => {
                        in_product = true;
                        current_ean.clear();
                        current_count.clear();
                    }
                    b"Ean" | b"Sklad" => {
                        current_tag = Some(e.name().as_ref()) // Fix later with enum
                    }
                    _ => {}
                }
            }

            Ok(Event::Text(e)) => {
                if in_product {
                    match current_tag {
                        Some(b"EAN") => {
                            current_ean = e.decode().unwrap().into_owned();
                        }
                        Some(b"Sklad") => {
                            current_count = e.decode().unwrap().into_owned();
                        }
                        _ => {}
                    }
                }
            }

            Ok(Event::End(e)) => {
                match e.name().as_ref() {
                    b"Product" => {
                        in_product = false;

                        items.push(Product {
                            ean: current_ean.clone(),
                            count: current_count.parse::<i32>()?
                        });
                    }

                    b"EAN" | b"Sklad" => {
                        current_tag = None;
                    } 

                    _ => {}
                }
            }

            Ok(Event::Eof) => {
                break
            }
            Err(e) => {
                panic!("XML Parsing error {:?}", e);
            }
            _ => {}

        }
        buf.clear();
    }
    
    println!("Loaded products: {}", items.len());

    return Ok(());
}
