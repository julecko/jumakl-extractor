//! A paginated HTML product listing, plus a handful of standalone product
//! pages not reachable through the listing. Everything about this page's
//! structure is hardcoded here rather than config-driven, since it differs
//! per source that needs it.

use anyhow::{Context, Result};

use crate::sources::{Record, RecordValue};
use crate::webrequest;

const PRODUCT_START: &str = "<div class=\"item item-card shadow-around\">";
const PRODUCT_END: &str = "</div>";
const CODE_START: &str = "<div class=\"item-code\">";
const CODE_END: &str = "</div>";
const STOCK_START: &str = "<div class=\"item-stock \"";
const STOCK_END: &str = "</div>";
const NO_PRODUCTS: &str = "<div class=\"title\">Nenájdené žiadne produkty</div>";

/// Standalone product pages not reachable through the paginated listing at all.
const EXTRA_URLS: &[&str] = &[
    "https://www.meva.sk/plynova-pec-verona-P/",
    "https://www.meva.sk/naftovy-teplogenerator-20-kw-P/",
    "https://www.meva.sk/varic-rivet-P/",
    "https://www.meva.sk/stojan-na-plynovu-kartusu-P/",
    "https://www.meva.sk/liehovy-varic-P/",
    "https://www.meva.sk/varic-tripper-P/",
    "https://www.meva.sk/varic-single-deluxe-P/",
    "https://www.meva.sk/rost-ku-grilom-pelvis-a-rascal-P/",
    "https://www.meva.sk/panvica-ku-grilom-pelvis-a-rascal-P/",
    "https://www.meva.sk/grilovaci-rost-s-regulaciou-P/",
    "https://www.meva.sk/varic-2horak-orlik-s-vekom-vysokotlakovy-P/",
    "https://www.meva.sk/sklenena-trubica-k-ohrievacu-hexagon-P/",
    "https://www.meva.sk/varic-orlik-bez-veka-termoel-poistka-P/",
    "https://www.meva.sk/varic-2horak-orlik-bez-veka-vysokotlakovy-P/",
    "https://www.meva.sk/-3-P/",
    "https://www.meva.sk/zatka-k-2-kg-pb-flasi-P/",
    "https://www.meva.sk/set-pripojovaci-P/",
    "https://www.meva.sk/pasca-na-komare-bg-protector-P/",
    "https://www.meva.sk/gril-sahara-s350-P/",
    "https://www.meva.sk/varic-thunder-dual-P/",
];

pub fn parse(
    url: &str,
    content: &str,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
) -> Result<()> {
    // Own client, scoped to this one source - a future handler needing its
    // own session (e.g. cookie-based login) can build one suited to that
    // here, without it ever being shared with any other source.
    let client = webrequest::build_client().context("failed to build http client")?;

    // `content` is page 1, already fetched generically by run_source. Parse
    // it, then keep fetching/parsing the next page until one comes back with
    // the "no products" marker - that final, empty page is fetched but not
    // parsed.
    let mut page_url = url.to_string();
    let mut html = content.to_string();

    while !html.contains(NO_PRODUCTS) {
        parse_listing_page(&html, on_record)?;

        page_url = increment_page(&page_url)?;
        html = webrequest::fetch(&client, &page_url, None)
            .with_context(|| format!("failed to fetch page {page_url}"))?;
    }

    for extra_url in EXTRA_URLS {
        let page = webrequest::fetch(&client, extra_url, None)
            .with_context(|| format!("failed to fetch extra page {extra_url}"))?;
        parse_solo_page(&page, on_record)?;
    }

    Ok(())
}

fn parse_listing_page(html: &str, on_record: &mut dyn FnMut(Record) -> Result<()>) -> Result<()> {
    let mut pos = 0;
    while let Some(start) = find_from(html, PRODUCT_START, pos) {
        let Some(close) = find_product_close(html, start) else {
            break;
        };
        let block = &html[start..close + PRODUCT_END.len()];

        if let Some(code) = extract_code(block) {
            let quantity = extract_quantity(block);
            on_record(Record {
                sku: code,
                value: RecordValue::StockText(quantity),
                extras: Default::default(),
            })?;
        }

        pos = close + PRODUCT_END.len();
    }
    Ok(())
}

/// Finds the end of the product `<div>` that opens at `start`, accounting for
/// `<div>`s nested inside it (a naive "next `</div>`" would close too early,
/// since the code/stock divs are themselves nested inside the product div).
fn find_product_close(html: &str, start: usize) -> Option<usize> {
    let mut pos = start;
    let mut depth = 1;

    while depth > 0 {
        let next_open = find_from(html, "<div", pos + 1);
        let next_close = find_from(html, PRODUCT_END, pos + 1)?;

        match next_open {
            Some(open) if open < next_close => {
                depth += 1;
                pos = open;
            }
            _ => {
                depth -= 1;
                pos = next_close;
            }
        }
    }
    Some(pos)
}

fn extract_code(block: &str) -> Option<String> {
    let after_start = block.find(CODE_START)? + CODE_START.len();
    let rest = &block[after_start..];
    let end = rest.find(CODE_END)?;
    let code = rest[..end].trim();
    (!code.is_empty()).then(|| code.to_string())
}

fn extract_quantity(block: &str) -> String {
    extract_quantity_inner(block).unwrap_or_else(|| "0".to_string())
}

fn extract_quantity_inner(block: &str) -> Option<String> {
    let stock_div = block.find(STOCK_START)?;
    let after_tag = block[stock_div..].find('>')? + stock_div + 1;
    let rest = &block[after_tag..];
    let end = rest.find(STOCK_END)?;
    let stock_text = rest[..end].trim();

    if !stock_text.contains("Skladom") {
        return None;
    }
    let qty_start = stock_text.find("Skladom")? + "Skladom".len();
    let qty_rest = &stock_text[qty_start..];
    let qty_end = qty_rest.find("ks")?;
    Some(qty_rest[..qty_end].trim().to_string())
}

fn parse_solo_page(html: &str, on_record: &mut dyn FnMut(Record) -> Result<()>) -> Result<()> {
    let Some(record) = extract_solo_record(html) else {
        return Ok(());
    };
    on_record(record)
}

fn extract_solo_record(html: &str) -> Option<Record> {
    let tag_pos = html.find("product-type")?;
    let first_gt = html[tag_pos..].find('>')? + tag_pos;
    let second_gt = html[first_gt + 1..].find('>')? + first_gt + 1;
    let span_end = html[second_gt + 1..].find("</span>")? + second_gt + 1;
    let code = html[second_gt + 1..span_end].to_string();

    let stock_pos = html.find("Skladom")?;
    let qty_start = stock_pos + "Skladom".len();
    let qty_end = html[qty_start..].find(" ks")? + qty_start;
    let quantity = html[qty_start..qty_end].to_string();

    Some(Record {
        sku: code,
        value: RecordValue::StockText(quantity),
        extras: Default::default(),
    })
}

fn find_from(html: &str, needle: &str, from: usize) -> Option<usize> {
    html.get(from..)
        .and_then(|rest| rest.find(needle))
        .map(|i| i + from)
}

/// Increments the `page=N` query parameter.
fn increment_page(url: &str) -> Result<String> {
    let digits_start = url
        .find("page=")
        .map(|pos| pos + "page=".len())
        .with_context(|| format!("url has no 'page=' parameter: {url}"))?;
    let digits_end = url[digits_start..]
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| i + digits_start)
        .unwrap_or(url.len());
    let current: u32 = url[digits_start..digits_end].parse()?;
    Ok(format!(
        "{}{}{}",
        &url[..digits_start],
        current + 1,
        &url[digits_end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing_records(html: &str) -> Vec<(String, String)> {
        let mut records = Vec::new();
        parse_listing_page(html, &mut |record| {
            let RecordValue::StockText(quantity) = record.value else {
                unreachable!()
            };
            records.push((record.sku, quantity));
            Ok(())
        })
        .unwrap();
        records
    }

    #[test]
    fn parses_a_simple_product_listing() {
        let html = r#"<div class="item item-card shadow-around"><div class="item-code">ABC123</div><div class="item-stock ">Skladom 5 ks</div></div>"#;
        assert_eq!(
            listing_records(html),
            vec![("ABC123".to_string(), "5".to_string())]
        );
    }

    #[test]
    fn handles_divs_nested_inside_a_product_block() {
        let html = r#"<div class="item item-card shadow-around"><div class="wrapper"><div class="item-code">NEST1</div></div><div class="item-stock ">Skladom 2 ks</div></div>"#;
        assert_eq!(
            listing_records(html),
            vec![("NEST1".to_string(), "2".to_string())]
        );
    }

    #[test]
    fn defaults_to_zero_stock_when_not_in_stock() {
        let html = r#"<div class="item item-card shadow-around"><div class="item-code">OUT1</div><div class="item-stock ">Nie je skladom</div></div>"#;
        assert_eq!(
            listing_records(html),
            vec![("OUT1".to_string(), "0".to_string())]
        );
    }

    #[test]
    fn skips_products_with_no_code() {
        let html = r#"<div class="item item-card shadow-around"><div class="item-stock ">Skladom 1 ks</div></div>"#;
        assert!(listing_records(html).is_empty());
    }

    #[test]
    fn parses_two_consecutive_products() {
        let html = r#"<div class="item item-card shadow-around"><div class="item-code">A1</div><div class="item-stock ">Skladom 1 ks</div></div><div class="item item-card shadow-around"><div class="item-code">B2</div><div class="item-stock ">Skladom 3 ks</div></div>"#;
        assert_eq!(
            listing_records(html),
            vec![
                ("A1".to_string(), "1".to_string()),
                ("B2".to_string(), "3".to_string())
            ]
        );
    }

    #[test]
    fn extracts_a_solo_product_page() {
        let html = r#"junk <span class="product-type" data-x="y"><span>SOLO1</span></span> more... Skladom 9 ks blah"#;
        let record = extract_solo_record(html).unwrap();
        assert_eq!(record.sku, "SOLO1");
        let RecordValue::StockText(quantity) = record.value else {
            unreachable!()
        };
        assert_eq!(quantity, "9");
    }

    #[test]
    fn solo_page_with_no_product_type_tag_yields_nothing() {
        assert!(extract_solo_record("<html>nothing here</html>").is_none());
    }

    #[test]
    fn increments_the_page_query_parameter() {
        assert_eq!(
            increment_page("https://example.com/x/?page=1").unwrap(),
            "https://example.com/x/?page=2"
        );
        assert_eq!(
            increment_page("https://example.com/x/?page=9&foo=bar").unwrap(),
            "https://example.com/x/?page=10&foo=bar"
        );
    }

    #[test]
    fn increment_page_fails_without_a_page_parameter() {
        assert!(increment_page("https://example.com/x/").is_err());
    }
}
