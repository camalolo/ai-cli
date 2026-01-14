use colored::{Color, Colorize};
use serde_json::{json, Value};
use regex::Regex;
use std::sync::Arc;
use tokio::sync::Mutex;
use termimad::MadSkin;
use termimad::crossterm::style::Color as TermColor;
use termimad::crossterm::style::Attribute;

use crate::command::execute_command;
use crate::search::search_online;
use crate::email::send_email;
use crate::alpha_vantage::alpha_vantage_query;
use crate::file_edit::file_editor;
use crate::chat::ChatManager;
use crate::utils::{confirm, get_opt_bool, get_opt_str};
use anyhow::{anyhow, Result};

pub fn process_execute_command(args: &Value, debug: bool, allow_commands: bool) -> (String, bool) {
    let command = args.get("command").and_then(|c| c.as_str());
    if let Some(cmd) = command {
        let confirmed = if allow_commands {
            true
        } else {
            confirm(&format!("LLM wants to execute command: {} | Confirm?", cmd))
        };
        if confirmed {
            println!("Executing command: {}", cmd.color(Color::Magenta));
            let result = execute_command(cmd, debug).unwrap_or_else(|e| e.to_string());
            println!();
            (tool_result("execute_command", &result), false)
        } else {
            (tool_result("execute_command", "User rejected the command execution."), true)
        }
    } else {
        (tool_error("execute_command", "Missing 'command' parameter"), false)
    }
}



pub async fn process_search_online(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> (String, bool) {
    let query = args.get("query").and_then(|q| q.as_str());
    let include_results = get_opt_bool(args, "include_results", false);
    let answer_mode = get_opt_str(args, "answer_mode", "basic");
    if let Some(q) = query {
        let manager = chat_manager.lock().await;
        let api_key = manager.get_tavily_api_key().to_string();
        let result = search_online(q, &api_key, include_results, &answer_mode, debug).await;
        (tool_result("search_online", &result), false)
    } else {
        (tool_error("search_online", "Missing 'query' parameter"), false)
    }
}

pub async fn process_send_email(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> (String, bool) {
    let subject = args.get("subject").and_then(|s| s.as_str());
    let body = args.get("body").and_then(|b| b.as_str());

    if let (Some(subj), Some(bod)) = (subject, body) {
        let manager = chat_manager.lock().await;
        let config = manager.get_config();
        match send_email(subj, bod, config, debug).await {
            Ok(msg) => (tool_result("send_email", &msg), false),
            Err(e) => (tool_error("send_email", &e.to_string()), false),
        }
    } else {
        (tool_error("send_email", "Missing required parameters"), false)
    }
}

/// Normalizes LLM output text by removing excessive whitespace and ensuring consistent formatting
fn normalize_output(text: &str) -> String {
    let trimmed = text.trim();
    let normalized_line_endings = trimmed.replace("\r\n", "\n").replace('\r', "\n");
    // Use regex to limit consecutive newlines to 2
    let re = Regex::new(r"\n{3,}").unwrap();
    let limited_newlines = re.replace_all(&normalized_line_endings, "\n\n");
    limited_newlines.trim_end().to_string()
}

/// Adds consistent block spacing with a single blank line
pub fn add_block_spacing() {
    println!();
}

fn tool_result(name: &str, msg: &str) -> String {
    normalize_output(&format!("[Tool result] {}: {}", name, msg))
}

fn tool_error(name: &str, err: &str) -> String {
    normalize_output(&format!("[Tool error] {}: {}", name, err))
}

async fn handle_async_tool_result<Fut>(fut: Fut, tool_name: &str) -> (String, bool)
where
    Fut: std::future::Future<Output = Result<String>>,
{
    match fut.await {
        Ok(result) => (tool_result(tool_name, &result), false),
        Err(e) => (tool_error(tool_name, &e.to_string()), false),
    }
}

pub fn summarize_text(text: &str, num_sentences: usize) -> String {
    let mut summariser = pithy::Summariser::new();
    summariser.add_raw_text("content".to_string(), text.to_string(), ".", 10, 500, false);
    let top_sentences = summariser.approximate_top_sentences(num_sentences, 0.3, 0.1);
    top_sentences.into_iter().map(|s| s.text).collect::<Vec<_>>().join(" ")
}

/// Displays normalized LLM output with Markdown rendering
pub fn display_llm_output(content: &str) {
    let mut skin = MadSkin::default();
    skin.paragraph.set_fg(TermColor::AnsiValue(222)); // Light orange from Ubuntu palette
    // Configure styles for headers using Ubuntu-inspired colors
    skin.headers[0].set_fg(TermColor::AnsiValue(202)); // H1: Orange (#ff5f00 ~ Ubuntu orange)
    skin.headers[1].set_fg(TermColor::AnsiValue(89)); // H2: Aubergine purple (#87005f ~ #772953)
    skin.headers[2].set_fg(TermColor::AnsiValue(34)); // H3: Green (#00af00 ~ Ubuntu green)
    skin.headers[3].set_fg(TermColor::AnsiValue(33)); // H4: Blue (#0087ff ~ Ubuntu blue)
    skin.headers[4].set_fg(TermColor::AnsiValue(201)); // H5: Magenta (#ff00ff ~ Ubuntu magenta)
    skin.headers[5].set_fg(TermColor::AnsiValue(226)); // H6: Yellow (#ffff00 ~ Ubuntu yellow)
    // Bold text
    skin.bold.set_fg(TermColor::AnsiValue(255)); // White
    skin.bold.add_attr(Attribute::Bold);
    // Italic text
    skin.italic.set_fg(TermColor::AnsiValue(93)); // Purple
    skin.italic.add_attr(Attribute::Italic);
    // Code blocks
    skin.code_block.set_bg(TermColor::AnsiValue(0)); // Black
    skin.code_block.set_fg(TermColor::AnsiValue(255)); // White
    // Inline code
    skin.inline_code.set_bg(TermColor::AnsiValue(8)); // Dark grey
    skin.inline_code.set_fg(TermColor::AnsiValue(255)); // White
    print!("{}", skin.term_text(content));
}

pub fn display_response(response: &Value) {
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    display_llm_output(content);
                }
            }
        }
    }
}

fn extract_tool_calls(response: &Value) -> Vec<(String, String, Value)> {
    response
        .get("choices")
        .and_then(|c| c.as_array())
        .unwrap_or(&vec![])
        .iter()
        .flat_map(|choice| {
            choice
                .get("message")
                .and_then(|m| m.get("tool_calls"))
                .and_then(|tc| tc.as_array())
                .map(|tool_calls| {
                    tool_calls
                        .iter()
                        .filter_map(|tc| {
                            let tool_call_id = tc.get("id")
                                .and_then(|id| id.as_str())
                                .unwrap_or("")
                                .to_string();
                            let func = tc.get("function");
                            let name = func
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let args = func
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| serde_json::from_str::<Value>(a.as_str()?).ok())
                                .unwrap_or(json!({}));
                            if !name.is_empty() && !tool_call_id.is_empty() {
                                Some((tool_call_id, name, args))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect()
}

pub async fn process_tool_calls(response: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool, quiet: bool, allow_commands: bool) -> Result<()> {
    let mut current_response = response.clone();

    loop {
        let tool_calls = extract_tool_calls(&current_response);
        if debug && !tool_calls.is_empty() {
            crate::utils::log_to_file(debug, &format!("Tool calls found: {:?}", tool_calls));
        }

        if tool_calls.is_empty() {
            break;
        }

        let mut rejection_occurred = false;
        let mut tool_results: Vec<(String, String)> = Vec::new(); // (tool_call_id, result)
        for (tool_call_id, func_name, args) in tool_calls {
            match func_name.as_str() {
                "execute_command" => {
                    let (result, rejected) = process_execute_command(&args, debug, allow_commands);
                    tool_results.push((tool_call_id, result));
                    if rejected { rejection_occurred = true; }
                }

                "search_online" => {
                    let (result, rejected) = process_search_online(&args, chat_manager, debug).await;
                    tool_results.push((tool_call_id, result));
                    if rejected { rejection_occurred = true; }
                }
                 "scrape_url" => {
                     let result = handle_async_tool_result(async {
                         let url = args.get("url").and_then(|u| u.as_str()).ok_or_else(|| anyhow!("Missing 'url' parameter"))?;
                         let mode = get_opt_str(&args, "mode", "summarized");
                         crate::scrape::scrape_url(url, &mode, debug).await
                     }, "scrape_url").await;
                     tool_results.push((tool_call_id, result.0));
                 }
                "send_email" => {
                    let subject = get_opt_str(&args, "subject", "unknown");
                    println!("ai-cli is sending email: {}", subject.color(Color::Cyan).bold());
                    let (result, rejected) = process_send_email(&args, chat_manager, debug).await;
                    tool_results.push((tool_call_id, result));
                    if rejected { rejection_occurred = true; }
                }
                   "alpha_vantage_query" => {
                       let result = handle_async_tool_result(async {
                             let function = args.get("function").and_then(|f| f.as_str()).ok_or_else(|| anyhow!("Missing 'function' parameter"))?;
                             let symbol = args.get("symbol").and_then(|s| s.as_str()).ok_or_else(|| anyhow!("Missing 'symbol' parameter"))?;
                             let outputsize = args.get("outputsize").and_then(|s| s.as_str());
                             let limit = args.get("limit").and_then(|l| l.as_u64()).map(|l| l as usize);
                             let api_key = chat_manager.lock().await.get_alpha_vantage_api_key().to_string();
                             alpha_vantage_query(function, symbol, &api_key, outputsize, limit, debug).await
                       }, "alpha_vantage_query").await;
                       tool_results.push((tool_call_id, result.0));
                   }
                "file_editor" => {
                    let filename_opt = args.get("filename").and_then(|f| f.as_str());
                    let filename = filename_opt.unwrap_or("unknown");
                    println!("ai-cli is editing file: {}", filename.color(Color::Cyan).bold());
                    let subcommand = args.get("subcommand").and_then(|s| s.as_str());
                    let data = args.get("data").and_then(|d| d.as_str());
                    let replacement = args.get("replacement").and_then(|r| r.as_str());

                    if let (Some(subcmd), Some(fname)) = (subcommand, filename_opt) {
                        let skip_confirmation = matches!(subcmd, "read" | "search"); // Only skip for non-destructive ops
                         let (result, rejected) = file_editor(subcmd, fname, data, replacement, skip_confirmation, debug);
                         tool_results.push((tool_call_id, tool_result("file_editor", &result)));
                         if rejected { rejection_occurred = true; }
                     } else {
                         tool_results.push((tool_call_id, tool_error("file_editor", "Missing required parameters 'subcommand' or 'filename'")));
                     }
                }
                 _ => {
                     tool_results.push((tool_call_id, tool_error("unknown", &format!("Unknown function: {}", func_name))));
                 }
            }
        }

        if !tool_results.is_empty() {
            current_response = chat_manager.lock().await.send_tool_results(tool_results, quiet, debug).await?;
            display_response(&current_response);
            add_block_spacing();
            if rejection_occurred {
                break;
            }
        } else {
            break;
        }
    }

    Ok(())
}