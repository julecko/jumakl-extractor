use anyhow::{Context, Result};
use chrono::Local;
use std::fs;
use std::path::Path;

use super::email::html_escape;
use crate::paths::templates_folder_path;
use crate::pipeline::{PriceFix, SourceReport};

/// The price-fix mail for one run. Built here, sent by the reporter.
pub struct PriceFixMail {
    pub subject: String,
    pub html: String,
}

/// `None` when there are no fixes, so a clean run sends nothing.
pub fn build(reports: &[SourceReport]) -> Result<Option<PriceFixMail>> {
    let fix_count: usize = reports.iter().map(|r| r.price_fixes.len()).sum();
    if fix_count == 0 {
        return Ok(None);
    }

    let supplier_count = reports.iter().filter(|r| !r.price_fixes.is_empty()).count();
    Ok(Some(PriceFixMail {
        subject: format!("Price fixes - {fix_count} product(s) in {supplier_count} supplier(s)"),
        html: render(reports)?,
    }))
}

/// Fills templates/price_fix/: mail.html is the shell, supplier.html is
/// repeated once per supplier with fixes, row.html once per product. Each
/// level is inserted last so its placeholders are never filled twice.
fn render(reports: &[SourceReport]) -> Result<String> {
    let folder = templates_folder_path().join("price_fix");
    let mail_template = read_template(&folder, "mail.html")?;
    let supplier_template = read_template(&folder, "supplier.html")?;
    let row_template = read_template(&folder, "row.html")?;

    let mut suppliers_html = String::new();
    let mut supplier_count = 0;
    let mut fix_count = 0;

    for report in reports.iter().filter(|r| !r.price_fixes.is_empty()) {
        supplier_count += 1;
        fix_count += report.price_fixes.len();

        let rows_html: String = report
            .price_fixes
            .iter()
            .map(|fix| render_row(&row_template, fix))
            .collect();

        suppliers_html.push_str(
            &supplier_template
                .replace("{{SUPPLIER}}", &html_escape(&report.supplier))
                .replace("{{COUNT}}", &report.price_fixes.len().to_string())
                .replace("{{ROWS}}", &rows_html),
        );
    }

    Ok(mail_template
        .replace("{{FIX_COUNT}}", &fix_count.to_string())
        .replace("{{SUPPLIER_COUNT}}", &supplier_count.to_string())
        .replace(
            "{{DATE}}",
            &Local::now().format("%d.%m.%Y %H:%M").to_string(),
        )
        .replace("{{SUPPLIERS}}", &suppliers_html))
}

fn render_row(template: &str, fix: &PriceFix) -> String {
    template
        .replace("{{SKU}}", &html_escape(&fix.sku))
        .replace("{{ORIGINAL}}", &format!("{:.2}", fix.original_price))
        .replace("{{NEW}}", &format!("{:.2}", fix.new_price))
        .replace("{{CHANGE}}", &change_percent(fix))
        .replace("{{CHANGE_COLOR}}", change_color(fix))
}

/// Change from the original to the new price, e.g. "+12.5%".
fn change_percent(fix: &PriceFix) -> String {
    if fix.original_price == 0.0 {
        return "n/a".to_string();
    }
    format!(
        "{:+.1}%",
        (fix.new_price / fix.original_price - 1.0) * 100.0
    )
}

/// Green when the new price is higher, red when lower (matches the legend in
/// mail.html). Grey when there's no original price to compare against.
fn change_color(fix: &PriceFix) -> &'static str {
    if fix.original_price == 0.0 {
        "#6b7280"
    } else if fix.new_price >= fix.original_price {
        "#15803d"
    } else {
        "#b91c1c"
    }
}

fn read_template(folder: &Path, name: &str) -> Result<String> {
    let path = folder.join(name);
    fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_fixes_and_skips_suppliers_without_fixes() {
        let reports = vec![
            SourceReport {
                supplier: "Automax".to_string(),
                records: 2,
                excluded: 0,
                errors: Vec::new(),
                price_fixes: vec![PriceFix {
                    sku: "AM - 6285".to_string(),
                    original_price: 4.06,
                    new_price: 3.75,
                }],
            },
            SourceReport {
                supplier: "Clean".to_string(),
                records: 5,
                excluded: 0,
                errors: Vec::new(),
                price_fixes: Vec::new(),
            },
        ];

        let html = render(&reports).expect("price_fix templates should render");

        assert!(html.contains("AM - 6285"));
        assert!(html.contains("-7.6%"));
        // 3.75 is lower than 4.06, so the change is red.
        assert!(html.contains("#b91c1c"));
        assert!(!html.contains("Clean"));
        assert!(!html.contains("{{"), "unfilled placeholder left in mail");
    }
}
