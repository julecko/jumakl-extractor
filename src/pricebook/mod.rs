use std::collections::HashMap;

use anyhow::{Result, bail};
use reqwest::blocking::Client;

use crate::config::PriceBookConfig;
use crate::webrequest;

/// Our own price for one SKU, from the PriceBook feed.
pub struct PriceReference {
    pub original_price: f64,
    pub profit_euro: f64,
    pub profit_percent: f64,
}

/// Keyed by the full SKU as the feed writes it, prefix included (e.g. "PRE - 6285").
pub struct PriceBook(HashMap<String, PriceReference>);

impl PriceBook {
    pub fn get(&self, key: &str) -> Option<&PriceReference> {
        self.0.get(key)
    }
}

pub fn load(client: &Client, config: &PriceBookConfig) -> Result<PriceBook> {
    let content = webrequest::fetch(client, &config.url, None)?;
    let book = parse(&content);

    // An empty book would make every supplier record unmatched and the run
    // would look successful, so treat it as a failure instead.
    if book.0.is_empty() {
        bail!("pricebook at {} contained no usable items", config.url);
    }

    Ok(book)
}

/// The feed isn't real XML: each <item> holds bracketed pseudo-tags such as
/// `[sku]PRE - 6285[/sku]`, so it's read as text.
fn parse(content: &str) -> PriceBook {
    let mut map = HashMap::new();

    for item in content.split("<item>").skip(1) {
        let body = item.split("</item>").next().unwrap_or(item);

        let Some(sku) = field(body, "sku") else {
            continue;
        };
        // Without an original price there's nothing to compare against.
        let Some(original_price) = field(body, "price")
            .filter(|text| !text.is_empty())
            .and_then(|text| text.parse().ok())
        else {
            continue;
        };
        let (Some(profit_euro), Some(profit_percent)) = (
            optional_number(body, "price_zisk_euro"),
            optional_number(body, "price_zisk_percent"),
        ) else {
            tracing::debug!("pricebook item {sku} has an unreadable discount value, skipping");
            continue;
        };

        // First occurrence wins if the feed lists a SKU twice.
        map.entry(sku.to_string()).or_insert(PriceReference {
            original_price,
            profit_euro,
            profit_percent,
        });
    }

    PriceBook(map)
}

/// Text between `[name]` and `[/name]`, trimmed.
fn field<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("[{name}]");
    let close = format!("[/{name}]");

    let start = body.find(&open)? + open.len();
    let len = body[start..].find(&close)?;
    Some(body[start..start + len].trim())
}

/// A missing or empty discount counts as 0. `None` means the value is present
/// but isn't a number.
fn optional_number(body: &str, name: &str) -> Option<f64> {
    match field(body, name) {
        Some(text) if !text.is_empty() => text.parse().ok(),
        _ => Some(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bracketed_items() {
        let feed = "<rss><channel>\
            <item>\n [sku]PRE - 6285[/sku]\n [price]4.06[/price]\n \
            [price_nakup]1.27[/price_nakup]\n \
            [price_zisk_euro]2.000000[/price_zisk_euro]\n \
            [price_zisk_percent]5.000000[/price_zisk_percent]\n</item>\
            <item>[sku]PRE - 1[/sku][price][/price]</item>\
            <item>[sku]PRE - 2[/sku][price]10[/price]</item>\
            </channel></rss>";

        let book = parse(feed);

        let item = book.get("PRE - 6285").expect("item should be parsed");
        assert_eq!(item.original_price, 4.06);
        assert_eq!(item.profit_euro, 2.0);
        assert_eq!(item.profit_percent, 5.0);

        // No price -> skipped.
        assert!(book.get("PRE - 1").is_none());
        // Missing discounts default to 0.
        let no_discount = book
            .get("PRE - 2")
            .expect("item without discounts should be parsed");
        assert_eq!(no_discount.profit_euro, 0.0);
        assert_eq!(no_discount.profit_percent, 0.0);
    }
}
