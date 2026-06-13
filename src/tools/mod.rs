pub mod alpha_vantage;
pub mod command;
pub mod email;
pub mod file_editor;
pub mod scrape;
pub mod search;

use std::sync::Arc;


/// Custom error type for tool execution (satisfies rig's `Tool::Error` bound).
#[derive(Debug)]
pub struct ToolRunError(pub String);

impl std::fmt::Display for ToolRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolRunError {}

impl From<String> for ToolRunError {
    fn from(s: String) -> Self {
        ToolRunError(s)
    }
}

impl From<&str> for ToolRunError {
    fn from(s: &str) -> Self {
        ToolRunError(s.to_string())
    }
}

/// Shared context for all tool implementations.
/// All operations are auto-approved — the LLM decides what to do.
pub struct ToolCtx {
    pub config: Arc<crate::config::Config>,
    pub debug: bool,
}

impl ToolCtx {
    pub fn new(config: Arc<crate::config::Config>, debug: bool) -> Self {
        ToolCtx { config, debug }
    }

    /// Print a tool notification to stderr.
    pub fn notify(&self, event_type: &str, tool_name: &str, message: &str) {
        match event_type {
            "call" => eprintln!("[{}] {}", tool_name, message),
            "done" => eprintln!("[{}] done", tool_name),
            "error" => eprintln!("[{}] error: {}", tool_name, message),
            _ => {}
        }
    }
}
