use anyhow::{Result, bail};
use std::collections::HashMap;

use super::price::VAT;
use crate::config::RuleConfig;
use crate::sources::{Record, RecordValue};

/// One adjustment to a record. Rules are built from each source's own config,
/// so nothing about a particular supplier lives in code.
pub trait RecordRule {
    fn apply(&self, record: &mut Record) -> Result<()>;
}

/// Turns a source's rules into trait objects, in the order listed. The one
/// place that matches on rule kind.
pub fn build(configs: &[RuleConfig]) -> Vec<Box<dyn RecordRule>> {
    configs
        .iter()
        .map(|config| -> Box<dyn RecordRule> {
            match config {
                RuleConfig::RemoveVat => Box::new(RemoveVat),
                RuleConfig::DiscountMultiplier {
                    field,
                    when,
                    factor,
                } => Box::new(DiscountMultiplier {
                    field: field.clone(),
                    when: when.clone(),
                    factor: *factor,
                }),
                RuleConfig::SkuSubstring { start, drop_end } => Box::new(SkuSubstring {
                    start: *start,
                    drop_end: *drop_end,
                }),
                RuleConfig::QuantityMap { map, default } => Box::new(QuantityMap {
                    map: map.clone(),
                    default: default.clone(),
                }),
            }
        })
        .collect()
}

/// Divides the price by the VAT rate and rounds to two decimals.
struct RemoveVat;

impl RecordRule for RemoveVat {
    fn apply(&self, record: &mut Record) -> Result<()> {
        let price = price_mut(record)?;
        *price = round2(*price / VAT);
        Ok(())
    }
}

/// Multiplies the price by `factor` when an extra field has the value `when`.
struct DiscountMultiplier {
    field: String,
    when: String,
    factor: f64,
}

impl RecordRule for DiscountMultiplier {
    fn apply(&self, record: &mut Record) -> Result<()> {
        let matches = record
            .extras
            .get(&self.field)
            .is_some_and(|value| value.trim() == self.when);
        if matches {
            *price_mut(record)? *= self.factor;
        }
        Ok(())
    }
}

/// Keeps the characters from `start` up to `drop_end` from the end of the SKU.
struct SkuSubstring {
    start: usize,
    drop_end: usize,
}

impl RecordRule for SkuSubstring {
    fn apply(&self, record: &mut Record) -> Result<()> {
        let chars: Vec<char> = record.sku.chars().collect();
        // TODO: decide what a short SKU should do. massExtraction clears the SKU
        // and still writes the row (with an empty SKU) and only alerts on the
        // console. For now the SKU is left unchanged.
        if chars.len() < self.start + self.drop_end {
            tracing::debug!(
                "sku '{}' is too short for the sku_substring rule, leaving it unchanged",
                record.sku
            );
            return Ok(());
        }
        record.sku = chars[self.start..chars.len() - self.drop_end]
            .iter()
            .collect();
        Ok(())
    }
}

/// Replaces a stock quantity using `map`, falling back to `default` when set.
struct QuantityMap {
    map: HashMap<String, String>,
    default: Option<String>,
}

impl RecordRule for QuantityMap {
    fn apply(&self, record: &mut Record) -> Result<()> {
        let RecordValue::StockText(text) = &mut record.value else {
            bail!("quantity_map rule received a non-stock record");
        };

        let replacement = self.map.get(text.trim()).or(self.default.as_ref()).cloned();
        if let Some(replacement) = replacement {
            *text = replacement;
        }
        Ok(())
    }
}

fn price_mut(record: &mut Record) -> Result<&mut f64> {
    match &mut record.value {
        RecordValue::Price(price) => Ok(price),
        RecordValue::Stock(_) | RecordValue::StockText(_) => {
            bail!("price rule received a stock record")
        }
    }
}

/// Rounds to two decimal places.
fn round2(value: f64) -> f64 {
    format!("{value:.2}").parse().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price_record(price: f64, extras: &[(&str, &str)]) -> Record {
        Record {
            sku: "X".to_string(),
            value: RecordValue::Price(price),
            extras: extras
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    fn stock_record(sku: &str, quantity: &str) -> Record {
        Record {
            sku: sku.to_string(),
            value: RecordValue::StockText(quantity.to_string()),
            extras: HashMap::new(),
        }
    }

    fn discount_then_vat() -> Vec<Box<dyn RecordRule>> {
        build(&[
            RuleConfig::DiscountMultiplier {
                field: "discount".to_string(),
                when: "0".to_string(),
                factor: 0.8,
            },
            RuleConfig::RemoveVat,
        ])
    }

    fn price_of(record: &Record) -> f64 {
        record.value.as_price().expect("price record")
    }

    #[test]
    fn remove_vat_divides_and_rounds() {
        let mut record = price_record(4.06, &[]);
        build(&[RuleConfig::RemoveVat])[0]
            .apply(&mut record)
            .unwrap();
        assert_eq!(price_of(&record), 3.30);
    }

    #[test]
    fn discount_applies_before_vat_when_the_flag_matches() {
        let mut record = price_record(10.0, &[("discount", "0")]);
        for rule in discount_then_vat() {
            rule.apply(&mut record).unwrap();
        }
        // 10 * 0.8 = 8, then 8 / 1.23 = 6.50
        assert_eq!(price_of(&record), 6.50);
    }

    #[test]
    fn discount_is_skipped_when_the_flag_differs() {
        let mut record = price_record(10.0, &[("discount", "1")]);
        for rule in discount_then_vat() {
            rule.apply(&mut record).unwrap();
        }
        assert_eq!(price_of(&record), 8.13);
    }

    #[test]
    fn price_rules_reject_stock_records() {
        let mut record = stock_record("X", "3");
        assert!(
            build(&[RuleConfig::RemoveVat])[0]
                .apply(&mut record)
                .is_err()
        );
    }

    #[test]
    fn sku_substring_keeps_the_middle_of_the_sku() {
        // 15 characters: start 9, drop the last 3 -> "MID"
        let mut record = stock_record("AAAAAAAAAMID123", "1");
        build(&[RuleConfig::SkuSubstring {
            start: 9,
            drop_end: 3,
        }])[0]
            .apply(&mut record)
            .unwrap();
        assert_eq!(record.sku, "MID");
    }

    #[test]
    fn sku_substring_leaves_a_short_sku_unchanged() {
        let mut record = stock_record("SHORT", "1");
        build(&[RuleConfig::SkuSubstring {
            start: 9,
            drop_end: 3,
        }])[0]
            .apply(&mut record)
            .unwrap();
        assert_eq!(record.sku, "SHORT");
    }

    #[test]
    fn quantity_map_uses_default_for_unlisted_values() {
        let map: HashMap<String, String> = [("true", "1000"), ("3", "1000")]
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        let rule = build(&[RuleConfig::QuantityMap {
            map,
            default: Some("0".to_string()),
        }]);

        let mut listed = stock_record("X", "true");
        rule[0].apply(&mut listed).unwrap();
        listed.finish().unwrap();
        assert_eq!(listed.value.as_stock(), Some(1000));

        let mut unlisted = stock_record("X", "5");
        rule[0].apply(&mut unlisted).unwrap();
        unlisted.finish().unwrap();
        assert_eq!(unlisted.value.as_stock(), Some(0));
    }

    #[test]
    fn quantity_map_without_default_leaves_unlisted_values() {
        let map: HashMap<String, String> = [("1", "2")]
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        let rule = build(&[RuleConfig::QuantityMap { map, default: None }]);

        let mut record = stock_record("X", "5");
        rule[0].apply(&mut record).unwrap();
        record.finish().unwrap();
        assert_eq!(record.value.as_stock(), Some(5));
    }
}
