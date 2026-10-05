use anyhow::{Context, Result, bail};
use std::collections::HashMap;

use super::{FormatParser, Record, assign_field};
use crate::config::{CsvConfig, FieldMapping};

impl FormatParser for CsvConfig {
    fn parse(
        &self,
        content: &str,
        fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        let delimiter = single_char(&self.csv.delimiter)?;
        let mut rest = content;

        // With a header, each field's selector is a column name. Without one,
        // the selector is the column's position, starting at 0.
        let header = if self.csv.has_header {
            Some(next_row(&mut rest, delimiter).context("csv has no header row")?)
        } else {
            None
        };

        let mut columns: Vec<(&str, &FieldMapping, usize)> = Vec::with_capacity(fields.len());
        for (name, mapping) in fields {
            let index = match &header {
                Some(header) => header
                    .iter()
                    .position(|column| column.trim() == mapping.selector)
                    .with_context(|| {
                        format!(
                            "csv header has no column '{}' for field '{name}'",
                            mapping.selector
                        )
                    })?,
                None => mapping.selector.parse().with_context(|| {
                    format!("field '{name}' needs a column number when the csv has no header")
                })?,
            };
            columns.push((name.as_str(), mapping, index));
        }

        while let Some(row) = next_row(&mut rest, delimiter) {
            let mut sku = None;
            let mut value = None;
            let mut extras = HashMap::new();

            for &(name, mapping, index) in &columns {
                let text = row.get(index).map(String::as_str).unwrap_or("");
                // An empty cell is treated as missing, like an absent XML element.
                if text.trim().is_empty() {
                    continue;
                }
                assign_field(
                    name,
                    text,
                    mapping.r#type,
                    &mut sku,
                    &mut value,
                    &mut extras,
                )?;
            }

            match (sku, value) {
                (Some(sku), Some(value)) => on_record(Record { sku, value, extras })?,
                _ => tracing::warn!("skipping csv row missing sku and/or value"),
            }
        }

        Ok(())
    }
}

fn single_char(text: &str) -> Result<char> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(c),
        _ => bail!("csv delimiter must be a single character, got '{text}'"),
    }
}

/// The next non-blank row. Advances `rest` past it.
fn next_row(rest: &mut &str, delimiter: char) -> Option<Vec<String>> {
    loop {
        if rest.is_empty() {
            return None;
        }
        let (row, used) = parse_row(rest, delimiter);
        *rest = &rest[used..];
        if row.len() == 1 && row[0].is_empty() {
            continue;
        }
        return Some(row);
    }
}

/// Parses one row from the start of `input`. Returns its fields and how many
/// bytes it used, including the line break. A quoted field may contain the
/// delimiter, doubled quotes, and line breaks.
fn parse_row(input: &str, delimiter: char) -> (Vec<String>, usize) {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = input.char_indices().peekable();

    while let Some((index, c)) = chars.next() {
        if in_quotes {
            if c == '"' {
                if matches!(chars.peek(), Some((_, '"'))) {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            in_quotes = true;
        } else if c == delimiter {
            fields.push(std::mem::take(&mut field));
        } else if c == '\n' {
            fields.push(field);
            return (fields, index + 1);
        } else if c != '\r' {
            field.push(c);
        }
    }

    fields.push(field);
    (fields, input.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CsvOptions, FieldType};
    use crate::sources::RecordValue;

    fn csv_config(has_header: bool) -> CsvConfig {
        CsvConfig {
            csv: CsvOptions {
                delimiter: ",".to_string(),
                has_header,
            },
        }
    }

    fn mapping(selector: &str) -> FieldMapping {
        FieldMapping {
            selector: selector.to_string(),
            r#type: FieldType::String,
        }
    }

    fn fields(pairs: &[(&str, &str)]) -> HashMap<String, FieldMapping> {
        pairs
            .iter()
            .map(|(name, selector)| (name.to_string(), mapping(selector)))
            .collect()
    }

    fn parse_all(
        config: &CsvConfig,
        content: &str,
        fields: &HashMap<String, FieldMapping>,
    ) -> Result<Vec<Record>> {
        let mut records = Vec::new();
        config.parse(content, fields, &mut |record| {
            records.push(record);
            Ok(())
        })?;
        Ok(records)
    }

    #[test]
    fn maps_columns_by_header_name() {
        let content = "SKU,Availability\nAB1,5\nCD2,0\n";
        let records = parse_all(
            &csv_config(true),
            content,
            &fields(&[("sku", "SKU"), ("stock", "Availability")]),
        )
        .unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].sku, "AB1");
        assert!(matches!(&records[0].value, RecordValue::StockText(text) if text == "5"));
        assert_eq!(records[1].sku, "CD2");
    }

    #[test]
    fn handles_quoted_fields_with_delimiters_quotes_and_line_breaks() {
        let content = "SKU,Availability\n\"A,1\",\"say \"\"hi\"\"\"\n\"B\nC\",7\n";
        let records = parse_all(
            &csv_config(true),
            content,
            &fields(&[("sku", "SKU"), ("stock", "Availability")]),
        )
        .unwrap();

        assert_eq!(records[0].sku, "A,1");
        assert!(matches!(&records[0].value, RecordValue::StockText(text) if text == "say \"hi\""));
        assert_eq!(records[1].sku, "B\nC");
    }

    #[test]
    fn skips_blank_lines_and_rows_missing_a_value() {
        let content = "SKU,Availability\n\nAB1,5\nCD2,\n\n";
        let records = parse_all(
            &csv_config(true),
            content,
            &fields(&[("sku", "SKU"), ("stock", "Availability")]),
        )
        .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sku, "AB1");
    }

    #[test]
    fn fails_when_a_mapped_column_is_missing_from_the_header() {
        let content = "SKU,Other\nAB1,5\n";
        let result = parse_all(
            &csv_config(true),
            content,
            &fields(&[("sku", "SKU"), ("stock", "Availability")]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn uses_column_positions_without_a_header() {
        let content = "AB1;5\n";
        let mut config = csv_config(false);
        config.csv.delimiter = ";".to_string();
        let records =
            parse_all(&config, content, &fields(&[("sku", "0"), ("stock", "1")])).unwrap();

        assert_eq!(records[0].sku, "AB1");
    }
}
