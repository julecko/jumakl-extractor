use anyhow::{Result, bail};

use super::price::VAT;
use crate::config::HookConfig;
use crate::sources::{Record, RecordValue};

/// One adjustment to a record's price. Hooks are built from the source's
/// config, so nothing about a particular supplier lives in code.
pub trait RecordHook {
    fn apply(&self, record: &mut Record) -> Result<()>;
}

/// Turns a source's configured hooks into trait objects, in the order listed.
/// The one place that matches on hook kind.
pub fn build(configs: &[HookConfig]) -> Vec<Box<dyn RecordHook>> {
    configs
        .iter()
        .map(|config| -> Box<dyn RecordHook> {
            match config {
                HookConfig::RemoveVat => Box::new(RemoveVat),
                HookConfig::DiscountMultiplier {
                    field,
                    when,
                    factor,
                } => Box::new(DiscountMultiplier {
                    field: field.clone(),
                    when: when.clone(),
                    factor: *factor,
                }),
            }
        })
        .collect()
}

/// Divides the price by the VAT rate and rounds to two decimals.
struct RemoveVat;

impl RecordHook for RemoveVat {
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

impl RecordHook for DiscountMultiplier {
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

fn price_mut(record: &mut Record) -> Result<&mut f64> {
    match &mut record.value {
        RecordValue::Price(price) => Ok(price),
        RecordValue::Stock(_) => bail!("price hook received a stock record"),
    }
}

/// Rounds to two decimal places.
fn round2(value: f64) -> f64 {
    format!("{value:.2}").parse().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn price_record(price: f64, extras: &[(&str, &str)]) -> Record {
        Record {
            sku: "X".to_string(),
            value: RecordValue::Price(price),
            extras: extras
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<HashMap<_, _>>(),
        }
    }

    fn discount_then_vat() -> Vec<Box<dyn RecordHook>> {
        build(&[
            HookConfig::DiscountMultiplier {
                field: "discount".to_string(),
                when: "0".to_string(),
                factor: 0.8,
            },
            HookConfig::RemoveVat,
        ])
    }

    fn price_of(record: &Record) -> f64 {
        record.value.as_price().expect("price record")
    }

    #[test]
    fn remove_vat_divides_and_rounds() {
        let mut record = price_record(4.06, &[]);
        build(&[HookConfig::RemoveVat])[0]
            .apply(&mut record)
            .unwrap();
        assert_eq!(price_of(&record), 3.30);
    }

    #[test]
    fn discount_applies_before_vat_when_the_flag_matches() {
        let mut record = price_record(10.0, &[("discount", "0")]);
        for hook in discount_then_vat() {
            hook.apply(&mut record).unwrap();
        }
        // 10 * 0.8 = 8, then 8 / 1.23 = 6.50
        assert_eq!(price_of(&record), 6.50);
    }

    #[test]
    fn discount_is_skipped_when_the_flag_differs() {
        let mut record = price_record(10.0, &[("discount", "1")]);
        for hook in discount_then_vat() {
            hook.apply(&mut record).unwrap();
        }
        assert_eq!(price_of(&record), 8.13);
    }

    #[test]
    fn price_hooks_reject_stock_records() {
        let mut record = Record {
            sku: "X".to_string(),
            value: RecordValue::Stock(3),
            extras: HashMap::new(),
        };
        assert!(
            build(&[HookConfig::RemoveVat])[0]
                .apply(&mut record)
                .is_err()
        );
    }
}
