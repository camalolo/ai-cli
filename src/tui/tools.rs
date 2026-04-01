use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, mpsc, oneshot};
use serde_json::{json, Value};

use crate::chat::ChatManager;

use super::types::AppEvent;
use super::llm::perform_streaming_call;

pub(crate) async fn run_tool_processing(
    chat_manager: Arc<Mutex<ChatManager>>,
    tx: mpsc::UnboundedSender<AppEvent>,
    response: Value,
    debug: bool,
    always_approve: Arc<AtomicBool>,
    cancel_stream: Arc<AtomicBool>,
) {
    let tool_calls = extract_tool_calls_from_response(&response);

    if tool_calls.is_empty() {
        return;
    }

    let mut tool_results: Vec<(String, String)> = Vec::new();
    let mut rejection_occurred = false;

    for (tool_call_id, func_name, args) in tool_calls {
        match func_name.as_str() {
            "execute_command" => {
                let cmd = args
                    .get("command")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let needs_confirm = !always_approve.load(Ordering::Relaxed);

                let confirmed = if needs_confirm {
                    let (send_confirm, recv_confirm) = oneshot::channel();
                    let _ = tx.send(AppEvent::NeedConfirmation {
                        prompt: format!("Execute command: {}?", cmd),
                        respond_to: send_confirm,
                    });
                    recv_confirm.await.unwrap_or(false)
                } else {
                    true
                };

                if confirmed {
                    let _ = tx.send(AppEvent::ToolCall {
                        name: "execute_command".into(),
                        args: format!("{{\"command\":\"{}\"}}", cmd),
                    });
                    let result = crate::command::execute_command(cmd, debug)
                        .await
                        .unwrap_or_else(|e| e.to_string());
                    let display_result = if result.len() > 200 {
                        crate::utils::truncate_str(&result, 200)
                    } else {
                        result.clone()
                    };
                    let _ = tx.send(AppEvent::ToolDone {
                        name: "execute_command".into(),
                        result: display_result,
                    });
                    tool_results.push((
                        tool_call_id,
                        format!("[Tool result] execute_command: {}", result),
                    ));
                } else {
                    let _ = tx.send(AppEvent::ToolError {
                        name: "execute_command".into(),
                        error: "User rejected".into(),
                    });
                    tool_results.push((
                        tool_call_id,
                        "[Tool result] execute_command: User rejected".into(),
                    ));
                    rejection_occurred = true;
                }
            }
            "search_online" => {
                let query = args
                    .get("query")
                    .and_then(|q| q.as_str())
                    .unwrap_or("");
                let include_results = crate::utils::get_opt_bool(&args, "include_results", false);
                let answer_mode = crate::utils::get_opt_str(&args, "answer_mode", "basic");
                let api_key = {
                    let manager = chat_manager.lock().await;
                    manager.get_tavily_api_key().to_string()
                };

                let _ = tx.send(AppEvent::ToolCall {
                    name: "search_online".into(),
                    args: format!("{{\"query\":\"{}\"}}", query),
                });

                let result =
                    crate::search::search_online(query, &api_key, include_results, &answer_mode, debug)
                        .await;
                let display_result = if result.len() > 200 {
                    crate::utils::truncate_str(&result, 200)
                } else {
                    result.clone()
                };
                let _ = tx.send(AppEvent::ToolDone {
                    name: "search_online".into(),
                    result: display_result,
                });
                tool_results.push((
                    tool_call_id,
                    format!("[Tool result] search_online: {}", result),
                ));
            }
            "scrape_url" => {
                let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let mode = crate::utils::get_opt_str(&args, "mode", "summarized");

                let _ = tx.send(AppEvent::ToolCall {
                    name: "scrape_url".into(),
                    args: format!("{{\"url\":\"{}\"}}", url),
                });

                let result = crate::scrape::scrape_url(url, &mode, debug)
                    .await
                    .unwrap_or_else(|e| e.to_string());
                let display_result = if result.len() > 200 {
                    crate::utils::truncate_str(&result, 200)
                } else {
                    result.clone()
                };
                let _ = tx.send(AppEvent::ToolDone {
                    name: "scrape_url".into(),
                    result: display_result,
                });
                tool_results.push((
                    tool_call_id,
                    format!("[Tool result] scrape_url: {}", result),
                ));
            }
            "send_email" => {
                let subject = crate::utils::get_opt_str(&args, "subject", "unknown");
                let body = crate::utils::get_opt_str(&args, "body", "");
                let needs_confirm = !always_approve.load(Ordering::Relaxed);

                let confirmed = if needs_confirm {
                    let (send_confirm, recv_confirm) = oneshot::channel();
                    let body_preview = body.chars().take(100).collect::<String>();
                    let _ = tx.send(AppEvent::NeedConfirmation {
                        prompt: format!(
                            "Send email? Subject: {}, Body: {}...",
                            subject, body_preview
                        ),
                        respond_to: send_confirm,
                    });
                    recv_confirm.await.unwrap_or(false)
                } else {
                    true
                };

                if confirmed {
                    let _ = tx.send(AppEvent::ToolCall {
                        name: "send_email".into(),
                        args: format!("{{\"subject\":\"{}\"}}", subject),
                    });
                    let config = {
                        let manager = chat_manager.lock().await;
                        manager.get_config().clone()
                    };
                    match crate::email::send_email(&subject, &body, &config, debug).await {
                        Ok(msg) => {
                            let _ = tx.send(AppEvent::ToolDone {
                                name: "send_email".into(),
                                result: msg.clone(),
                            });
                            tool_results.push((
                                tool_call_id,
                                format!("[Tool result] send_email: {}", msg),
                            ));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::ToolError {
                                name: "send_email".into(),
                                error: e.to_string(),
                            });
                            tool_results.push((
                                tool_call_id,
                                format!("[Tool error] send_email: {}", e),
                            ));
                        }
                    }
                } else {
                    tool_results.push((
                        tool_call_id,
                        "[Tool result] send_email: User rejected".into(),
                    ));
                    rejection_occurred = true;
                }
            }
            "alpha_vantage_query" => {
                let api_key = {
                    let manager = chat_manager.lock().await;
                    manager.get_alpha_vantage_api_key().to_string()
                };
                let function = args
                    .get("function")
                    .and_then(|f| f.as_str())
                    .unwrap_or("");
                let symbol = args
                    .get("symbol")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let outputsize = args.get("outputsize").and_then(|s| s.as_str());
                let limit = args
                    .get("limit")
                    .and_then(|l| l.as_u64())
                    .map(|l| l as usize);

                let _ = tx.send(AppEvent::ToolCall {
                    name: "alpha_vantage_query".into(),
                    args: format!("{{\"function\":\"{}\",\"symbol\":\"{}\"}}", function, symbol),
                });

                match crate::alpha_vantage::alpha_vantage_query(
                    function, symbol, &api_key, outputsize, limit, debug,
                )
                .await
                {
                    Ok(r) => {
                        let display = if r.len() > 200 {
                            crate::utils::truncate_str(&r, 200)
                        } else {
                            r.clone()
                        };
                        let _ = tx.send(AppEvent::ToolDone {
                            name: "alpha_vantage_query".into(),
                            result: display,
                        });
                        tool_results.push((
                            tool_call_id,
                            format!("[Tool result] alpha_vantage_query: {}", r),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::ToolError {
                            name: "alpha_vantage_query".into(),
                            error: e.to_string(),
                        });
                        tool_results.push((
                            tool_call_id,
                            format!("[Tool error] alpha_vantage_query: {}", e),
                        ));
                    }
                }
            }
            "file_editor" => {
                let filename = args
                    .get("filename")
                    .and_then(|f| f.as_str())
                    .unwrap_or("unknown");
                let subcommand = args
                    .get("subcommand")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let data = args.get("data").and_then(|d| d.as_str());
                let replacement = args.get("replacement").and_then(|r| r.as_str());

                let needs_confirm = !matches!(subcommand, "read" | "search")
                    && !always_approve.load(Ordering::Relaxed);

                let confirmed = if needs_confirm {
                    let (send_confirm, recv_confirm) = oneshot::channel();
                    let _ = tx.send(AppEvent::NeedConfirmation {
                        prompt: format!("File operation: {} on {}?", subcommand, filename),
                        respond_to: send_confirm,
                    });
                    recv_confirm.await.unwrap_or(false)
                } else {
                    true
                };

                if confirmed {
                    let _ = tx.send(AppEvent::ToolCall {
                        name: format!("file_editor({})", filename),
                        args: format!(
                            "{{\"subcommand\":\"{}\",\"filename\":\"{}\"}}",
                            subcommand, filename
                        ),
                    });
                    let (result, _rejected) = crate::file_edit::file_editor(
                        subcommand,
                        filename,
                        data,
                        replacement,
                        true,
                        debug,
                    );
                    let display = if result.len() > 200 {
                        crate::utils::truncate_str(&result, 200)
                    } else {
                        result.clone()
                    };
                    let _ = tx.send(AppEvent::ToolDone {
                        name: format!("file_editor({})", filename),
                        result: display,
                    });
                    tool_results.push((tool_call_id, result));
                } else {
                    tool_results.push((
                        tool_call_id,
                        format!(
                            "[Tool result] file_editor: User rejected {} on {}",
                            subcommand, filename
                        ),
                    ));
                    rejection_occurred = true;
                }
            }
            _ => {
                let _ = tx.send(AppEvent::ToolError {
                    name: func_name.clone(),
                    error: format!("Unknown function: {}", func_name),
                });
                tool_results.push((
                    tool_call_id,
                    format!("[Tool error] unknown: Unknown function: {}", func_name),
                ));
            }
        }
    }

    if !tool_results.is_empty() {
        let llm_data = {
            let mut manager = chat_manager.lock().await;
            manager.push_tool_results_to_history(&tool_results, debug);
            if rejection_occurred {
                return;
            }
            manager.prepare_llm_call()
        };

        let tx_clone = tx.clone();
        let cancel = cancel_stream;
        tokio::spawn(async move {
            let _ = tx_clone.send(AppEvent::LlmToken(String::new()));
            if let Some(result) =
                perform_streaming_call(llm_data, &tx_clone, &cancel, debug).await
            {
                let _ = tx_clone.send(AppEvent::LlmDone {
                    full_content: result.full_content,
                    full_response: Some(result.full_response),
                    new_messages: result.new_messages,
                });
            }
        });
    }
}

pub(crate) fn extract_tool_calls_from_response(response: &Value) -> Vec<(String, String, Value)> {
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
                            let tool_call_id = tc
                                .get("id")
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
