//! Shared mail helpers: the SMTP sender, MAIL_TO parsing, and HTML escaping.
//! Used by every mail the program sends (email_report, price_fix_report).

use anyhow::{Context, Result, bail};
use lettre::message::{Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::SmtpTransport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, Transport};
use std::env;

/// SMTP server and sender from .env. Shared by every mail we send, so each
/// mail only has to decide who gets it and what it says.
pub struct Mailer {
    host: String,
    username: String,
    password: String,
    from: String,
}

impl Mailer {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: env::var("MAIL_HOST").context("MAIL_HOST not set in .env")?,
            username: env::var("MAIL_USERNAME").context("MAIL_USERNAME not set in .env")?,
            password: env::var("MAIL_PASSWORD").context("MAIL_PASSWORD not set in .env")?,
            from: env::var("MAIL_FROM").context("MAIL_FROM not set in .env")?,
        })
    }

    /// Sends one HTML mail to every address in `to`.
    pub fn send_html(&self, to: &[Mailbox], subject: String, html: String) -> Result<()> {
        let mut builder =
            Message::builder().from(self.from.parse().context("invalid MAIL_FROM address")?);
        for recipient in to {
            builder = builder.to(recipient.clone());
        }

        let message = builder
            .subject(subject)
            .multipart(MultiPart::alternative().singlepart(SinglePart::html(html)))
            .context("failed to build email message")?;

        let transport = SmtpTransport::relay(&self.host)
            .context("failed to configure smtp relay")?
            .credentials(Credentials::new(
                self.username.clone(),
                self.password.clone(),
            ))
            .build();

        transport.send(&message).context("failed to send email")?;
        Ok(())
    }
}

/// Recipients from MAIL_TO. Used by every mail, so both the status report and
/// the price-fix mail go to the same people.
pub fn recipients_from_env() -> Result<Vec<Mailbox>> {
    parse_recipients(&env::var("MAIL_TO").context("MAIL_TO not set in .env")?)
}

/// MAIL_TO may hold several addresses separated by commas, e.g.
/// `a@x.sk,b@y.sk`. Blank entries are ignored.
fn parse_recipients(raw: &str) -> Result<Vec<Mailbox>> {
    let recipients = raw
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(|address| {
            address
                .parse::<Mailbox>()
                .with_context(|| format!("invalid MAIL_TO address: '{address}'"))
        })
        .collect::<Result<Vec<_>>>()?;

    if recipients.is_empty() {
        bail!("MAIL_TO contains no addresses");
    }

    Ok(recipients)
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comma_separated_recipients() {
        let recipients = parse_recipients("a@x.sk, b@y.sk ,,").expect("should parse");
        assert_eq!(recipients.len(), 2);
        assert_eq!(recipients[0].email.to_string(), "a@x.sk");
        assert_eq!(recipients[1].email.to_string(), "b@y.sk");
    }

    #[test]
    fn rejects_invalid_or_empty_recipients() {
        assert!(parse_recipients("not-an-address").is_err());
        assert!(parse_recipients(" , ").is_err());
    }
}
