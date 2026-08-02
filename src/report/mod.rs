mod email;

use anyhow::Result;
use std::time::Duration;

use crate::pipeline::SourceReport;

pub struct RunSummary {
    pub elapsed: Duration,
    pub reports: Vec<SourceReport>,
    pub program_errors: Vec<String>,
}

/// One implementor per report channel (email.rs, ...). Nothing outside this
/// module knows or cares which channel is behind the trait object.
pub trait Reporter {
    fn send(&self, summary: &RunSummary) -> Result<()>;
}

// Only place that would match on channel - until a second channel (e.g. an
// API push) exists, there's nothing to match on yet.
pub fn create_reporter() -> Result<Box<dyn Reporter>> {
    Ok(Box::new(email::EmailReporter::from_env()?))
}
