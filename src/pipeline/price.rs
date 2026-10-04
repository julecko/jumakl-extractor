use anyhow::{Result, bail};
use tracing::{debug, info};

use super::{Handler, SourceReport};
use crate::config::SourceConfig;
use crate::output::WriteRow;
use crate::pricebook::{PriceBook, PriceReference};
use crate::sources::Record;

const VAT: f64 = 1.23;

/// Port of the price comparison in massExtraction's PriceExtrAndCalc.cpp.
/// Records whose SKU isn't in the PriceBook are skipped (Ok(None)), like the C++.
pub struct PriceHandler<'a> {
    source: &'a SourceConfig,
    pricebook: &'a PriceBook,
    record_count: usize,
    unmatched_count: usize,
    fix_count: usize,
}

impl<'a> PriceHandler<'a> {
    pub fn new(source: &'a SourceConfig, pricebook: &'a PriceBook) -> Self {
        Self {
            source,
            pricebook,
            record_count: 0,
            unmatched_count: 0,
            fix_count: 0,
        }
    }
}

impl Handler for PriceHandler<'_> {
    fn on_record(&mut self, record: &Record) -> Result<Option<WriteRow>> {
        self.record_count += 1;

        let Some(supplier_price) = record.value.as_price() else {
            bail!("PriceHandler received a non-price record");
        };

        // TODO: Automax discount flag (C++ label "Nepodlieha_zlave"). When it is
        // "0", the C++ multiplies the supplier price by 0.8 before removing VAT.
        // Record doesn't carry that field yet, so the discount is not applied.
        let buy = if self.source.price_includes_vat {
            round2(supplier_price / VAT)
        } else {
            supplier_price
        };

        // The PriceBook keys include the prefix, e.g. "AM - 6285".
        let key = format!("{} - {}", self.source.prefix, record.sku);
        let Some(reference) = self.pricebook.get(&key) else {
            debug!("sku not in pricebook, skipping (sku={key})");
            self.unmatched_count += 1;
            return Ok(None);
        };

        let sell = sell_price(buy, reference);

        // Flag when our sell price is more than 2% away from the original price.
        let ratio = sell / reference.original_price;
        let fix = !(0.98..=1.02).contains(&ratio);
        if fix {
            self.fix_count += 1;
            info!(
                "{key} Original Cena: {} Nova Cena: {sell:.2}",
                reference.original_price
            );
        }

        Ok(Some(WriteRow::Price { buy, sell, fix }))
    }

    fn finish(&mut self, source_name: &str) -> SourceReport {
        info!(
            "{source_name}: processed {} records, {} not in pricebook, {} need a price fix",
            self.record_count, self.unmatched_count, self.fix_count
        );
        SourceReport {
            supplier: source_name.to_string(),
            records: self.record_count,
            excluded: 0, // run_source overwrites this with the real count
            errors: Vec::new(),
        }
    }
}

/// Same formula as the C++ calcSellPrice. A discount percent above 800 switches
/// to a second formula, ported as-is.
fn sell_price(buy: f64, reference: &PriceReference) -> f64 {
    let profit = if reference.profit_percent > 800.0 {
        reference.profit_euro + buy * ((1000.0 - reference.profit_percent) / 100.0)
    } else {
        reference.profit_euro + buy + buy * (reference.profit_percent / 100.0)
    };

    profit * VAT
}

/// Rounds the same way the C++ "%.2f" formatting does.
fn round2(value: f64) -> f64 {
    format!("{value:.2}").parse().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(euro: f64, percent: f64) -> PriceReference {
        PriceReference {
            original_price: 0.0,
            profit_euro: euro,
            profit_percent: percent,
        }
    }

    #[test]
    fn sell_price_adds_euro_and_percent_then_vat() {
        // (2 + 1 + 1*5%) * 1.23
        let sell = sell_price(1.0, &reference(2.0, 5.0));
        assert!((sell - 3.7515).abs() < 1e-9);
    }

    #[test]
    fn sell_price_uses_second_formula_above_800_percent() {
        // (1 + 10 * (1000 - 900) / 100) * 1.23
        let sell = sell_price(10.0, &reference(1.0, 900.0));
        assert!((sell - 13.53).abs() < 1e-9);
    }

    #[test]
    fn round2_matches_two_decimal_formatting() {
        assert_eq!(round2(4.06 / VAT), 3.30);
    }
}
