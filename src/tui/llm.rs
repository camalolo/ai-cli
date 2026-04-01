use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use tokio::sync::{Mutex, mpsc};
use serde_json::{json, Value};
use futures::StreamExt;

use crate::chat::{ChatManager, LlmCallData};

use super::types::{App, AppEvent, AppState, ChatMessage, StreamResult};

pub(crate) async fn start_llm_call(
    app: &mut App,
    user_input: String,
    chat_manager: &Arc<Mutex<ChatManager>>,
    tx: &mpsc::UnboundedSender<AppEvent>,
    debug: bool,
) {
    app.add_message(ChatMessage::User {
        content: user_input.clone(),
    });
    app.add_message(ChatMessage::Assistant {
        content: String::new(),
        is_streaming: true,
    });
    app.state = AppState::Streaming;
    app.cancel_stream.store(false, Ordering::Relaxed);

    let llm_data = {
        let mut manager = chat_manager.lock().await;
        manager.push_user_message(&user_input, debug);
        manager.prepare_llm_call()
    };

    let tx_clone = tx.clone();
    let cancel = app.cancel_stream.clone();
    tokio::spawn(async move {
        if let Some(result) = perform_streaming_call(llm_data, &tx_clone, &cancel, debug).await {
            let _ = tx_clone.send(AppEvent::LlmDone {
                full_content: result.full_content,
                full_response: Some(result.full_response),
                new_messages: result.new_messages,
            });
        }
    });
}

pub(crate) async fn perform_streaming_call(
    llm_data: LlmCallData,
    tx: &mpsc::UnboundedSender<AppEvent>,
    cancel: &Arc<AtomicBool>,
    debug: bool,
) -> Option<StreamResult> {
    use async_openai::types::{
        ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent,
        CreateChatCompletionRequest,
    };

    let mut chat_messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: ChatCompletionRequestSystemMessageContent::Text(
                llm_data.system_instruction.clone(),
            ),
            name: None,
        }),
    ];

    for msg in &llm_data.history {
        let message: ChatCompletionRequestMessage =
            match serde_json::from_value(msg.clone()) {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx.send(AppEvent::LlmError(format!("Failed to parse message: {}", e)));
                    return None;
                }
            };
        chat_messages.push(message);
    }

    let request = CreateChatCompletionRequest {
        model: llm_data.model.clone(),
        messages: chat_messages,
        tools: Some(llm_data.tools.clone()),
        stream: Some(true),
        ..Default::default()
    };

    crate::utils::log_to_file(debug, "TUI: Starting streaming LLM call...");

    let stream = match llm_data.client.chat().create_stream(request).await {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(AppEvent::LlmError(format!("Failed to start stream: {}", e)));
            return None;
        }
    };

    let mut full_content = String::new();
    let mut tool_calls_acc: HashMap<u32, (String, String, String, String)> =
        HashMap::new();
    let mut finish_reason: Option<String> = None;

    let mut pinned = std::pin::pin!(stream);

    while let Some(result) = pinned.next().await {
        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(AppEvent::LlmError("Stream cancelled by user.".into()));
            return None;
        }
        match result {
            Ok(response) => {
                for choice in &response.choices {
                    let delta = &choice.delta;
                    if let Some(content) = &delta.content {
                        let _ = tx.send(AppEvent::LlmToken(content.clone()));
                        full_content.push_str(content);
                    }
                    if let Some(tc_deltas) = &delta.tool_calls {
                        for tc_delta in tc_deltas {
                            let idx = tc_delta.index as u32;
                            let entry = tool_calls_acc.entry(idx).or_insert_with(|| {
                                (String::new(), String::new(), String::new(), String::new())
                            });
                            if let Some(id) = &tc_delta.id {
                                entry.0 = id.clone();
                            }
                            if let Some(ref func) = tc_delta.function {
                                if let Some(name) = &func.name {
                                    entry.1 = name.clone();
                                }
                                if let Some(args) = &func.arguments {
                                    entry.3.push_str(args);
                                }
                            }
                        }
                    }
                    if let Some(reason) = &choice.finish_reason {
                        finish_reason = Some(format!("{:?}", reason));
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(AppEvent::LlmError(format!("Stream error: {}", e)));
                return None;
            }
        }
    }

    crate::utils::log_to_file(
        debug,
        &format!(
            "TUI: Stream completed, {} content chars, {} tool calls",
            full_content.len(),
            tool_calls_acc.len()
        ),
    );

    let mut tool_calls_json = Vec::new();
    for (_, (id, name, _, raw_args)) in &tool_calls_acc {
        let args_map: serde_json::Map<String, Value> = serde_json::from_str(raw_args).unwrap_or_default();
        let args_json = serde_json::Value::Object(args_map);
        tool_calls_json.push(json!({
            "id": id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": serde_json::to_string(&args_json).unwrap_or_default()
            }
        }));
    }

    let content_value = if full_content.is_empty() {
        Value::Null
    } else {
        json!(full_content)
    };

    let msg_for_history = if !tool_calls_json.is_empty() {
        json!({
            "role": "assistant",
            "content": content_value,
            "tool_calls": tool_calls_json
        })
    } else {
        json!({
            "role": "assistant",
            "content": content_value
        })
    };

    let full_response = json!({
        "choices": [{
            "message": msg_for_history,
            "finish_reason": finish_reason.unwrap_or_else(|| "stop".to_string())
        }]
    });

    Some(StreamResult {
        full_content,
        full_response,
        new_messages: vec![msg_for_history],
    })
}
