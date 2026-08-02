use anyhow::{Context, Result};
use lettre::message::{MultiPart, SinglePart};
use lettre::transport::smtp::SmtpTransport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, Transport};
use std::env;

use super::{Reporter, RunSummary};

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
            .multipart(MultiPart::alternative().singlepart(SinglePart::html(render_html(summary))))
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

fn render_html(summary: &RunSummary) -> String {
    let mut body = format!(
        "<h2>Extraction run completed in {:.2?}</h2>",
        summary.elapsed
    );

    for report in &summary.reports {
        body.push_str(&format!(
            "<h3>{}</h3><p>{} record(s), {} error(s)</p>",
            html_escape(&report.supplier),
            report.records,
            report.errors.len()
        ));

        if !report.errors.is_empty() {
            body.push_str("<ul>");
            for err in &report.errors {
                body.push_str(&format!("<li>{}</li>", html_escape(err)));
            }
            body.push_str("</ul>");
        }
    }

    body
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
