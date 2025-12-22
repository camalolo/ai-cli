use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::env;
use std::time::Duration;

pub fn send_email(subject: &str, body: &str, smtp_server: &str, debug: bool) -> Result<String, String> {
    if debug {
        println!("=== Email Debug Info ===");
        println!("SMTP Server: {}", smtp_server);
        println!("Subject: {}", subject);
        println!("Body length: {} characters", body.len());
    }
 
    // DEBUG: Check if DESTINATION_EMAIL is set
    let destination_email_result = env::var("DESTINATION_EMAIL");
    if debug {
        println!("DESTINATION_EMAIL env var result: {:?}", destination_email_result);
    }
    
    let recipient = match destination_email_result {
        Ok(val) => {
            if debug {
                println!("Recipient: {}", val);
            }
            val
        },
        Err(e) => {
            if debug {
                println!("Failed to get DESTINATION_EMAIL: {}", e);
                println!("Available env vars with DESTINATION: {:?}",
                    env::vars().filter(|(k,_)| k.contains("DESTINATION")).collect::<Vec<_>>());
                println!("Available env vars with EMAIL: {:?}",
                    env::vars().filter(|(k,_)| k.contains("EMAIL")).collect::<Vec<_>>());
            }
            return Err("DESTINATION_EMAIL environment variable not set. Please set it to the recipient's email address.".to_string())
        },
    };

    // For simplicity, assume sender is the same as recipient or a default
    let sender_result = env::var("SENDER_EMAIL");
    if debug {
        println!("SENDER_EMAIL env var result: {:?}", sender_result);
    }
    let sender = sender_result.unwrap_or_else(|_| recipient.clone());
    if debug {
        println!("Sender: {}", sender);
    }

    // Build the email message
    let email = Message::builder()
        .from(sender.parse().map_err(|e| format!("Invalid sender email '{}': {}", sender, e))?)
        .to(recipient.parse().map_err(|e| format!("Invalid recipient email '{}': {}", recipient, e))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {}", e))?;

    // Create SMTP transport
    if debug {
        println!("Creating SMTP transport...");
    }
    let mailer = if smtp_server == "localhost" {
        if debug {
            println!("Using localhost configuration (no auth)");
        }
        // For localhost, try without auth
        SmtpTransport::builder_dangerous(smtp_server)
            .port(25)
            .timeout(Some(Duration::from_secs(30)))
            .build()
    } else {
        if debug {
            println!("Using remote server configuration");
        }
        // For other servers, check for credentials
        let smtp_username_result = env::var("SMTP_USERNAME");
        let smtp_password_result = env::var("SMTP_PASSWORD");
        if debug {
            println!("SMTP_USERNAME env var result: {:?}", smtp_username_result);
            println!("SMTP_PASSWORD env var result: {:?}", smtp_password_result);
        }
        
        let creds = if let (Ok(username), Ok(password)) = (smtp_username_result, smtp_password_result) {
            if debug {
                println!("Found SMTP credentials for user: {}", username);
            }
            Some(Credentials::new(username, password))
        } else {
            if debug {
                println!("No SMTP credentials found, trying without authentication");
            }
            None
        };

        if let Some(creds) = creds {
            if debug {
                println!("Building SMTP transport with authentication...");
            }
            match SmtpTransport::relay(smtp_server) {
                Ok(relay) => {
                    if debug {
                        println!("SMTP relay created successfully, adding credentials...");
                    }
                    // Try port 25 first (plain SMTP), then fall back to 587 if needed
                    let mailer = relay.port(25).timeout(Some(Duration::from_secs(30))).credentials(creds).build();
                    if debug {
                        println!("SMTP transport created on port 25");
                    }
                    mailer
                },
                Err(e) => {
                    if debug {
                        println!("Failed to create SMTP relay: {}", e);
                    }
                    return Err(format!("Failed to create SMTP relay: {}", e));
                }
            }
        } else {
            if debug {
                println!("No SMTP credentials found, trying without authentication...");
            }
            // Try without authentication for local/trusted servers
            match SmtpTransport::builder_dangerous(smtp_server).port(25).timeout(Some(Duration::from_secs(30))).build() {
                mailer => {
                    if debug {
                        println!("SMTP transport created without authentication");
                    }
                    mailer
                }
            }
        }
    };
    if debug {
        println!("SMTP transport created successfully");
    }

    // Send the email
    if debug {
        println!("Attempting to send email...");
    }
    match mailer.send(&email) {
        Ok(_) => {
            if debug {
                println!("Email sent successfully!");
            }
            Ok(format!("Email sent successfully to {} via {}", recipient, smtp_server))
        },
        Err(e) => {
            if debug {
                println!("Email send failed with error: {}", e);
            }
            Err(format!("Failed to send email: {}", e))
        }
    }
}
