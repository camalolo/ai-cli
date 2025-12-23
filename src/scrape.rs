use colored::{Color, Colorize};
use reqwest::StatusCode;
use scraper::{Html, Selector};
use std::sync::OnceLock;

static SELECTOR: OnceLock<Selector> = OnceLock::new();

pub fn scrape_url(url: &str) -> String {
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
                            let document = Html::parse_document(&text);
                            // Target readable content: paragraphs, headings, articles
                            let selector = SELECTOR.get_or_init(|| Selector::parse("p, h1, h2, h3, h4, h5, h6, article").expect("Failed to parse CSS selector"));
                            let readable_text: Vec<String> = document
                                .select(selector)
                                .filter(|element| element.value().name() != "script" && element.value().name() != "style")
                                .flat_map(|element| element.text())
                                .map(|t| t.trim())
                                .filter(|t| !t.is_empty())
                                .map(|t| t.to_string())
                                .collect();

                            if readable_text.is_empty() {
                                "No readable content found on this page.".to_string()
                            } else {
                                readable_text.join(" ")
                            }
                        }
                        Err(e) => format!("Error reading content: {}", e),
                    }
                }
                StatusCode::NOT_FOUND => "Skipped: 404 Not Found".to_string(),
                StatusCode::FORBIDDEN => "Skipped: 403 Forbidden".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR => {
                    "Skipped: 500 Internal Server Error".to_string()
                }
                status => format!("Skipped: HTTP status {}", status),
            }
        }
        Err(e) => {
            if e.is_timeout() {
                "Skipped: Request timed out".to_string()
            } else if e.is_connect() {
                "Skipped: Connection error".to_string()
            } else {
                format!("Error fetching {}: {}", url, e)
            }
        }
    }
}