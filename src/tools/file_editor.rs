use std::sync::Arc;
use serde::Deserialize;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

use super::{ToolCtx, ToolRunError};

#[derive(Deserialize)]
pub struct FileEditorArgs {
    pub subcommand: String,
    pub filename: String,
    pub data: Option<String>,
    pub replacement: Option<String>,
}

pub struct FileEditorTool {
    pub ctx: Arc<ToolCtx>,
}

impl Tool for FileEditorTool {
    const NAME: &'static str = "file_editor";
    type Error = ToolRunError;
    type Args = FileEditorArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "file_editor".to_string(),
            description: "Edit files in the sandbox with sub-commands: read, write, search, search_and_replace, apply_diff.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subcommand": {
                        "type": "string",
                        "description": "The sub-command to execute: read, write, search, search_and_replace, apply_diff",
                        "enum": ["read", "write", "search", "search_and_replace", "apply_diff"]
                    },
                    "filename": {
                        "type": "string",
                        "description": "The name of the file in the sandbox to operate on"
                    },
                    "data": {
                        "type": "string",
                        "description": "Content to write (for write), regex pattern (for search/search_and_replace), or diff content (for apply_diff)"
                    },
                    "replacement": {
                        "type": "string",
                        "description": "Replacement text for search_and_replace"
                    }
                },
                "required": ["subcommand", "filename"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let summary = format!("subcommand={}, filename={}", args.subcommand, args.filename);
        self.ctx.notify("call", Self::NAME, &summary);

        // Auto-approve all file operations — skip_confirmation is always true
        let (result, _rejected) = crate::file_edit::file_editor(
            &args.subcommand,
            &args.filename,
            args.data.as_deref(),
            args.replacement.as_deref(),
            true, // skip_confirmation — auto-approve
            self.ctx.debug,
        );

        let display = if result.len() > 200 {
            crate::utils::truncate_str(&result, 200)
        } else {
            result.clone()
        };
        self.ctx.notify("done", Self::NAME, &display);

        Ok(result)
    }
}
