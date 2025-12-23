use anyhow::{anyhow, Context, Result};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::time::Duration;

pub fn send_email(subject: &str, body: &str, config: &crate::config::Config, _debug: bool) -> Result<String> {
    log::debug!("=== Email Debug Info ===");
    log::debug!("SMTP Server: {}", config.smtp_server);
    log::debug!("Subject: {}", subject);
    log::debug!("Body length: {} characters", body.len());

    let recipient = config.destination_email.clone();
    if recipient.is_empty() {
        return Err(anyhow!("DESTINATION_EMAIL not set in config. Please set it to the recipient's email address."));
    }
    log::debug!("Recipient: {}", recipient);

    let sender = if config.sender_email.is_empty() {
        recipient.clone()
    } else {
        config.sender_email.clone()
    };
    log::debug!("Sender: {}", sender);

    // Build the email message
    let email = Message::builder()
        .from(sender.parse().with_context(|| format!("Invalid sender email '{}'", sender))?)
        .to(recipient.parse().with_context(|| format!("Invalid recipient email '{}'", recipient))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .with_context(|| "Failed to build email")?;

    // Create SMTP transport
    log::debug!("Creating SMTP transport...");
    let mailer = if config.smtp_server == "localhost" {
        log::debug!("Using localhost configuration (no auth)");
        // For localhost, try without auth
        SmtpTransport::builder_dangerous(&config.smtp_server)
            .port(25)
            .timeout(Some(Duration::from_secs(5)))
            .build()
    } else {
        log::debug!("Using remote server configuration");
        // For other servers, check for credentials
        let creds = if !config.smtp_username.is_empty() && !config.smtp_password.is_empty() {
            log::debug!("Found SMTP credentials for user: {}", config.smtp_username);
            Some(Credentials::new(config.smtp_username.clone(), config.smtp_password.clone()))
        } else {
            log::debug!("No SMTP credentials found, trying without authentication");
            None
        };

        if let Some(creds) = creds {
            log::debug!("Building SMTP transport with authentication...");
            match SmtpTransport::relay(&config.smtp_server) {
                Ok(relay) => {
                    log::debug!("SMTP relay created successfully, adding credentials...");
                    // Try port 25 first (plain SMTP), then fall back to 587 if needed
                    let mailer = relay.port(25).timeout(Some(Duration::from_secs(5))).credentials(creds).build();
                    log::debug!("SMTP transport created on port 25");
                    mailer
                },
                Err(e) => {
                    log::debug!("Failed to create SMTP relay: {}", e);
                     return Err(anyhow!("Failed to create SMTP relay: {}", e));
                }
            }
        } else {
            log::debug!("No SMTP credentials found, trying without authentication...");
            // Try without authentication for local/trusted servers
            let mailer = SmtpTransport::builder_dangerous(&config.smtp_server).port(25).timeout(Some(Duration::from_secs(5))).build();
            log::debug!("SMTP transport created without authentication");
            mailer
        }
    };
    log::debug!("SMTP transport created successfully");

    // Send the email
    log::debug!("Attempting to send email...");
    match mailer.send(&email) {
        Ok(_) => {
            log::debug!("Email sent successfully!");
            Ok(format!("Email sent successfully to {} via {}", recipient, config.smtp_server))
        },
        Err(e) => {
            log::debug!("Email send failed with error: {}", e);
            Err(anyhow!("Failed to send email: {}", e))
        }
    }
}
