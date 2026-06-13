use std::sync::Arc;
use serde::Deserialize;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

use super::{ToolCtx, ToolRunError};

#[derive(Deserialize)]
pub struct ScrapeArgs {
    pub url: String,
    pub mode: Option<String>,
}

pub struct ScrapeTool {
    pub ctx: Arc<ToolCtx>,
}

impl Tool for ScrapeTool {
    const NAME: &'static str = "scrape_url";
    type Error = ToolRunError;
    type Args = ScrapeArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "scrape_url".to_string(),
            description: "Scrapes the content of a single URL".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to scrape"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["summarized", "full"],
                        "default": "summarized",
                        "description": "Mode: 'summarized' provides a concise summary (default), 'full' returns complete extracted text"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let display_url = crate::utils::truncate_str(&args.url, 80);
        self.ctx.notify("call", Self::NAME, &format!("url={}", display_url));

        let mode = args.mode.as_deref().unwrap_or("summarized");

        let result = crate::scrape::scrape_url(&args.url, mode, self.ctx.debug)
            .await
            .map_err(|e| ToolRunError(format!("Scrape failed: {}", e)))?;

        let display = if result.len() > 200 {
            crate::utils::truncate_str(&result, 200)
        } else {
            result.clone()
        };
        self.ctx.notify("done", Self::NAME, &display);

        Ok(result)
    }
}
