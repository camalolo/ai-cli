use colored::{Color, Colorize};
use serde_json::{json, Value};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::Mutex;
use crate::command::execute_command;
use crate::search::search_online;
use crate::email::send_email;
use crate::alpha_vantage::alpha_vantage_query;
use crate::file_edit::file_editor;
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
            format!("[Tool result] execute_command: {}", result)
        } else {
            "[Tool result] execute_command: User rejected the command execution.".to_string()
        }
    } else {
        "[Tool error] execute_command: Missing 'command' parameter".to_string()
    }
}

pub fn process_search_online(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>) -> String {
    let query = args.get("query").and_then(|q| q.as_str());
    if let Some(q) = query {
        let api_key_result = chat_manager.lock().map(|manager| manager.get_google_search_api_key().to_string());
        let engine_id_result = chat_manager.lock().map(|manager| manager.get_google_search_engine_id().to_string());
        match (api_key_result, engine_id_result) {
            (Ok(api_key), Ok(engine_id)) => {
                let result = search_online(q, &api_key, &engine_id);
                format!("[Tool result] search_online: {}", result)
            }
            _ => "[Tool error] search_online: Failed to access configuration".to_string(),
        }
    } else {
        "[Tool error] search_online: Missing 'query' parameter".to_string()
    }
}

pub fn process_send_email(args: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> String {
    let subject = args.get("subject").and_then(|s| s.as_str());
    let body = args.get("body").and_then(|b| b.as_str());

    if let (Some(subj), Some(bod)) = (subject, body) {
        let smtp_server_result = chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e)).map(|manager| manager.get_smtp_server().to_string());
        match smtp_server_result {
            Ok(smtp_server) => match send_email(subj, bod, &smtp_server, debug) {
                Ok(msg) => format!("[Tool result] send_email: {}", msg),
                Err(e) => format!("[Tool error] send_email: {}", e),
            },
            Err(e) => format!("[Tool error] send_email: {}", e),
        }
    } else {
        "[Tool error] send_email: Missing required parameters".to_string()
    }
}

pub fn display_response(response: &Value) {
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("{}", content.color(Color::Yellow));
                }
            }
        }
    }
}

pub fn process_tool_calls(response: &Value, chat_manager: &Arc<Mutex<ChatManager>>, debug: bool) -> Result<(), String> {
    let mut current_response = response.clone();

    loop {
        let tool_calls: Vec<(String, Value)> = current_response
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
            .collect();

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
                        results.push(format!("[Tool result] scrape_url: {}", result));
                    } else {
                        results.push(
                            "[Tool error] scrape_url: Missing 'url' parameter".to_string(),
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
                        let api_key = {
                            let manager = chat_manager.lock().unwrap();
                            manager.get_alpha_vantage_api_key().to_string()
                        };
                        match alpha_vantage_query(func, sym, &api_key) {
                            Ok(result) => results.push(format!(
                                "[Tool result] alpha_vantage_query: {}",
                                result
                            )),
                            Err(e) => results
                                .push(format!("[Tool error] alpha_vantage_query: {}", e)),
                        }
                    } else {
                        results.push(
                            "[Tool error] alpha_vantage_query: Missing required parameters"
                                .to_string(),
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
                        results.push(format!("[Tool result] file_editor: {}", result));
                    } else {
                        results.push("[Tool error] file_editor: Missing required parameters 'subcommand' or 'filename'".to_string());
                    }
                }
                _ => {
                    results.push(format!("[Tool error] Unknown function: {}", func_name));
                }
            }
        }

        if !results.is_empty() {
            let combined_results = results.join("\n");
            current_response = chat_manager.lock().map_err(|e| format!("Failed to acquire chat manager lock: {}", e))?.send_message(&combined_results, false)?;
            display_response(&current_response);
        } else {
            break;
        }
    }

    Ok(())
}