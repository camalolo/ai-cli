use colored::Color;
use colored::Colorize;
use anyhow::{anyhow, Result};

pub async fn alpha_vantage_query(function: &str, symbol: &str, api_key: &str, outputsize: Option<&str>, debug: bool) -> Result<String> {
    if api_key.is_empty() {
        return Err(anyhow!("ALPHA_VANTAGE_API_KEY not found in ~/.aicli.conf"));
    }
    let client = crate::http::create_async_http_client();

    let outputsize_param = outputsize.unwrap_or("compact");
    let url = if function == "GLOBAL_QUOTE" {
        format!(
            "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={}&apikey={}",
            symbol, api_key
        )
    } else {
        format!(
            "https://www.alphavantage.co/query?function={}&symbol={}&apikey={}&outputsize={}",
            function, symbol, api_key, outputsize_param
        )
    };

    println!(
        "{} {}",
        "ai-cli is querying alpha vantage for:"
            .color(Color::Cyan)
            .bold(),
        symbol
    );

    crate::log_to_file(debug, &format!("Alpha Vantage Query: function={}, symbol={}, outputsize={}", function, symbol, outputsize_param));

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

    Ok(response_text)
}
