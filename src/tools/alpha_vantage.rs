use std::sync::Arc;
use serde::Deserialize;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

use super::{ToolCtx, ToolRunError};

#[derive(Deserialize)]
pub struct AlphaVantageArgs {
    pub function: String,
    pub symbol: String,
    pub outputsize: Option<String>,
    pub limit: Option<u64>,
}

pub struct AlphaVantageTool {
    pub ctx: Arc<ToolCtx>,
}

impl Tool for AlphaVantageTool {
    const NAME: &'static str = "alpha_vantage_query";
    type Error = ToolRunError;
    type Args = AlphaVantageArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "alpha_vantage_query".to_string(),
            description: "Query the Alpha Vantage API for stock/financial data".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "function": {
                        "type": "string",
                        "description": "The Alpha Vantage function (e.g., TIME_SERIES_DAILY)"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "The stock symbol (e.g., IBM)"
                    },
                    "outputsize": {
                        "type": "string",
                        "enum": ["compact", "full"],
                        "description": "The size of the output data. 'compact' returns the last 100 data points, 'full' returns all available data. Defaults to 'compact'."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of most recent data points to return (default 5)",
                        "default": 5
                    }
                },
                "required": ["function", "symbol"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let summary = format!("function={}, symbol={}", args.function, args.symbol);
        self.ctx.notify("call", Self::NAME, &summary);

        let outputsize = args.outputsize.as_deref();
        let limit = args.limit.map(|l| l as usize);

        let result = crate::alpha_vantage::alpha_vantage_query(
            &args.function,
            &args.symbol,
            &self.ctx.config.alpha_vantage_api_key,
            outputsize,
            limit,
            self.ctx.debug,
        )
        .await
        .map_err(|e| ToolRunError(format!("Alpha Vantage query failed: {}", e)))?;

        let display = if result.len() > 200 {
            crate::utils::truncate_str(&result, 200)
        } else {
            result.clone()
        };
        self.ctx.notify("done", Self::NAME, &display);

        Ok(result)
    }
}
