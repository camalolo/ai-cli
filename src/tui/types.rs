use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Style},
    widgets::{Block, Borders},
    Terminal as RatatuiTerminal,
};
use serde_json::Value;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::oneshot;
use tui_textarea::TextArea;

pub(crate) enum ChatMessage {
    User { content: String },
    Assistant { content: String, is_streaming: bool },
    ToolCall { name: String, args: String },
    ToolResult { name: String, result: String },
    Error { message: String },
    Info { message: String },
}

pub(crate) enum AppState {
    Idle,
    Streaming,
    WaitingConfirmation {
        prompt: String,
        respond_to: oneshot::Sender<bool>,
    },
    ProcessingTools,
}

pub(crate) enum AppEvent {
    Key(crossterm::event::KeyEvent),
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
    ToolDone {
        name: String,
        result: String,
    },
    ToolCall {
        name: String,
        args: String,
    },
    ToolError {
        name: String,
        error: String,
    },
    ShellCommandDone {
        command: String,
        output: String,
    },
}

pub(crate) struct App {
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) state: AppState,
    pub(crate) scroll_offset: u16,
    pub(crate) auto_scroll: bool,
    pub(crate) input: TextArea<'static>,
    pub(crate) model: String,
    pub(crate) should_quit: bool,
    pub(crate) always_approve: Arc<AtomicBool>,
    pub(crate) cancel_stream: Arc<AtomicBool>,
    pub(crate) tick_counter: u32,
}

impl App {
    pub(crate) fn new(model: String, always_approve: Arc<AtomicBool>) -> Self {
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
            tick_counter: 0,
        }
    }

    pub(crate) fn make_textarea() -> TextArea<'static> {
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

    pub(crate) fn reset_input(&mut self) {
        self.input = Self::make_textarea();
    }

    pub(crate) fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.auto_scroll = true;
    }

    pub(crate) fn append_to_streaming_message(&mut self, token: &str) {
        if let Some(last) = self.messages.last_mut() {
            if let ChatMessage::Assistant { content, .. } = last {
                content.push_str(token);
                self.auto_scroll = true;
            }
        }
    }

    pub(crate) fn finalize_streaming_message(&mut self, full_content: &str) {
        if let Some(last) = self.messages.last_mut() {
            if let ChatMessage::Assistant {
                content,
                is_streaming,
            } = last
            {
                *content = full_content.to_string();
                *is_streaming = false;
            }
        }
    }
}

pub(crate) type TuiTerminal = RatatuiTerminal<CrosstermBackend<std::io::Stdout>>;

pub(crate) struct StreamResult {
    pub(crate) full_content: String,
    pub(crate) full_response: Value,
    pub(crate) new_messages: Vec<Value>,
}
