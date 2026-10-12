//! A generic scraper for sources whose "feed" is actually an HTML page: a
//! paginated listing of repeating blocks, plus optionally a handful of
//! standalone pages outside that listing. Every page-structure detail
//! (block markers, field markers, the pagination parameter, extra page
//! URLs) is config (see `HtmlListing1Options`/`HtmlField` in `config.rs`) -
//! nothing supplier-specific is hardcoded here.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;

use crate::config::{FieldMapping, HtmlField, HtmlListing1Config, HtmlListing1Options};
use crate::sources::{FormatParser, Record, RecordValue};
use crate::webrequest;

impl FormatParser for HtmlListing1Config {
    fn parse(
        &self,
        url: &str,
        content: &str,
        _fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        let opts = &self.html_listing1;

        // Own client, scoped to this one source: a format that needs its own
        // session (e.g. cookie-based login) can build one suited to that
        // here, without it ever being shared with any other source.
        let client =
            webrequest::build_client().context("failed to build html listing http client")?;

        match &opts.stop_marker {
            // No stop marker configured: `content` (the page already fetched
            // generically) is the only page there is.
            None => parse_block_page(content, opts, on_record)?,
            Some(stop_marker) => {
                let page_param = opts
                    .page_param
                    .as_deref()
                    .context("html_listing1 config needs 'page_param' when 'stop_marker' is set")?;

                // `content` is page 1, already fetched generically. Keep
                // parsing and fetching the next page until one comes back
                // containing the stop marker - that final, empty page is
                // fetched but not parsed.
                let mut page_url = url.to_string();
                let mut html = content.to_string();

                while !html.contains(stop_marker.as_str()) {
                    tracing::debug!("extracting page {page_url}");
                    parse_block_page(&html, opts, on_record)?;

                    page_url = increment_query_param(&page_url, page_param)?;
                    html = webrequest::fetch(&client, &page_url, None)
                        .with_context(|| format!("failed to fetch page {page_url}"))?;
                }
            }
        }

        for extra_url in &opts.extra_pages {
            tracing::debug!("extracting page {extra_url}");
            let page = webrequest::fetch(&client, extra_url, None)
                .with_context(|| format!("failed to fetch extra page {extra_url}"))?;
            emit_record(&page, &opts.extra_page_fields, on_record)?;
        }

        Ok(())
    }
}

/// Splits one listing page into record blocks and emits one record per block.
fn parse_block_page(
    html: &str,
    opts: &HtmlListing1Options,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
) -> Result<()> {
    let mut pos = 0;
    while let Some(start) = find_from(html, &opts.record_start, pos) {
        let Some(close) = find_block_close(html, start, &opts.nested_open, &opts.record_end) else {
            break;
        };
        let block = &html[start..close + opts.record_end.len()];

        emit_record(block, &opts.fields, on_record)?;

        pos = close + opts.record_end.len();
    }
    Ok(())
}

/// Finds the end of the block that opens at `start`, accounting for nested
/// elements of the same kind inside it (a naive "next close marker" would
/// close too early when a field's own markers nest inside the block).
fn find_block_close(html: &str, start: usize, nested_open: &str, close: &str) -> Option<usize> {
    let mut pos = start;
    let mut depth = 1;

    while depth > 0 {
        let next_open = find_from(html, nested_open, pos + 1);
        let next_close = find_from(html, close, pos + 1)?;

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

/// Extracts "sku" and "stock"/"price" out of `block` using `fields`, and
/// emits a record when a sku is found. A missing sku (no `default`
/// configured) means this block isn't a record worth keeping, not an error.
fn emit_record(
    block: &str,
    fields: &HashMap<String, HtmlField>,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
) -> Result<()> {
    let Some(sku) = fields
        .get("sku")
        .and_then(|field| extract_field(block, field))
    else {
        return Ok(());
    };
    if sku.is_empty() {
        return Ok(());
    }

    let value = if let Some(field) = fields.get("stock") {
        RecordValue::StockText(extract_field(block, field).unwrap_or_default())
    } else if let Some(field) = fields.get("price") {
        let text = extract_field(block, field).unwrap_or_default();
        let price = text
            .trim()
            .parse()
            .with_context(|| format!("'{text}' is not a decimal (sku={sku})"))?;
        RecordValue::Price(price)
    } else {
        bail!("html_listing1 fields must define 'stock' or 'price'");
    };

    on_record(Record {
        sku,
        value,
        extras: HashMap::new(),
    })
}

/// Runs one field's extraction, falling back to `field.default` (if any)
/// when the markers - or, with `value_prefix`/`value_suffix` set, the inner
/// value - aren't found.
fn extract_field(block: &str, field: &HtmlField) -> Option<String> {
    let raw = extract_between(
        block,
        &field.after,
        field.skip_closing_brackets,
        &field.until,
    );

    let value = match (&raw, &field.value_prefix, &field.value_suffix) {
        (Some(raw), Some(prefix), Some(suffix)) => extract_between(raw, prefix, 0, suffix),
        (Some(raw), _, _) => Some(raw.trim().to_string()),
        (None, _, _) => None,
    };

    value.or_else(|| field.default.clone())
}

/// Finds the first `after`, skips `skip_closing_brackets` '>' characters
/// past it, then captures up to the next `until`.
fn extract_between(
    text: &str,
    after: &str,
    skip_closing_brackets: usize,
    until: &str,
) -> Option<String> {
    let mut pos = text.find(after)? + after.len();
    for _ in 0..skip_closing_brackets {
        pos = find_from(text, ">", pos)? + 1;
    }

    let rest = &text[pos..];
    let end = rest.find(until)?;
    Some(rest[..end].trim().to_string())
}

fn find_from(text: &str, needle: &str, from: usize) -> Option<usize> {
    text.get(from..)
        .and_then(|rest| rest.find(needle))
        .map(|i| i + from)
}

/// Increments a `param=N` query parameter.
fn increment_query_param(url: &str, param: &str) -> Result<String> {
    let needle = format!("{param}=");
    let digits_start = url
        .find(&needle)
        .map(|pos| pos + needle.len())
        .with_context(|| format!("url has no '{needle}' parameter: {url}"))?;
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

    fn field(after: &str, until: &str) -> HtmlField {
        HtmlField {
            after: after.to_string(),
            skip_closing_brackets: 0,
            until: until.to_string(),
            value_prefix: None,
            value_suffix: None,
            default: None,
        }
    }

    fn field_with_default(after: &str, until: &str, default: &str) -> HtmlField {
        HtmlField {
            default: Some(default.to_string()),
            ..field(after, until)
        }
    }

    fn opts(fields: &[(&str, HtmlField)]) -> HtmlListing1Options {
        HtmlListing1Options {
            record_start: "<record>".to_string(),
            nested_open: "<record".to_string(),
            record_end: "</record>".to_string(),
            stop_marker: None,
            page_param: None,
            extra_pages: Vec::new(),
            fields: fields
                .iter()
                .map(|(name, field)| (name.to_string(), field.clone()))
                .collect(),
            extra_page_fields: HashMap::new(),
        }
    }

    fn block_records(html: &str, opts: &HtmlListing1Options) -> Vec<(String, String)> {
        let mut records = Vec::new();
        parse_block_page(html, opts, &mut |record| {
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
    fn parses_a_simple_record() {
        let opts = opts(&[
            ("sku", field("<code>", "</code>")),
            ("stock", field_with_default("<stock>", "</stock>", "0")),
        ]);
        let html = "<record><code>ABC123</code><stock>5</stock></record>";
        assert_eq!(
            block_records(html, &opts),
            vec![("ABC123".to_string(), "5".to_string())]
        );
    }

    #[test]
    fn handles_nested_elements_inside_a_record_block() {
        let opts = opts(&[
            ("sku", field("<code>", "</code>")),
            ("stock", field_with_default("<stock>", "</stock>", "0")),
        ]);
        let html = "<record><record class=\"wrapper\"><code>NEST1</code></record><stock>2</stock></record>";
        assert_eq!(
            block_records(html, &opts),
            vec![("NEST1".to_string(), "2".to_string())]
        );
    }

    #[test]
    fn uses_the_default_when_the_stock_field_is_missing() {
        let opts = opts(&[
            ("sku", field("<code>", "</code>")),
            ("stock", field_with_default("<stock>", "</stock>", "0")),
        ]);
        let html = "<record><code>OUT1</code></record>";
        assert_eq!(
            block_records(html, &opts),
            vec![("OUT1".to_string(), "0".to_string())]
        );
    }

    #[test]
    fn skips_records_with_no_sku() {
        let opts = opts(&[
            ("sku", field("<code>", "</code>")),
            ("stock", field_with_default("<stock>", "</stock>", "0")),
        ]);
        let html = "<record><stock>1</stock></record>";
        assert!(block_records(html, &opts).is_empty());
    }

    #[test]
    fn parses_two_consecutive_records() {
        let opts = opts(&[
            ("sku", field("<code>", "</code>")),
            ("stock", field_with_default("<stock>", "</stock>", "0")),
        ]);
        let html = "<record><code>A1</code><stock>1</stock></record><record><code>B2</code><stock>3</stock></record>";
        assert_eq!(
            block_records(html, &opts),
            vec![
                ("A1".to_string(), "1".to_string()),
                ("B2".to_string(), "3".to_string())
            ]
        );
    }

    #[test]
    fn extracts_a_value_between_a_prefix_and_suffix_inside_the_field() {
        let field = HtmlField {
            after: "<stock>".to_string(),
            skip_closing_brackets: 0,
            until: "</stock>".to_string(),
            value_prefix: Some("In stock:".to_string()),
            value_suffix: Some("units".to_string()),
            default: Some("0".to_string()),
        };
        assert_eq!(
            extract_field("<stock>In stock: 9 units</stock>", &field),
            Some("9".to_string())
        );
        assert_eq!(
            extract_field("<stock>not available</stock>", &field),
            Some("0".to_string())
        );
    }

    #[test]
    fn skips_closing_brackets_to_get_past_unpredictable_attributes() {
        let field = HtmlField {
            after: "data-role".to_string(),
            skip_closing_brackets: 2,
            until: "</span>".to_string(),
            value_prefix: None,
            value_suffix: None,
            default: None,
        };
        let html = "<span data-role=\"code\" data-x=\"y\"><span>SOLO1</span></span>";
        assert_eq!(extract_field(html, &field), Some("SOLO1".to_string()));
    }

    #[test]
    fn increments_the_named_query_parameter() {
        assert_eq!(
            increment_query_param("https://example.com/x?page=1", "page").unwrap(),
            "https://example.com/x?page=2"
        );
        assert_eq!(
            increment_query_param("https://example.com/x?page=9&foo=bar", "page").unwrap(),
            "https://example.com/x?page=10&foo=bar"
        );
    }

    #[test]
    fn increment_query_param_fails_without_that_parameter() {
        assert!(increment_query_param("https://example.com/x", "page").is_err());
    }
}
