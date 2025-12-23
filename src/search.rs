use colored::{Color, Colorize};
use tavily::{Tavily, SearchRequest};

async fn perform_search(query: &str, api_key: &str, include_answer: bool, include_results: bool, max_results: i32, answer_mode: &str, debug: bool) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("Tavily Search API is not configured. Please set TAVILY_API_KEY in ~/.aicli.conf".to_string());
    }

    crate::log_to_file(debug, &format!("Tavily Query: {}", crate::truncate_str(query, 200)));

    let tavily = match Tavily::builder(api_key).build() {
        Ok(t) => t,
        Err(e) => return Err(format!("Failed to create Tavily client: {}", e)),
    };

    let request = SearchRequest::new(api_key, query)
        .search_depth("basic")
        .include_answer(include_answer)
        .include_raw_content(include_results)
        .max_results(max_results);

    let request_json = serde_json::to_string(&request).unwrap_or_else(|_| "Failed to serialize request".to_string());
    crate::log_to_file(debug, &format!("Tavily Request: {}", crate::truncate_str(&request_json, 500)));

    match tavily.call(&request).await {
        Ok(response) => {
            let mut output_parts = Vec::new();

            if include_answer {
                if let Some(answer) = response.answer {
                    let final_answer = if answer_mode == "basic" && answer.len() > 200 {
                        crate::log_to_file(debug, &format!("Summarizing answer from {} to 3 sentences", answer.len()));
                        let mut summariser = pithy::Summariser::new();
                        summariser.add_raw_text("answer".to_string(), answer.clone(), ".", 10, 500, false);
                        let top_sentences = summariser.approximate_top_sentences(3, 0.3, 0.1);
                        top_sentences.into_iter().map(|s| s.text).collect::<Vec<_>>().join(" ")
                    } else {
                        answer
                    };
                    output_parts.push(final_answer);
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
            crate::log_to_file(debug, &format!("Tavily Response: {}", crate::truncate_str(&result, 500)));
            Ok(result)
        }
        Err(e) => {
            crate::log_to_file(debug, &format!("Tavily Error: {}", e));
            Err(format!("Search failed: {}", e))
        }
    }
}



pub async fn search_online(query: &str, api_key: &str, include_results: bool, answer_mode: &str, debug: bool) -> String {
    println!(
        "{} {}",
        "ai-cli is searching online for:".color(Color::Cyan).bold(),
        query
    );
    let max_results = 5;
    match perform_search(query, api_key, true, include_results, max_results, answer_mode, debug).await {
        Ok(result) => result,
        Err(e) => e,
    }
}


