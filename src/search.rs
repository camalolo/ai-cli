use colored::{Color, Colorize};
use tavily::{Tavily, SearchRequest};
use tokio::time::{sleep, Duration};

async fn perform_search(query: &str, api_key: &str, include_answer: bool, include_results: bool, answer_mode: &str, debug: bool) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("Tavily Search API is not configured. Please set TAVILY_API_KEY in ~/.aicli.conf".to_string());
    }

    crate::utils::log_to_file(debug, &format!("Tavily Query: {}", crate::utils::truncate_str(query, 200)));

    let tavily = match Tavily::builder(api_key).build() {
        Ok(t) => t,
        Err(e) => return Err(format!("Failed to create Tavily client: {}", e)),
    };

    let max_results = 5;
    let request = SearchRequest::new(api_key, query)
        .search_depth("basic")
        .include_answer(include_answer)
        .include_raw_content(include_results)
        .max_results(max_results);

    let request_json = serde_json::to_string(&request).unwrap_or_else(|_| "Failed to serialize request".to_string());
    crate::utils::log_to_file(debug, &format!("Tavily Request: {}", crate::utils::truncate_str(&request_json, 500)));

    for attempt in 0..3 {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt - 1));
            crate::utils::log_to_file(debug, &format!("Retrying Tavily search in {}s (attempt {})", delay.as_secs(), attempt + 1));
            sleep(delay).await;
        }
        crate::utils::log_to_file(debug, &format!("Tavily Query Attempt {}: {}", attempt + 1, crate::utils::truncate_str(query, 200)));

        let start_time = std::time::Instant::now();
        match tavily.call(&request).await {
            Ok(response) => {
                let elapsed = start_time.elapsed();
                crate::utils::log_to_file(debug, &format!("Tavily Response ({}ms): success", elapsed.as_millis()));

                let mut output_parts = Vec::new();

            if include_answer {
                if let Some(answer) = response.answer {
                    let final_answer = if answer_mode == "basic" && answer.len() > 200 {
                        crate::utils::log_to_file(debug, &format!("Summarizing answer from {} to 3 sentences", answer.len()));
                        crate::tools::summarize_text(&answer, 3)
                    } else {
                        answer
                    };
                    output_parts.push(final_answer);
                } else if !response.results.is_empty() {
                    let combined_content: String = response.results.iter()
                        .map(|r| r.content.as_str())
                        .collect::<Vec<&str>>()
                        .join("\n\n");
                    crate::utils::log_to_file(debug, &format!("Synthesizing answer from {} search results", response.results.len()));
                    let synthesized_answer = crate::tools::summarize_text(&combined_content, 3);
                    output_parts.push(synthesized_answer);
                } else {
                    output_parts.push("No answer generated.".to_string());
                }
            }

            if include_results {
                if response.results.is_empty() {
                    output_parts.push("No results found.".to_string());
                } else {
                    let mut results_text = String::new();
                    for result in response.results.into_iter().take(max_results as usize) {
                        results_text.push_str(&format!("- **{}**: {}\n  {}\n\n", result.title, result.url, result.content));
                    }
                    output_parts.push(results_text);
                }
            }

                let result = output_parts.join("\n");
                crate::utils::log_to_file(debug, &format!("Tavily Response: {}", result));
                return Ok(result);
            }
            Err(e) => {
                let elapsed = start_time.elapsed();
                crate::utils::log_to_file(debug, &format!("Tavily Error (attempt {}, {}ms): {}", attempt + 1, elapsed.as_millis(), e));
                if attempt == 2 {
                    return Err(format!("Search failed after 3 attempts: {}", e));
                }
            }
        }
    }
    unreachable!()
}



pub async fn search_online(query: &str, api_key: &str, include_results: bool, answer_mode: &str, debug: bool) -> String {
    println!(
        "{} {}",
        "ai-cli is searching online for:".color(Color::Cyan).bold(),
        query
    );
    match perform_search(query, api_key, true, include_results, answer_mode, debug).await {
        Ok(result) => result,
        Err(e) => e,
    }
}


