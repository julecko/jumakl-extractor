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
        let total_errors: usize = summary
            .reports
            .iter()
            .map(|r| r.errors.len())
            .sum::<usize>()
            + summary.program_errors.len();

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
/// templates/report.html (the shell), templates/source.html (repeated once
/// per SourceReport) and templates/program_errors.html (rendered once, only
/// when there are program-level errors), all resolved relative to the
/// project root via paths::templates_folder_path().
fn render_html(summary: &RunSummary) -> Result<String> {
    let templates = templates_folder_path();

    let report_template = fs::read_to_string(templates.join("report.html"))
        .context("failed to read templates/report.html")?;
    let source_template = fs::read_to_string(templates.join("source.html"))
        .context("failed to read templates/source.html")?;

    let total_records: usize = summary.reports.iter().map(|r| r.records).sum();
    let source_errors: usize = summary.reports.iter().map(|r| r.errors.len()).sum();
    let total_errors = source_errors + summary.program_errors.len();

    let sources_html: String = summary
        .reports
        .iter()
        .map(|report| render_source(&source_template, report))
        .collect();

    let program_errors_html = if summary.program_errors.is_empty() {
        String::new()
    } else {
        let program_errors_template = fs::read_to_string(templates.join("program_errors.html"))
            .context("failed to read templates/program_errors.html")?;
        render_program_errors(&program_errors_template, &summary.program_errors)
    };

    let status = if total_errors == 0 { "ok" } else { "err" };
    let status_label = if total_errors == 0 {
        format!(
            "All {} source(s) completed without errors",
            summary.reports.len()
        )
    } else {
        format!(
            "{total_errors} error(s) occurred ({} program, {source_errors} across sources)",
            summary.program_errors.len()
        )
    };

    Ok(report_template
        .replace("{{ELAPSED}}", &format!("{:.2?}", summary.elapsed))
        .replace("{{STATUS}}", status)
        .replace("{{STATUS_LABEL}}", &status_label)
        .replace("{{PROGRAM_ERRORS}}", &program_errors_html)
        .replace("{{SOURCE_COUNT}}", &summary.reports.len().to_string())
        .replace("{{RECORD_COUNT}}", &total_records.to_string())
        .replace("{{ERROR_COUNT}}", &source_errors.to_string())
        .replace("{{SOURCES}}", &sources_html))
}

/// Errors not tied to any specific source (e.g. the output file couldn't be
/// created at all) - only called when there's at least one.
fn render_program_errors(template: &str, errors: &[String]) -> String {
    let items: String = errors
        .iter()
        .map(|err| format!("<li>{}</li>", html_escape(err)))
        .collect();
    template.replace("{{ERRORS}}", &items)
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

    let status = if report.errors.is_empty() {
        "ok"
    } else {
        "err"
    };

    template
        .replace("{{SUPPLIER}}", &html_escape(&report.supplier))
        .replace("{{RECORDS}}", &report.records.to_string())
        .replace("{{ERROR_COUNT}}", &report.errors.len().to_string())
        .replace("{{STATUS}}", status)
        .replace("{{ERRORS}}", &errors_html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
