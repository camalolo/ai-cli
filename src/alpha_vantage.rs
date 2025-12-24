use colored::Color;
use colored::Colorize;
use anyhow::{anyhow, Result};
use serde_json::{Value, Map};
use regex::Regex;

fn find_time_series_key(json: &Value) -> Option<String> {
    if let Value::Object(map) = json {
        for (key, value) in map {
            if let Value::Object(ts_map) = value {
                // Check if at least one key looks like a date
                let date_regex = Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap();
                if ts_map.keys().any(|k| date_regex.is_match(k)) {
                    return Some(key.clone());
                }
            }
        }
    }
    None
}

fn limit_time_series(ts_map: &mut Map<String, Value>, n: usize) {
    let date_regex = Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap();
    let mut dates: Vec<String> = ts_map.keys()
        .filter(|k| date_regex.is_match(k))
        .cloned()
        .collect();
    dates.sort_by(|a, b| b.cmp(a)); // descending
    dates.truncate(n);
    let to_keep: std::collections::HashSet<String> = dates.into_iter().collect();
    ts_map.retain(|k, _| to_keep.contains(k));
}

pub async fn alpha_vantage_query(function: &str, symbol: &str, api_key: &str, outputsize: Option<&str>, limit: Option<usize>, debug: bool) -> Result<String> {
    if api_key.is_empty() {
        return Err(anyhow!("ALPHA_VANTAGE_API_KEY not found in ~/.aicli.conf"));
    }
    let client = crate::http::create_async_http_client();

    let outputsize_param = outputsize.unwrap_or("compact");
    let url = format!(
        "https://www.alphavantage.co/query?function={}&symbol={}&apikey={}{}",
        function, symbol, api_key,
        if function == "GLOBAL_QUOTE" { "".to_string() } else { format!("&outputsize={}", outputsize_param) }
    );

    println!(
        "{} {}",
        "ai-cli is querying alpha vantage for:"
            .color(Color::Cyan)
            .bold(),
        symbol
    );

    crate::log_to_file(debug, &format!("Alpha Vantage Query: function={}, symbol={}, outputsize={}, limit={:?}", function, symbol, outputsize_param, limit));

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Alpha Vantage API request failed: {}", e).context("HTTP request error"))?;

    let response_text = response
        .text()
        .await
        .map_err(|e| anyhow!("Failed to parse Alpha Vantage response: {}", e).context("Response parsing error"))?;

    crate::log_to_file(debug, &format!("Alpha Vantage Response: {}", response_text));

    if let Some(n) = limit {
        if n > 0 {
            if let Ok(mut json) = serde_json::from_str::<Value>(&response_text) {
                if let Some(time_series_key) = find_time_series_key(&json) {
                    if let Some(Value::Object(ts_map)) = json.get_mut(&time_series_key) {
                        limit_time_series(ts_map, n);
                    }
                }
                if let Ok(limited_text) = serde_json::to_string(&json) {
                    return Ok(limited_text);
                }
            }
        }
    }
    Ok(response_text)
}
