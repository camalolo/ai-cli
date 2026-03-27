use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use tokio::sync::{Mutex, mpsc, oneshot};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame,
    Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use tui_textarea::TextArea;
use serde_json::{json, Value};
use futures::StreamExt;

use crate::chat::{ChatManager, LlmCallData, LlmCallResult};

fn word_wrap_line(text: &str, max_width: usize, style: Style) -> Vec<Line<'static>> {
    let mut result = Vec::new();
    let max_width = max_width.max(10);
    for paragraph in text.split('\n') {
        if paragraph.chars().count() <= max_width {
            result.push(Line::from(Span::styled(paragraph.to_string(), style)));
            continue;
        }
        let mut current_line = String::new();
        let mut current_len: usize = 0;
        for word in paragraph.split(' ') {
            let word_len = word.chars().count();
            if word_len > max_width {
                if !current_line.is_empty() {
                    result.push(Line::from(Span::styled(current_line.clone(), style)));
                    current_line.clear();
                    current_len = 0;
                }
                let mut chunk_start = 0;
                for (i, _ch) in word.char_indices() {
                    if i >= chunk_start && current_len == max_width {
                        let chunk: String = word[chunk_start..i].to_string();
                        result.push(Line::from(Span::styled(chunk, style)));
                        chunk_start = i;
                        current_len = 0;
                    }
                    current_len += 1;
                }
                if chunk_start < word.len() {
                    let chunk = word[chunk_start..].to_string();
                    current_line = chunk.clone();
                    current_len = chunk.chars().count();
                }
            } else if current_line.is_empty() {
                current_line = word.to_string();
                current_len = word_len;
            } else if current_len + 1 + word_len <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
                current_len += 1 + word_len;
            } else {
                result.push(Line::from(Span::styled(current_line.clone(), style)));
                current_line = word.to_string();
                current_len = word_len;
            }
        }
        if !current_line.is_empty() {
            result.push(Line::from(Span::styled(current_line, style)));
        }
    }
    if result.is_empty() {
        result.push(Line::from(Span::styled(String::new(), style)));
    }
    result
}

enum ChatMessage {
    User { content: String },
    Assistant { content: String, is_streaming: bool },
    ToolCall { name: String, args: String },
    ToolResult { name: String, result: String },
    Error { message: String },
    Info { message: String },
}

enum AppState {
    Idle,
    Streaming,
    WaitingConfirmation {
        prompt: String,
        respond_to: oneshot::Sender<bool>,
    },
    ProcessingTools,
}

enum AppEvent {
    Key(KeyEvent),
    LlmToken(String),
    LlmDone {
        full_content: String,
        full_response: Option<Value>,
        new_messages: Vec<Value>,
    },
    LlmError(String),
    NeedConfirmation {
        prompt: String,
        respond_to: oneshot::Sender<bool>,
    },
    ToolDone { name: String, result: String },
    ToolCall { name: String, args: String },
    ToolError { name: String, error: String },
    ShellCommandDone { command: String, output: String },
}

struct App {
    messages: Vec<ChatMessage>,
    state: AppState,
    scroll_offset: u16,
    auto_scroll: bool,
    input: TextArea<'static>,
    model: String,
    should_quit: bool,
    always_approve: Arc<AtomicBool>,
    cancel_stream: Arc<AtomicBool>,
}

impl App {
    fn new(model: String, always_approve: Arc<AtomicBool>) -> Self {
        App {
            messages: Vec::new(),
            state: AppState::Idle,
            scroll_offset: 0,
            auto_scroll: true,
            input: Self::make_textarea(),
            model,
            should_quit: false,
            always_approve,
            cancel_stream: Arc::new(AtomicBool::new(false)),
        }
    }

    fn make_textarea() -> TextArea<'static> {
        let mut ta = TextArea::default();
        ta.set_style(Style::default().fg(Color::White));
        ta.set_cursor_line_style(Style::default());
        ta.set_placeholder_text("Type a message...");
        ta.set_block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        ta
    }

    fn reset_input(&mut self) {
        self.input = Self::make_textarea();
    }

    fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.auto_scroll = true;
    }

    fn append_to_streaming_message(&mut self, token: &str) {
        if let Some(last) = self.messages.last_mut() {
            if let ChatMessage::Assistant { content, .. } = last {
                content.push_str(token);
                self.auto_scroll = true;
            }
        }
    }

    fn finalize_streaming_message(&mut self, full_content: &str) {
        if let Some(last) = self.messages.last_mut() {
            if let ChatMessage::Assistant { content, is_streaming } = last {
                *content = full_content.to_string();
                *is_streaming = false;
            }
        }
    }
}

type TuiTerminal = RatatuiTerminal<CrosstermBackend<std::io::Stdout>>;

fn init_terminal() -> Result<TuiTerminal> {
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let terminal = RatatuiTerminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(mut terminal: TuiTerminal) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(size);

    let chat_area = chunks[0];
    let input_area = chunks[1];
    let status_area = chunks[2];

    let max_width = chat_area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg {
            ChatMessage::User { content } => {
                lines.extend(word_wrap_line(
                    &format!("> {}", content),
                    max_width,
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ));
                lines.push(Line::from(""));
            }
            ChatMessage::Assistant { content, is_streaming } => {
                if content.is_empty() && *is_streaming {
                    lines.push(Line::from(Span::styled(
                        "\u{2588}",
                        Style::default().fg(Color::Yellow),
                    )));
                } else {
                    lines.extend(word_wrap_line(
                        content,
                        max_width,
                        Style::default().fg(Color::White),
                    ));
                }
                if *is_streaming {
                    lines.push(Line::from(Span::styled(
                        "\u{2588}",
                        Style::default().fg(Color::Yellow),
                    )));
                }
                lines.push(Line::from(""));
            }
            ChatMessage::ToolCall { name, args } => {
                let preview = if args.len() > 100 {
                    crate::utils::truncate_str(args, 100)
                } else {
                    args.clone()
                };
                lines.extend(word_wrap_line(
                    &format!("  [Tool: {}] {}", name, preview),
                    max_width,
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ChatMessage::ToolResult { name, result } => {
                let preview = if result.len() > 200 {
                    crate::utils::truncate_str(result, 200)
                } else {
                    result.clone()
                };
                lines.extend(word_wrap_line(
                    &format!("  [Result: {}] {}", name, preview),
                    max_width,
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ChatMessage::Error { message } => {
                lines.extend(word_wrap_line(
                    &format!("  Error: {}", message),
                    max_width,
                    Style::default().fg(Color::Red),
                ));
                lines.push(Line::from(""));
            }
            ChatMessage::Info { message } => {
                lines.extend(word_wrap_line(
                    &format!("  {}", message),
                    max_width,
                    Style::default().fg(Color::Cyan),
                ));
                lines.push(Line::from(""));
            }
        }
    }

    if app.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "Welcome to AI CLI! Type a message to start chatting.",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Commands: /exit (quit), /clear (reset), !command (shell)",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Key bindings: Enter=Send, PageUp/PageDown=Scroll, Ctrl+C=Cancel, Ctrl+D=Quit",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let total_lines = lines.len() as u16;
    let visible_height = chat_area.height;
    let max_scroll = total_lines.saturating_sub(visible_height);

    if app.auto_scroll {
        app.scroll_offset = max_scroll;
    }
    let actual_scroll = app.scroll_offset.min(max_scroll);

    let chat_widget = Paragraph::new(lines)
        .scroll((0, actual_scroll));
    f.render_widget(chat_widget, chat_area);

    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state = ScrollbarState::new(total_lines as usize)
            .position(actual_scroll as usize);
        f.render_stateful_widget(scrollbar, chat_area, &mut scrollbar_state);
    }

    match &app.state {
        AppState::WaitingConfirmation { prompt, .. } => {
            let confirm_text = Paragraph::new(Line::from(vec![
                Span::styled(prompt.clone(), Style::default().fg(Color::Yellow)),
                Span::raw(" [y/n/a]: "),
            ]))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            f.render_widget(confirm_text, input_area);
        }
        _ => {
            f.render_widget(&app.input, input_area);
            let (cursor_y, cursor_x) = app.input.cursor();
            let cursor_y = cursor_y as u16;
            let cursor_x = cursor_x as u16;
            let safe_y = cursor_y.min(input_area.height.saturating_sub(1));
            let safe_x = cursor_x.min(input_area.width.saturating_sub(1));
            f.set_cursor_position((
                input_area.x.saturating_add(safe_x),
                input_area.y.saturating_add(safe_y),
            ));
        }
    }

    let status_line = match &app.state {
        AppState::Streaming => Line::from(vec![
            Span::styled(format!(" {} ", app.model), Style::default().fg(Color::White)),
            Span::raw("\u{2502}"),
            Span::styled(" \u{25cf} Streaming... ", Style::default().fg(Color::Yellow)),
            Span::raw("\u{2502}"),
            Span::styled(" Ctrl+C:Cancel ", Style::default().fg(Color::DarkGray)),
        ]),
        AppState::WaitingConfirmation { .. } => Line::from(vec![
            Span::styled(format!(" {} ", app.model), Style::default().fg(Color::White)),
            Span::raw("\u{2502}"),
            Span::styled(
                " y=Yes  n=No  a=Always approve ",
                Style::default().fg(Color::Yellow),
            ),
        ]),
        AppState::ProcessingTools => Line::from(vec![
            Span::styled(format!(" {} ", app.model), Style::default().fg(Color::White)),
            Span::raw("\u{2502}"),
            Span::styled(" Processing tools... ", Style::default().fg(Color::Yellow)),
        ]),
        AppState::Idle => Line::from(vec![
            Span::styled(format!(" {} ", app.model), Style::default().fg(Color::White)),
            Span::raw("\u{2502}"),
            Span::styled(
                " Enter:Send  /exit:Quit  /clear:Reset  !cmd:Shell ",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    };

    let status_bar = Paragraph::new(status_line)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(status_bar, status_area);
}

async fn handle_event(
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
                let tool_calls = extract_tool_calls_from_response(response);
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

async fn handle_key_event(
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

async fn start_llm_call(
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

struct StreamResult {
    full_content: String,
    full_response: Value,
    new_messages: Vec<Value>,
}

async fn perform_streaming_call(
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

async fn run_tool_processing(
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

fn extract_tool_calls_from_response(response: &Value) -> Vec<(String, String, Value)> {
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

pub async fn run_tui(
    chat_manager: Arc<Mutex<ChatManager>>,
    debug: bool,
    always_approve: Arc<AtomicBool>,
) -> Result<()> {
    let mut terminal = init_terminal()?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let tx_input = tx.clone();

    let model_name = {
        let manager = chat_manager.lock().await;
        manager.get_config().model.clone()
    };

    tokio::task::spawn_blocking(move || {
        loop {
            if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    if tx_input.send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut app = App::new(model_name, always_approve);

    loop {
        terminal.draw(|f| render(f, &mut app))?;
        terminal.hide_cursor()?;

        match rx.recv().await {
            Some(event) => {
                handle_event(&mut app, event, &chat_manager, &tx, debug).await?;
            }
            None => break,
        }

        if app.should_quit {
            break;
        }
    }

    restore_terminal(terminal)?;
    Ok(())
}
