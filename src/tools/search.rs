use std::sync::Arc;
use serde::Deserialize;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

use super::{ToolCtx, ToolRunError};

#[derive(Deserialize)]
pub struct SearchArgs {
    pub query: String,
    pub include_results: Option<bool>,
    pub answer_mode: Option<String>,
}

pub struct SearchTool {
    pub ctx: Arc<ToolCtx>,
}

impl Tool for SearchTool {
    const NAME: &'static str = "search_online";
    type Error = ToolRunError;
    type Args = SearchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "search_online".to_string(),
            description: "Search the web for a query and return a synthesized answer. Use for factual lookups, current events, or research. Defaults to concise summaries for speed.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "include_results": {
                        "type": "boolean",
                        "description": "Whether to include a list of search results (default: false). Set to true only if you need to review sources directly.",
                        "default": false
                    },
                    "answer_mode": {
                        "type": "string",
                        "enum": ["basic", "full"],
                        "description": "Answer detail level. 'basic' (default): Quick summary. 'full': Comprehensive answer.",
                        "default": "basic"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let display_query = crate::utils::truncate_str(&args.query, 80);
        self.ctx.notify("call", Self::NAME, &format!("query={}", display_query));

        let include_results = args.include_results.unwrap_or(false);
        let answer_mode = args.answer_mode.as_deref().unwrap_or("basic");

        let result = crate::search::search_online(
            &args.query,
            &self.ctx.config.tavily_api_key,
            include_results,
            answer_mode,
            self.ctx.debug,
        )
        .await;

        let display = if result.len() > 200 {
            crate::utils::truncate_str(&result, 200)
        } else {
            result.clone()
        };
        self.ctx.notify("done", Self::NAME, &display);

        Ok(result)
    }
}
