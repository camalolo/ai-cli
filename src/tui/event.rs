use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{Mutex, mpsc};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde_json::json;

use crate::chat::{ChatManager, LlmCallResult};

use super::types::{App, AppEvent, AppState, ChatMessage};
use super::llm::start_llm_call;
use super::tools::run_tool_processing;

pub(crate) async fn handle_event(
    app: &mut App,
    event: AppEvent,
    chat_manager: &Arc<Mutex<ChatManager>>,
    tx: &mpsc::UnboundedSender<AppEvent>,
    debug: bool,
) -> Result<()> {
    match event {
        AppEvent::Key(key) => {
            if matches!(key.kind, KeyEventKind::Release) {
                return Ok(());
            }
            handle_key_event(app, key, chat_manager, tx, debug).await
        }
        AppEvent::LlmToken(token) => {
            match app.messages.last() {
                Some(ChatMessage::Assistant { is_streaming: true, .. }) => {
                    if !token.is_empty() {
                        app.append_to_streaming_message(&token);
                    }
                }
                _ => {
                    app.add_message(ChatMessage::Assistant {
                        content: String::new(),
                        is_streaming: true,
                    });
                    app.state = AppState::Streaming;
                    if !token.is_empty() {
                        app.append_to_streaming_message(&token);
                    }
                }
            }
            app.auto_scroll = true;
            Ok(())
        }
        AppEvent::LlmDone {
            full_content,
            full_response,
            new_messages,
        } => {
            app.finalize_streaming_message(&full_content);

            if !new_messages.is_empty() {
                let result = LlmCallResult {
                    response: full_response.clone().unwrap_or(json!({})),
                    new_messages,
                };
                chat_manager.lock().await.apply_llm_result(&result);
            }

            if let Some(ref response) = full_response {
                let tool_calls = crate::tui::tools::extract_tool_calls_from_response(response);
                if !tool_calls.is_empty() {
                    tokio::spawn(run_tool_processing(
                        chat_manager.clone(),
                        tx.clone(),
                        response.clone(),
                        debug,
                        app.always_approve.clone(),
                        app.cancel_stream.clone(),
                    ));
                    app.state = AppState::ProcessingTools;
                    return Ok(());
                }
            }

            app.state = AppState::Idle;
            Ok(())
        }
        AppEvent::LlmError(message) => {
            if let Some(last) = app.messages.last_mut() {
                if let ChatMessage::Assistant { content, is_streaming } = last {
                    if *is_streaming {
                        *is_streaming = false;
                        if content.is_empty() {
                            *content = "(error)".to_string();
                        }
                    }
                }
            }
            app.add_message(ChatMessage::Error { message });
            app.state = AppState::Idle;
            Ok(())
        }
        AppEvent::NeedConfirmation { prompt, respond_to } => {
            app.state = AppState::WaitingConfirmation { prompt, respond_to };
            Ok(())
        }
        AppEvent::ToolDone { name, result } => {
            app.add_message(ChatMessage::ToolResult { name, result });
            Ok(())
        }
        AppEvent::ToolCall { name, args } => {
            app.add_message(ChatMessage::ToolCall { name: name.clone(), args });
            Ok(())
        }
        AppEvent::ToolError { name, error } => {
            app.add_message(ChatMessage::Error {
                message: format!("{}: {}", name, error),
            });
            Ok(())
        }
        AppEvent::ShellCommandDone { command, output } => {
            app.add_message(ChatMessage::Info {
                message: format!("Output:\n{}", output),
            });
            let llm_input = format!("User ran command '!{}' with output: {}", command, output);
            start_llm_call(app, llm_input, chat_manager, tx, debug).await;
            Ok(())
        }
    }
}

pub(crate) async fn handle_key_event(
    app: &mut App,
    key: KeyEvent,
    chat_manager: &Arc<Mutex<ChatManager>>,
    tx: &mpsc::UnboundedSender<AppEvent>,
    debug: bool,
) -> Result<()> {
    match &app.state {
        AppState::WaitingConfirmation { .. } => {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let old_state = std::mem::replace(&mut app.state, AppState::ProcessingTools);
                    if let AppState::WaitingConfirmation { prompt: _, respond_to } = old_state {
                        let _ = respond_to.send(true);
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    let old_state = std::mem::replace(&mut app.state, AppState::Idle);
                    if let AppState::WaitingConfirmation { prompt: _, respond_to } = old_state {
                        let _ = respond_to.send(false);
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    let old_state = std::mem::replace(&mut app.state, AppState::ProcessingTools);
                    if let AppState::WaitingConfirmation { prompt: _, respond_to } = old_state {
                        let _ = respond_to.send(true);
                    }
                    app.always_approve.store(true, Ordering::Relaxed);
                }
                _ => {}
            }
        }
        AppState::Streaming => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.cancel_stream.store(true, Ordering::Relaxed);
            }
        }
        AppState::ProcessingTools => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.cancel_stream.store(true, Ordering::Relaxed);
            }
        }
        AppState::Idle => {
            match key.code {
                KeyCode::Enter => {
                    let input_text: String = app.input.lines().join("\n");
                    let input_text = input_text.trim().to_string();
                    app.reset_input();

                    if input_text.is_empty() {
                        return Ok(());
                    }

                    let input_lower = input_text.to_lowercase();
                    if input_lower == "/exit" {
                        app.should_quit = true;
                        return Ok(());
                    }
                    if input_lower == "/clear" {
                        app.messages.clear();
                        chat_manager.lock().await.create_chat();
                        app.scroll_offset = 0;
                        app.auto_scroll = true;
                        return Ok(());
                    }

                    if let Some(command) = input_text.strip_prefix('!') {
                        let command = command.trim();
                        if command.is_empty() {
                            app.add_message(ChatMessage::Info {
                                message: "Interactive shell mode is not available in TUI. Use !command syntax instead.".into(),
                            });
                        } else {
                            app.add_message(ChatMessage::Info {
                                message: format!("Running: {}", command),
                            });
                            app.state = AppState::ProcessingTools;
                            let tx_clone = tx.clone();
                            let cmd_owned = command.to_string();
                            tokio::spawn(async move {
                                let output = crate::command::execute_command(&cmd_owned, debug)
                                    .await
                                    .unwrap_or_else(|e| e.to_string());
                                let _ = tx_clone.send(AppEvent::ShellCommandDone {
                                    command: cmd_owned,
                                    output,
                                });
                            });
                        }
                    } else {
                        start_llm_call(app, input_text, chat_manager, tx, debug).await;
                    }
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.should_quit = true;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if !app.input.lines().join("").trim().is_empty() {
                        app.input = App::make_textarea();
                    }
                }
                KeyCode::PageUp => {
                    app.auto_scroll = false;
                    app.scroll_offset = app.scroll_offset.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    app.auto_scroll = false;
                    app.scroll_offset = app.scroll_offset.saturating_add(10);
                }
                _ => {
                    app.input.input(key);
                }
            }
        }
    }
    Ok(())
}
