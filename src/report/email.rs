use anyhow::{Context, Result};
use lettre::message::{MultiPart, SinglePart};
use lettre::transport::smtp::SmtpTransport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, Transport};
use std::env;
use std::fs;

use super::{Reporter, RunSummary};
use crate::paths::templates_folder_path;
use crate::pipeline::SourceReport;

pub struct EmailReporter {
    host: String,
    username: String,
    password: String,
    from: String,
    to: String,
}

impl EmailReporter {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: env::var("MAIL_HOST").context("MAIL_HOST not set in .env")?,
            username: env::var("MAIL_USERNAME").context("MAIL_USERNAME not set in .env")?,
            password: env::var("MAIL_PASSWORD").context("MAIL_PASSWORD not set in .env")?,
            from: env::var("MAIL_FROM").context("MAIL_FROM not set in .env")?,
            to: env::var("MAIL_TO").context("MAIL_TO not set in .env")?,
        })
    }
}

impl Reporter for EmailReporter {
    fn send(&self, summary: &RunSummary) -> Result<()> {
        let total_errors: usize = summary.reports.iter().map(|r| r.errors.len()).sum();

        let message = Message::builder()
            .from(self.from.parse().context("invalid MAIL_FROM address")?)
            .to(self.to.parse().context("invalid MAIL_TO address")?)
            .subject(format!(
                "Extraction report - {} source(s), {total_errors} error(s)",
                summary.reports.len()
            ))
            .multipart(MultiPart::alternative().singlepart(SinglePart::html(render_html(summary)?)))
            .context("failed to build email message")?;

        let transport = SmtpTransport::relay(&self.host)
            .context("failed to configure smtp relay")?
            .credentials(Credentials::new(
                self.username.clone(),
                self.password.clone(),
            ))
            .build();

        transport
            .send(&message)
            .context("failed to send report email")?;
        Ok(())
    }
}

/// This module only fills in data - the actual layout/styling lives in
/// templates/report.html (the shell) and templates/source.html (repeated
/// once per SourceReport), both resolved relative to the project root via
/// paths::templates_folder_path().
fn render_html(summary: &RunSummary) -> Result<String> {
    let templates = templates_folder_path();

    let report_template = fs::read_to_string(templates.join("report.html"))
        .context("failed to read templates/report.html")?;
    let source_template = fs::read_to_string(templates.join("source.html"))
        .context("failed to read templates/source.html")?;

    let total_records: usize = summary.reports.iter().map(|r| r.records).sum();
    let total_errors: usize = summary.reports.iter().map(|r| r.errors.len()).sum();

    let sources_html: String = summary
        .reports
        .iter()
        .map(|report| render_source(&source_template, report))
        .collect();

    Ok(report_template
        .replace("{{ELAPSED}}", &format!("{:.2?}", summary.elapsed))
        .replace("{{SOURCE_COUNT}}", &summary.reports.len().to_string())
        .replace("{{RECORD_COUNT}}", &total_records.to_string())
        .replace("{{ERROR_COUNT}}", &total_errors.to_string())
        .replace("{{SOURCES}}", &sources_html))
}

fn render_source(template: &str, report: &SourceReport) -> String {
    let errors_html = if report.errors.is_empty() {
        String::new()
    } else {
        let items: String = report
            .errors
            .iter()
            .map(|err| format!("<li>{}</li>", html_escape(err)))
            .collect();
        format!("<ul class=\"errors\">{items}</ul>")
    };

    let badge_class = if report.errors.is_empty() {
        "badge badge-ok"
    } else {
        "badge badge-err"
    };

    template
        .replace("{{SUPPLIER}}", &html_escape(&report.supplier))
        .replace("{{RECORDS}}", &report.records.to_string())
        .replace("{{ERROR_COUNT}}", &report.errors.len().to_string())
        .replace("{{BADGE_CLASS}}", badge_class)
        .replace("{{ERRORS}}", &errors_html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
