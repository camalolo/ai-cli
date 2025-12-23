use anyhow::{anyhow, Result};
use colored::{Color, Colorize};
use reqwest::{StatusCode, Url};
use readability::extractor;



pub fn scrape_url(url: &str) -> Result<String> {
    println!("{} {}", "ai-cli is reading:".color(Color::Cyan).bold(), url);

    // Create a client with timeout
    let client = crate::http::create_http_client();

    match client.get(url).send() {
        Ok(resp) => {
            // Check status code first
            match resp.status() {
                StatusCode::OK => {
                    match resp.text() {
                        Ok(text) => {
                            let url_parsed = Url::parse(url)?;
                            let product = extractor::extract(&mut text.as_bytes(), &url_parsed)?;
                            if product.text.is_empty() {
                                Ok("No readable content found on this page.".to_string())
                            } else {
                                Ok(product.text)
                            }
                        }
                        Err(e) => Err(anyhow!("Error reading content: {}", e)),
                    }
                }
                StatusCode::NOT_FOUND => Ok("Skipped: 404 Not Found".to_string()),
                StatusCode::FORBIDDEN => Ok("Skipped: 403 Forbidden".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR => {
                    Ok("Skipped: 500 Internal Server Error".to_string())
                }
                status => Ok(format!("Skipped: HTTP status {}", status)),
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Ok("Skipped: Request timed out".to_string())
            } else if e.is_connect() {
                Ok("Skipped: Connection error".to_string())
            } else {
                Err(anyhow!("Error fetching {}: {}", url, e))
            }
        }
    }
}