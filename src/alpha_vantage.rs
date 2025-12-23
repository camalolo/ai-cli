use colored::Color;
use colored::Colorize;
use reqwest::blocking::Client;

pub fn alpha_vantage_query(function: &str, symbol: &str, api_key: &str) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("ALPHA_VANTAGE_API_KEY not found in ~/.aicli.conf".to_string());
    }
    let client = crate::http::create_http_client().unwrap_or_else(|_| Client::new());

    let url = format!(
        "https://www.alphavantage.co/query?function={}&symbol={}&apikey={}",
        function, symbol, api_key
    );

    println!(
        "{} {}",
        "ai-cli is querying alpha vantage for:"
            .color(Color::Cyan)
            .bold(),
        symbol
    );

    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("Alpha Vantage API request failed: {}", e))?;

    let response_text = response
        .text()
        .map_err(|e| format!("Failed to parse Alpha Vantage response: {}", e))?;

    Ok(response_text)
}
