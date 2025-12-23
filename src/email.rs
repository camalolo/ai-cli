use anyhow::{anyhow, Context, Result};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::time::Duration;

fn create_smtp_transport(config: &crate::config::Config) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    if config.smtp_server == "localhost" {
        return Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_server)
            .port(25)
            .timeout(Some(Duration::from_secs(5)))
            .build());
    }

    let creds = if !config.smtp_username.is_empty() && !config.smtp_password.is_empty() {
        Some(Credentials::new(config.smtp_username.clone(), config.smtp_password.clone()))
    } else {
        None
    };

    let relay = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_server)?;
    if let Some(creds) = creds {
        Ok(relay.port(25).timeout(Some(Duration::from_secs(5))).credentials(creds).build())
    } else {
        Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_server).port(25).timeout(Some(Duration::from_secs(5))).build())
    }
}

pub async fn send_email(subject: &str, body: &str, config: &crate::config::Config, debug: bool) -> Result<String> {
    crate::log_to_file(debug, "=== Email Debug Info ===");
    crate::log_to_file(debug, &format!("SMTP Server: {}", config.smtp_server));
    crate::log_to_file(debug, &format!("Subject: {}", subject));
    crate::log_to_file(debug, &format!("Body length: {} characters", body.len()));

    let recipient = config.destination_email.clone();
    if recipient.is_empty() {
        return Err(anyhow!("DESTINATION_EMAIL not set in config. Please set it to the recipient's email address."));
    }
    crate::log_to_file(debug, &format!("Recipient: {}", recipient));

    let sender = if config.sender_email.is_empty() {
        recipient.clone()
    } else {
        config.sender_email.clone()
    };
    crate::log_to_file(debug, &format!("Sender: {}", sender));

    // Build the email message
    let email = Message::builder()
        .from(sender.parse().with_context(|| format!("Invalid sender email '{}'", sender))?)
        .to(recipient.parse().with_context(|| format!("Invalid recipient email '{}'", recipient))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .with_context(|| "Failed to build email")?;

    // Create SMTP transport
    let mailer = create_smtp_transport(config)?;

    // Send the email
    crate::log_to_file(debug, "Attempting to send email...");
    match mailer.send(email).await {
        Ok(_) => {
            crate::log_to_file(debug, "Email sent successfully!");
            Ok(format!("Email sent successfully to {} via {}", recipient, config.smtp_server))
        },
        Err(e) => {
            crate::log_to_file(debug, &format!("Email send failed with error: {}", e));
            Err(anyhow!("Failed to send email: {}", e))
        }
    }
}
