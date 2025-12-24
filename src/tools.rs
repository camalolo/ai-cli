use colored::{Color, Colorize};
use serde_json::{json, Value};
use regex::Regex;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::command::execute_command;
use crate::search::search_online;
use crate::email::send_email;
use crate::alpha_vantage::alpha_vantage_query;
use crate::file_edit::file_editor;
use termimad::MadSkin;
use termimad::crossterm::style::Color as TermColor;
use termimad::crossterm::style::Attribute;

use crate::chat::ChatManager;
use anyhow::Result;

pub fn process_execute_command(args: &Value, debug: bool, allow_commands: bool) -> (String, bool) {
    let command = args.get("command").and_then(|c| c.as_str());
    if let Some(cmd) = command {
        let confirmed = if allow_commands {
            true
        } else {
            dialoguer::Confirm::new()
                .with_prompt(format!("LLM wants to execute command: {} | Confirm?", cmd))
                .default(false)
                .interact()
                .unwrap_or(false)
        };
        if confirmed {
            println!("Executing command: {}", cmd.color(Color::Magenta));
            let result = execute_command(cmd, debug).unwrap_or_else(|e| e.to_string());
            println!();
            (normalize_output(&format!("[Tool result] execute_command: {}", result)), false)
        } else {
            (normalize_output("[Tool result] execute_command: User rejected the command execution."), true)
        }
    } else {
        (normalize_output("[Tool error] execute_command: Missing 'command' parameter"), false)
    }
}



pub async fn process_search_online(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> (String, bool) {
    let query = args.get("query").and_then(|q| q.as_str());
    let include_results = args.get("include_results").and_then(|ir| ir.as_bool()).unwrap_or(false);
    let answer_mode = args.get("answer_mode").and_then(|am| am.as_str()).unwrap_or("basic");
    if let Some(q) = query {
        let manager = chat_manager.lock().await;
        let api_key = manager.get_tavily_api_key().to_string();
        let result = search_online(q, &api_key, include_results, answer_mode, debug).await;
        (normalize_output(&format!("[Tool result] search_online: {}", result)), false)
    } else {
        (normalize_output("[Tool error] search_online: Missing 'query' parameter"), false)
    }
}

pub async fn process_send_email(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> (String, bool) {
    let subject = args.get("subject").and_then(|s| s.as_str());
    let body = args.get("body").and_then(|b| b.as_str());

    if let (Some(subj), Some(bod)) = (subject, body) {
        let manager = chat_manager.lock().await;
        let config = manager.get_config();
        match send_email(subj, bod, config, debug).await {
            Ok(msg) => (normalize_output(&format!("[Tool result] send_email: {}", msg)), false),
            Err(e) => (normalize_output(&format!("[Tool error] send_email: {}", e)), false),
        }
    } else {
        (normalize_output("[Tool error] send_email: Missing required parameters"), false)
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

/// Displays normalized LLM output with Markdown rendering
pub fn display_llm_output(content: &str, _color: Color) {
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
                    display_llm_output(content, Color::Yellow);
                }
            }
        }
    }
}

fn extract_tool_calls(response: &Value) -> Vec<(String, Value)> {
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
                            if !name.is_empty() {
                                Some((name, args))
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
            crate::log_to_file(debug, &format!("Tool calls found: {:?}", tool_calls));
        }

        if tool_calls.is_empty() {
            break;
        }

        let mut rejection_occurred = false;
        let mut results = Vec::new();
        for (func_name, args) in tool_calls {
            match func_name.as_str() {
                "execute_command" => {
                    let (result, rejected) = process_execute_command(&args, debug, allow_commands);
                    results.push(result);
                    if rejected { rejection_occurred = true; }
                }

                "search_online" => {
                    let (result, rejected) = process_search_online(&args, chat_manager, debug).await;
                    results.push(result);
                    if rejected { rejection_occurred = true; }
                }
                "scrape_url" => {
                    let url = args.get("url").and_then(|u| u.as_str());
                    let mode = args.get("mode").and_then(|m| m.as_str()).unwrap_or("summarized");
                     if let Some(u) = url {
                          match crate::scrape::scrape_url(u, mode, debug).await {
                             Ok(result) => results.push(tool_result("scrape_url", &result)),
                             Err(e) => results.push(tool_error("scrape_url", &e.to_string())),
                         }
                     } else {
                         results.push(tool_error("scrape_url", "Missing 'url' parameter"));
                     }
                }
                "send_email" => {
                    let subject = args.get("subject").and_then(|s| s.as_str()).unwrap_or("unknown");
                    println!("ai-cli is sending email: {}", subject.color(Color::Cyan).bold());
                    let (result, rejected) = process_send_email(&args, chat_manager, debug).await;
                    results.push(result);
                    if rejected { rejection_occurred = true; }
                }
                 "alpha_vantage_query" => {
                      let function = args.get("function").and_then(|f| f.as_str());
                      let symbol = args.get("symbol").and_then(|s| s.as_str());
                      let outputsize = args.get("outputsize").and_then(|s| s.as_str());
                        if let (Some(func), Some(sym)) = (function, symbol) {
                            let api_key = chat_manager.lock().await.get_alpha_vantage_api_key().to_string();
                             match alpha_vantage_query(func, sym, &api_key, outputsize, debug).await {
                              Ok(result) => results.push(tool_result("alpha_vantage_query", &result)),
                              Err(e) => results.push(tool_error("alpha_vantage_query", &e.to_string())),
                          }
                      } else {
                          results.push(tool_error("alpha_vantage_query", "Missing required parameters"));
                      }
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
                         results.push(tool_result("file_editor", &result));
                         if rejected { rejection_occurred = true; }
                     } else {
                         results.push(tool_error("file_editor", "Missing required parameters 'subcommand' or 'filename'"));
                     }
                }
                 _ => {
                     results.push(tool_error("unknown", &format!("Unknown function: {}", func_name)));
                 }
            }
        }

        if !results.is_empty() {
            let combined_results = results.join("\n");
            let normalized_results = normalize_output(&combined_results);
            current_response = chat_manager.lock().await.send_message(&normalized_results, quiet, debug).await?;
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