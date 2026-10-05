use anyhow::{Context, Result};
use lettre::message::Mailbox;
use std::fs;

use super::email::{Mailer, html_escape, recipients_from_env};
use super::price_fix_report;
use super::{Reporter, RunSummary};
use crate::paths::templates_folder_path;
use crate::pipeline::SourceReport;

/// The status report sent at the end of every run, to MAIL_TO.
pub struct EmailReporter {
    mailer: Mailer,
    to: Vec<Mailbox>,
}

impl EmailReporter {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            mailer: Mailer::from_env()?,
            to: recipients_from_env()?,
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

        self.mailer.send_html(
            &self.to,
            format!(
                "Extraction report - {} source(s), {total_errors} error(s)",
                summary.reports.len()
            ),
            render_html(summary)?,
        )
    }

    fn send_price_fixes(&self, reports: &[SourceReport]) -> Result<()> {
        let Some(mail) = price_fix_report::build(reports)? else {
            return Ok(());
        };
        self.mailer.send_html(&self.to, mail.subject, mail.html)
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
    let total_excluded: usize = summary.reports.iter().map(|r| r.excluded).sum();
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
        .replace("{{EXCLUDED_COUNT}}", &total_excluded.to_string())
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
        .replace("{{EXCLUDED}}", &report.excluded.to_string())
        .replace("{{ERROR_COUNT}}", &report.errors.len().to_string())
        .replace("{{STATUS}}", status)
        .replace("{{ERRORS}}", &errors_html)
}
