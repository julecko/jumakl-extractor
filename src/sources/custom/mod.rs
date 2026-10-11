mod html_listing1;

use anyhow::{Result, bail};
use std::collections::HashMap;

use super::{FormatParser, Record};
use crate::config::{CustomConfig, FieldMapping};

/// Dispatches on `handler` to a dedicated module, one per source that needs
/// hardcoded, non-generic scraping. The only place that matches on it -
/// everything downstream only ever sees the generic FormatParser interface.
impl FormatParser for CustomConfig {
    fn parse(
        &self,
        url: &str,
        content: &str,
        _fields: &HashMap<String, FieldMapping>,
        on_record: &mut dyn FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        match self.custom.handler.as_str() {
            "html_listing1" => html_listing1::parse(url, content, on_record),
            other => bail!("unknown custom handler '{other}'"),
        }
    }
}
