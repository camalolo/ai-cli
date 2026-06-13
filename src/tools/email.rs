use std::sync::Arc;
use serde::Deserialize;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

use super::{ToolCtx, ToolRunError};

#[derive(Deserialize)]
pub struct EmailArgs {
    pub subject: String,
    pub body: String,
}

pub struct EmailTool {
    pub ctx: Arc<ToolCtx>,
}

impl Tool for EmailTool {
    const NAME: &'static str = "send_email";
    type Error = ToolRunError;
    type Args = EmailArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "send_email".to_string(),
            description: "Sends an email to a fixed address using SMTP.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": {
                        "type": "string",
                        "description": "Email subject line"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email message body"
                    }
                },
                "required": ["subject", "body"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let body_preview: String = args.body.chars().take(100).collect();
        let summary = format!("subject={}, body={}...", args.subject, body_preview);
        self.ctx.notify("call", Self::NAME, &summary);

        let result = crate::email::send_email(&args.subject, &args.body, &self.ctx.config, self.ctx.debug)
            .await
            .map_err(|e| ToolRunError(format!("Email failed: {}", e)))?;

        let display = if result.len() > 200 {
            crate::utils::truncate_str(&result, 200)
        } else {
            result.clone()
        };
        self.ctx.notify("done", Self::NAME, &display);

        Ok(result)
    }
}
