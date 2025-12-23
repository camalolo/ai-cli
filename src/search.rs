use colored::{Color, Colorize};
use serde_json::{json, Value};
use rayon::prelude::*;

use crate::scrape::scrape_url;
use textdistance::str;

const RELEVANCE_THRESHOLD: f32 = 0.05;

pub fn search_online(query: &str, api_key: &str, engine_id: &str) -> String {
    if api_key.is_empty() || engine_id.is_empty() {
        return "Google Search API is not configured. Please set GOOGLE_SEARCH_API_KEY and GOOGLE_SEARCH_ENGINE_ID in ~/.aicli.conf".to_string();
    }

    println!(
        "{} {}",
        "ai-cli is searching online for:".color(Color::Cyan).bold(),
        query
    );

    // Create a client with timeout
    let client = crate::http::create_http_client();

    let url = format!(
        "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}",
        api_key,
        engine_id,
        ::urlencoding::encode(query)
    );

    match client.get(&url).send() {
        Ok(response) => {
            let json: Value = match response.json() {
                Ok(j) => j,
                Err(e) => return format!("Failed to parse search response: {}", e),
            };
            let items = json.get("items").and_then(|i| i.as_array());
            if let Some(items) = items {
                // Parallel scraping using rayon
                let search_results: Vec<(String, String, String)> = items.par_iter().map(|item| {
                    let title = item
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("No title")
                        .to_string();
                    let link = item
                        .get("link")
                        .and_then(|l| l.as_str())
                        .unwrap_or("No link")
                        .to_string();
                    let content = scrape_url(link.as_str()).unwrap_or_else(|e| format!("Error scraping: {}", e));
                    (title, link, content)
                }).collect();

                let documents: Vec<&str> = search_results
                    .iter()
                    .filter_map(|(_, _, content)| {
                        if content.starts_with("Error") || content.starts_with("Skipped") {
                            None
                        } else {
                            Some(content.as_str())
                        }
                    })
                    .collect();

                if documents.is_empty() {
                    return "No valid content to process.".to_string();
                }

                let scored_results: Vec<(f32, String, String, String)> = search_results
                    .into_iter()
                    .filter_map(|(title, link, content)| {
                        if content.starts_with("Error") || content.starts_with("Skipped") {
                            return None;
                        }

                        let similarity = str::cosine(query, &content) as f32;
                        Some((similarity, title, link, content))
                    })
                    .collect();

                let filtered_results: Vec<_> = scored_results
                    .into_iter()
                    .filter(|(score, _, _, _)| *score > 0.1)
                    .take(3)
                    .filter(|(score, _, _, _)| *score >= RELEVANCE_THRESHOLD)
                    .map(|(_, title, link, content)| {
                        json!({
                            "title": title,
                            "link": link,
                            "content": content
                        })
                    })
                    .collect();

                if filtered_results.is_empty() {
                    "No relevant results found, please ask the user if you should try a different search query.".to_string()
                } else {
                    serde_json::to_string(&filtered_results)
                        .unwrap_or("Error serializing results".to_string())
                }
            } else {
                "No results found.".to_string()
            }
        }
        Err(e) => {
            if e.is_timeout() {
                "Search failed: Request timed out".to_string()
            } else {
                format!("Search failed: {}", e)
            }
        }
    }
}


