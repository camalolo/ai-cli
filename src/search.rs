use colored::{Color, Colorize};
use serde_json::json;
use tavily::{Tavily, SearchRequest};

pub async fn search_online(query: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        return "Tavily Search API is not configured. Please set TAVILY_API_KEY in ~/.aicli.conf".to_string();
    }

    println!(
        "{} {}",
        "ai-cli is searching online for:".color(Color::Cyan).bold(),
        query
    );

    let tavily = Tavily::new(api_key);

    let mut request = SearchRequest::new(api_key, query);
    request
        .search_depth("advanced")
        .include_answer(true)
        .include_raw_content(true)
        .max_results(5);

    match tavily.call(&request).await {
        Ok(response) => {
            if response.results.is_empty() {
                return "No results found.".to_string();
            }

            let filtered_results: Vec<_> = response.results
                .into_iter()
                .take(5)
                .map(|result| {
                    json!({
                        "title": result.title,
                        "link": result.url,
                        "content": result.content
                    })
                })
                .collect();

            serde_json::to_string(&filtered_results)
                .unwrap_or("Error serializing results".to_string())
        }
        Err(e) => {
            format!("Search failed: {}", e)
        }
    }
}


