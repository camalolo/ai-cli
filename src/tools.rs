use colored::{Color, Colorize};
use serde_json::{json, Value};
use std::io::{self, Read};
use regex::Regex;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::Mutex;
use crate::command::execute_command;
use crate::search::search_online;
use crate::email::send_email;
use crate::alpha_vantage::alpha_vantage_query;
use crate::file_edit::file_editor;
use termimad::MadSkin;
use termimad::crossterm::style::Color as TermColor;
use termimad::crossterm::style::Attribute;
use crate::chat::ChatManager;

pub fn process_execute_command(args: &Value) -> String {
    let command = args.get("command").and_then(|c| c.as_str());
    if let Some(cmd) = command {
        println!("LLM wants to execute command: {} | Press Enter to confirm, Escape to deny", cmd.color(Color::Magenta));
        let stdin_fd = io::stdin().as_raw_fd();
        let mut orig_term: libc::termios = unsafe { std::mem::zeroed() };
        unsafe { libc::tcgetattr(stdin_fd, &mut orig_term) };
        let mut raw_term = orig_term;
        unsafe { libc::cfmakeraw(&mut raw_term) };
        unsafe { libc::tcsetattr(stdin_fd, libc::TCSANOW, &raw_term) };
        let confirmed = loop {
            let mut buf = [0u8; 1];
            if io::stdin().read_exact(&mut buf).is_ok() {
                let c = buf[0];
                if c == b'\r' { // Enter
                    break true;
                } else if c == 0x1b { // Escape
                    break false;
                }
            }
        };
        unsafe { libc::tcsetattr(stdin_fd, libc::TCSANOW, &orig_term) };
        if confirmed {
            println!("Executing command: {}", cmd.color(Color::Magenta));
            let result = execute_command(cmd);
            println!();
            normalize_output(&format!("[Tool result] execute_command: {}", result))
        } else {
            normalize_output("[Tool result] execute_command: User rejected the command execution.")
        }
    } else {
        normalize_output("[Tool error] execute_command: Missing 'command' parameter")
    }
}

pub fn process_search_online(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>) -> String {
    let query = args.get("query").and_then(|q| q.as_str());
    if let Some(q) = query {
        match chat_manager.lock() {
            Ok(manager) => {
                let api_key = manager.get_google_search_api_key().to_string();
                let engine_id = manager.get_google_search_engine_id().to_string();
                let result = search_online(q, &api_key, &engine_id);
                normalize_output(&format!("[Tool result] search_online: {}", result))
            }
            Err(_) => normalize_output("[Tool error] search_online: Failed to access configuration"),
        }
    } else {
        normalize_output("[Tool error] search_online: Missing 'query' parameter")
    }
}

pub fn process_send_email(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> String {
    let subject = args.get("subject").and_then(|s| s.as_str());
    let body = args.get("body").and_then(|b| b.as_str());

    if let (Some(subj), Some(bod)) = (subject, body) {
        let smtp_server_result = chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e)).map(|manager| manager.get_smtp_server().to_string());
        match smtp_server_result {
            Ok(smtp_server) => match send_email(subj, bod, &smtp_server, debug) {
                Ok(msg) => normalize_output(&format!("[Tool result] send_email: {}", msg)),
                Err(e) => normalize_output(&format!("[Tool error] send_email: {}", e)),
            },
            Err(e) => normalize_output(&format!("[Tool error] send_email: {}", e)),
        }
    } else {
        normalize_output("[Tool error] send_email: Missing required parameters")
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
    println!("{}", skin.term_text(content));
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

pub fn process_tool_calls(response: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool, quiet: bool) -> Result<(), String> {
    let mut current_response = response.clone();

    loop {
        let tool_calls = extract_tool_calls(&current_response);
        if debug && !tool_calls.is_empty() {
            println!("Tool calls found: {:?}", tool_calls);
        }

        if tool_calls.is_empty() {
            break;
        }

        let mut results = Vec::new();
        for (func_name, args) in tool_calls {
            match func_name.as_str() {
                "execute_command" => {
                    let result = process_execute_command(&args);
                    results.push(result);
                }
                "search_online" => {
                    let result = process_search_online(&args, chat_manager);
                    results.push(result);
                }
                "scrape_url" => {
                    let url = args.get("url").and_then(|u| u.as_str());
                    if let Some(u) = url {
                        let result = crate::scrape::scrape_url(u);
                        if result.starts_with("Error") || result.starts_with("Skipped") {
                            println!("Scrape failed: {}", result);
                        }
                        results.push(normalize_output(&format!("[Tool result] scrape_url: {}", result)));
                    } else {
                        results.push(
                            normalize_output("[Tool error] scrape_url: Missing 'url' parameter"),
                        );
                    }
                }
                "send_email" => {
                    let result = process_send_email(&args, chat_manager, debug);
                    results.push(result);
                }
                "alpha_vantage_query" => {
                    let function = args.get("function").and_then(|f| f.as_str());
                    let symbol = args.get("symbol").and_then(|s| s.as_str());
                    if let (Some(func), Some(sym)) = (function, symbol) {
                        match chat_manager.lock() {
                            Ok(manager) => {
                                let api_key = manager.get_alpha_vantage_api_key().to_string();
                                match alpha_vantage_query(func, sym, &api_key) {
                                    Ok(result) => results.push(normalize_output(&format!(
                                        "[Tool result] alpha_vantage_query: {}",
                                        result
                                    ))),
                                    Err(e) => results
                                        .push(normalize_output(&format!("[Tool error] alpha_vantage_query: {}", e))),
                                }
                            }
                            Err(_) => results.push(normalize_output("[Tool error] alpha_vantage_query: Failed to access configuration")),
                        }
                    } else {
                        results.push(
                            normalize_output("[Tool error] alpha_vantage_query: Missing required parameters"),
                        );
                    }
                }
                "file_editor" => {
                    let subcommand = args.get("subcommand").and_then(|s| s.as_str());
                    let filename = args.get("filename").and_then(|f| f.as_str());
                    let data = args.get("data").and_then(|d| d.as_str());
                    let replacement = args.get("replacement").and_then(|r| r.as_str());

                    if let (Some(subcmd), Some(fname)) = (subcommand, filename) {
                        let skip_confirmation = matches!(subcmd, "read" | "search"); // Only skip for non-destructive ops
                        let result = file_editor(subcmd, fname, data, replacement, skip_confirmation);
                        results.push(normalize_output(&format!("[Tool result] file_editor: {}", result)));
                    } else {
                        results.push(normalize_output("[Tool error] file_editor: Missing required parameters 'subcommand' or 'filename'"));
                    }
                }
                _ => {
                    results.push(normalize_output(&format!("[Tool error] Unknown function: {}", func_name)));
                }
            }
        }

        if !results.is_empty() {
            let combined_results = results.join("\n");
            let normalized_results = normalize_output(&combined_results);
            current_response = chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e))?.send_message(&normalized_results, quiet)?;
            display_response(&current_response);
        } else {
            break;
        }
    }

    Ok(())
}