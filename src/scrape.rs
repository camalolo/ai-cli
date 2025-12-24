use anyhow::{anyhow, Result};
use colored::{Color, Colorize};
use pithy;
use reqwest::{StatusCode, Url};
use readability::extractor;



pub async fn scrape_url(url: &str, mode: &str, debug: bool) -> Result<String> {
    println!("{} {}", "ai-cli is reading:".color(Color::Cyan).bold(), url);

    crate::log_to_file(debug, &format!("Scraping URL: {}", url));

    // Create a client with timeout
    let client = crate::http::create_async_http_client();

    let result = match client.get(url).send().await {
        Ok(resp) => {
            // Check status code first
            match resp.status() {
                StatusCode::OK => {
                    match resp.text().await {
                        Ok(text) => {
                            let url_parsed = Url::parse(url)?;
                            let product = extractor::extract(&mut text.as_bytes(), &url_parsed)?;
                            if product.text.is_empty() {
                                "No readable content found on this page.".to_string()
                            } else {
                                product.text
                            }
                        }
                        Err(e) => return Err(anyhow!("Error reading content: {}", e)),
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
                return Err(anyhow!("Error fetching {}: {}", url, e));
            }
        }
    };

    let final_result = if mode == "full" || result.len() <= 1024 {
        result
    } else {
        crate::log_to_file(debug, &format!("Summarizing content from {} chars", result.len()));
        let mut summariser = pithy::Summariser::new();
        summariser.add_raw_text("content".to_string(), result.clone(), ".", 10, 500, false);
        let top_sentences = summariser.approximate_top_sentences(3, 0.3, 0.1);
        let summary = top_sentences.into_iter().map(|s| s.text).collect::<Vec<_>>().join(" ");
        if summary.is_empty() {
            result // fallback to full content if summarization fails
        } else {
            summary
        }
    };

    crate::log_to_file(debug, &format!("Final scrape result: {}", final_result));

    Ok(final_result)
}