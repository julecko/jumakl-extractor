mod email;
mod email_report;
mod price_fix_report;

use anyhow::Result;
use std::time::Duration;

use crate::pipeline::SourceReport;

pub struct RunSummary {
    pub elapsed: Duration,
    pub reports: Vec<SourceReport>,
    pub program_errors: Vec<String>,
}

/// One implementor per report channel (email_report.rs, ...). Nothing outside
/// this module knows or cares which channel is behind the trait object.
/// Every channel must provide both mails.
pub trait Reporter {
    /// The status report, sent once at the end of the run.
    fn send(&self, summary: &RunSummary) -> Result<()>;
    /// The separate price-fix mail. Sends nothing when there are no fixes.
    fn send_price_fixes(&self, reports: &[SourceReport]) -> Result<()>;
}

// Only place that would match on channel - until a second channel (e.g. an
// API push) exists, there's nothing to match on yet.
pub fn create_reporter() -> Result<Box<dyn Reporter>> {
    Ok(Box::new(email_report::EmailReporter::from_env()?))
}
