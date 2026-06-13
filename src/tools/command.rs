use std::sync::Arc;
use serde::Deserialize;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

use super::{ToolCtx, ToolRunError};

#[derive(Deserialize)]
pub struct CommandArgs {
    pub command: String,
}

pub struct CommandTool {
    pub ctx: Arc<ToolCtx>,
}

impl Tool for CommandTool {
    const NAME: &'static str = "execute_command";
    type Error = ToolRunError;
    type Args = CommandArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "execute_command".to_string(),
            description: "Execute a system command. Use this for any shell task.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let display_cmd = crate::utils::truncate_str(&args.command, 100);
        self.ctx.notify("call", Self::NAME, &format!("command={}", display_cmd));

        let result = crate::command::execute_command(&args.command, self.ctx.debug)
            .await
            .unwrap_or_else(|e| e.to_string());

        let display = if result.len() > 200 {
            crate::utils::truncate_str(&result, 200)
        } else {
            result.clone()
        };
        self.ctx.notify("done", Self::NAME, &display);

        Ok(result)
    }
}
