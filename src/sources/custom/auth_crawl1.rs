//! A generic scraper for cookie-authenticated catalog sites: log in once,
//! then crawl every category link off the site's nav menu, paginating each
//! one until a page repeats a product already seen (or comes back empty).
//! Every page-structure/markup detail (login form field names, nav/product
//! markers, the extraction patterns) is config (see `AuthCrawl1Options` in
//! `config.rs`) - nothing supplier-specific is hardcoded here. Credentials
//! are the one exception: they come from `AUTH_CRAWL1_USERNAME`/
//! `AUTH_CRAWL1_PASSWORD` in `.env`, never from a config file.

use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::Client;
use std::collections::HashMap;
use std::env;

use crate::config::{AuthCrawl1Config, AuthCrawl1Options, FieldMapping};
use crate::sources::{FormatParser, Record, RecordValue};
use crate::webrequest;

impl FormatParser for AuthCrawl1Config {
    fn parse(
        &self,
        url: &str,
        _content: &str,
        _fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        let opts = &self.auth_crawl1;

        // Own client with its own cookie jar, scoped to this one source - the
        // session from logging in here must never be shared with any other
        // source's requests.
        let client = Client::builder()
            .cookie_store(true)
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:128.0)")
            .build()
            .context("failed to build http client")?;

        authenticate(&client, opts)?;

        // The page fetched generically by run_source was unauthenticated, so
        // it can't be reused here - fetch the homepage again now that the
        // client actually has a logged-in session.
        let homepage = webrequest::fetch(&client, url, None)
            .context("failed to fetch homepage after login")?;
        let nav = extract_nav(&homepage, &opts.nav_start, &opts.nav_end)
            .context("homepage has no category nav")?;
        let links = extract_links(nav)?;

        for link in &links {
            let category_url = full_link(url, link);
            let count = crawl_category(&client, &category_url, opts, on_record)?;
            tracing::debug!("extracted {count} products from {category_url}");
        }

        Ok(())
    }
}

/// Logs in, storing the resulting session cookie in `client`'s cookie jar.
/// Replays the login page's real __VIEWSTATE (and, if present,
/// __VIEWSTATEGENERATOR/__EVENTVALIDATION) rather than a stale value, since
/// ASP.NET validates these server-side on postback.
fn authenticate(client: &Client, opts: &AuthCrawl1Options) -> Result<()> {
    let username =
        env::var("AUTH_CRAWL1_USERNAME").context("AUTH_CRAWL1_USERNAME not set in .env")?;
    let password =
        env::var("AUTH_CRAWL1_PASSWORD").context("AUTH_CRAWL1_PASSWORD not set in .env")?;

    let login_page =
        webrequest::fetch(client, &opts.login_url, None).context("failed to fetch login page")?;

    let viewstate = extract_hidden_field(&login_page, "__VIEWSTATE")
        .context("login page has no __VIEWSTATE field")?;

    let mut form = vec![
        ("__EVENTTARGET".to_string(), opts.event_target.clone()),
        ("__EVENTARGUMENT".to_string(), String::new()),
        ("__VIEWSTATE".to_string(), viewstate),
    ];
    if let Ok(generator) = extract_hidden_field(&login_page, "__VIEWSTATEGENERATOR") {
        form.push(("__VIEWSTATEGENERATOR".to_string(), generator));
    }
    if let Ok(validation) = extract_hidden_field(&login_page, "__EVENTVALIDATION") {
        form.push(("__EVENTVALIDATION".to_string(), validation));
    }
    form.push((opts.username_field.clone(), username));
    form.push((opts.password_field.clone(), password));

    let response = client
        .post(&opts.login_url)
        .header("Origin", opts.origin.clone())
        .header("Referer", opts.login_url.clone())
        .form(&form)
        .send()
        .context("failed to send login request")?;

    let landed_on_login_page = response.url().as_str() == opts.login_url;
    if !response.status().is_success() || landed_on_login_page {
        bail!(
            "login did not redirect away from the login page (status {})",
            response.status()
        );
    }

    Ok(())
}

/// Finds a hidden `<input>`'s `value` attribute by its `name`, regardless of
/// attribute order within the tag.
fn extract_hidden_field(html: &str, name: &str) -> Result<String> {
    let needle = format!("name=\"{name}\"");
    let name_pos = html
        .find(&needle)
        .with_context(|| format!("no field named '{name}' on the page"))?;
    let tag_start = html[..name_pos]
        .rfind("<input")
        .with_context(|| format!("could not find the start of the '{name}' input tag"))?;
    let tag_end = html[tag_start..]
        .find('>')
        .map(|i| i + tag_start)
        .with_context(|| format!("could not find the end of the '{name}' input tag"))?;
    let tag = &html[tag_start..tag_end];

    let value_start = tag
        .find("value=\"")
        .map(|i| i + "value=\"".len())
        .with_context(|| format!("'{name}' input tag has no value attribute"))?;
    let value_end = tag[value_start..]
        .find('"')
        .map(|i| i + value_start)
        .with_context(|| format!("'{name}' input tag's value attribute is not closed"))?;

    Ok(tag[value_start..value_end].to_string())
}

/// The nav block holding every category link, out of the full homepage.
fn extract_nav<'a>(html: &'a str, nav_start: &str, nav_end: &str) -> Option<&'a str> {
    let start = html.find(nav_start)?;
    let end = html[start..]
        .find(nav_end)
        .map(|i| i + start + nav_end.len())?;
    Some(&html[start..end])
}

/// Every `href` out of an `<a ...>` tag within `nav_html`. This pattern
/// itself is generic (any href-bearing anchor), not supplier-specific, so
/// unlike the rest of this module it isn't config-driven.
fn extract_links(nav_html: &str) -> Result<Vec<String>> {
    let link_re =
        Regex::new(r#"(?is)<a\s+[^>]*href\s*=\s*"(.*?)"[^>]*>"#).context("invalid link regex")?;
    Ok(link_re
        .captures_iter(nav_html)
        .map(|c| c[1].to_string())
        .collect())
}

/// Category links are site-relative (e.g. "/kategoria/xyz"); `base_url`
/// already ends in "/", so the link's own leading "/" is dropped.
fn full_link(base_url: &str, href: &str) -> String {
    format!("{base_url}{}", href.strip_prefix('/').unwrap_or(href))
}

/// Paginates one category starting at page 1, stopping as soon as a page
/// repeats a product already seen in this category (or comes back with
/// none), and emits every product accumulated up to that point.
fn crawl_category(
    client: &Client,
    category_url: &str,
    opts: &AuthCrawl1Options,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
) -> Result<usize> {
    let mut all_products: HashMap<String, String> = HashMap::new();

    for page_index in 1.. {
        let page_url = format!("{category_url}?page={page_index}");
        tracing::debug!("extracting page {page_url}");

        let page_content = webrequest::fetch(client, &page_url, None)
            .with_context(|| format!("failed to fetch page {page_url}"))?;
        let new_products = process_page(&page_content, opts)?;

        if new_products.is_empty()
            || new_products
                .keys()
                .any(|code| all_products.contains_key(code))
        {
            break;
        }

        all_products.extend(new_products);
    }

    for (code, quantity) in &all_products {
        on_record(Record {
            sku: code.clone(),
            value: RecordValue::StockText(quantity.clone()),
            extras: HashMap::new(),
        })?;
    }

    Ok(all_products.len())
}

/// Extracts (code, quantity) pairs from one category page's product table.
/// A code can appear only once; later duplicates on the same page lose to
/// the first occurrence.
fn process_page(page: &str, opts: &AuthCrawl1Options) -> Result<HashMap<String, String>> {
    let mut products = HashMap::new();

    let Some(start) = page.find(opts.products_start.as_str()) else {
        return Ok(products);
    };
    let Some(end) = page[start..]
        .find(opts.products_end.as_str())
        .map(|i| i + start + opts.products_end.len())
    else {
        return Ok(products);
    };
    let section = &page[start..end];

    let id_re = Regex::new(&opts.id_pattern).context("invalid id_pattern regex")?;
    let qty_re = Regex::new(&opts.quantity_pattern).context("invalid quantity_pattern regex")?;

    let mut rest = section;
    while let Some(id_match) = id_re.find(rest) {
        let code = id_re
            .captures(rest)
            .context("id_pattern match had no capture group")?[1]
            .trim()
            .to_string();
        let after_id = &rest[id_match.end()..];

        match qty_re.find(after_id) {
            Some(qty_match) => {
                let quantity = qty_re
                    .captures(after_id)
                    .context("quantity_pattern match had no capture group")?[1]
                    .trim()
                    .to_string();
                products.entry(code).or_insert(quantity);
                rest = &after_id[qty_match.end()..];
            }
            None => rest = after_id,
        }
    }

    Ok(products)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> AuthCrawl1Options {
        AuthCrawl1Options {
            login_url: "https://example.com/login".to_string(),
            origin: "https://example.com".to_string(),
            event_target: "login_button".to_string(),
            username_field: "username".to_string(),
            password_field: "password".to_string(),
            nav_start: "<nav>".to_string(),
            nav_end: "</nav>".to_string(),
            products_start: "PRODUCTS_START".to_string(),
            products_end: "PRODUCTS_END".to_string(),
            id_pattern: r#"<a href="[^"]*">([^<]+)</a>"#.to_string(),
            quantity_pattern: r#"<qty>([^<]+)</qty>"#.to_string(),
        }
    }

    #[test]
    fn extracts_a_hidden_fields_value_regardless_of_attribute_order() {
        let html =
            r#"<input type="hidden" name="__VIEWSTATE" id="__VIEWSTATE" value="abc123==" />"#;
        assert_eq!(
            extract_hidden_field(html, "__VIEWSTATE").unwrap(),
            "abc123=="
        );

        let html_value_first = r#"<input value="xyz789" type="hidden" name="__EVENTVALIDATION" />"#;
        assert_eq!(
            extract_hidden_field(html_value_first, "__EVENTVALIDATION").unwrap(),
            "xyz789"
        );
    }

    #[test]
    fn missing_hidden_field_is_an_error() {
        assert!(extract_hidden_field("<html></html>", "__VIEWSTATE").is_err());
    }

    #[test]
    fn extracts_the_nav_block() {
        let html = concat!(
            "<header>before</header>",
            "<nav>",
            "<a href=\"/one\">One</a>",
            "</nav>",
            "<footer>after</footer>"
        );
        let nav = extract_nav(html, "<nav>", "</nav>").unwrap();
        assert!(nav.contains("/one"));
        assert!(!nav.contains("before"));
        assert!(!nav.contains("after"));
    }

    #[test]
    fn extracts_every_link_in_the_nav() {
        let nav = r#"<a class="x" href="/cat-one">Cat One</a><a href="/cat-two" target="_blank">Cat Two</a>"#;
        assert_eq!(extract_links(nav).unwrap(), vec!["/cat-one", "/cat-two"]);
    }

    #[test]
    fn full_link_drops_a_single_leading_slash() {
        assert_eq!(
            full_link("https://example.com/", "/kategoria/xyz"),
            "https://example.com/kategoria/xyz"
        );
    }

    #[test]
    fn parses_products_out_of_the_section() {
        let opts = opts();
        let page = format!(
            "junk before {} <a href=\"/p/1\">CODE1</a> stuff <qty>5</qty> \
             <a href=\"/p/2\">CODE2</a> stuff <qty>3</qty> {} junk after",
            opts.products_start, opts.products_end
        );
        let products = process_page(&page, &opts).unwrap();
        assert_eq!(products.get("CODE1"), Some(&"5".to_string()));
        assert_eq!(products.get("CODE2"), Some(&"3".to_string()));
        assert_eq!(products.len(), 2);
    }

    #[test]
    fn ignores_an_id_with_no_following_quantity() {
        let opts = opts();
        let page = format!(
            "{} <a href=\"/p/1\">CODE1</a> no quantity here {}",
            opts.products_start, opts.products_end
        );
        assert!(process_page(&page, &opts).unwrap().is_empty());
    }

    #[test]
    fn first_occurrence_of_a_duplicate_code_wins() {
        let opts = opts();
        let page = format!(
            "{} <a href=\"/p/1\">CODE1</a> <qty>5</qty> <a href=\"/p/1\">CODE1</a> <qty>9</qty> {}",
            opts.products_start, opts.products_end
        );
        let products = process_page(&page, &opts).unwrap();
        assert_eq!(products.get("CODE1"), Some(&"5".to_string()));
        assert_eq!(products.len(), 1);
    }

    #[test]
    fn missing_products_section_yields_nothing() {
        assert!(
            process_page("<html>no markers here</html>", &opts())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn invalid_id_pattern_is_an_error() {
        let mut bad_opts = opts();
        bad_opts.id_pattern = "(unclosed".to_string();
        let page = format!("{} x {}", bad_opts.products_start, bad_opts.products_end);
        assert!(process_page(&page, &bad_opts).is_err());
    }
}
