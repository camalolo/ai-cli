use anyhow::{Context, Result};
use serde::Deserialize;
use config::{Config as ConfigLoader, Environment, File, FileFormat};
use std::env;

/// Configuration structure holding all settings for the AI CLI
#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    // AI Provider Configuration
    pub api_base_url: String,
    pub api_version: String,
    pub model: String,
    pub api_key: String,

    // SMTP Configuration
    pub smtp_server: String,
    pub smtp_username: String,
    pub smtp_password: String,
    pub destination_email: String,
    pub sender_email: String,

    // Optional: Search APIs
    pub google_search_api_key: String,
    pub google_search_engine_id: String,
    pub alpha_vantage_api_key: String,
}

impl Config {
    /// Load configuration from ~/.aicli.toml file and environment variables
    pub fn load() -> Result<Self> {
        let home_dir = dirs::home_dir()
            .context("Could not determine home directory")?;

        let config_path = home_dir.join(".aicli.conf");

        let loader = ConfigLoader::builder()
            .add_source(File::from(config_path).format(FileFormat::Ini).required(false))
            .add_source(Environment::with_prefix(""))
            .build()
            .context("Failed to build config")?;

        let mut config: Config = loader.try_deserialize().context("Failed to deserialize config")?;

        // Override SMTP_SERVER_IP with SMTP_SERVER for backwards compatibility
        if let Ok(smtp_ip) = env::var("SMTP_SERVER_IP") {
            config.smtp_server = smtp_ip;
        }

        Ok(config)
    }
    
    /// Construct the API endpoint URL - always use OpenAI-compatible format
    pub fn get_api_endpoint(&self) -> String {
        // Always use OpenAI-compatible chat/completions endpoint
        // Google Gemini also supports OpenAI-compatible endpoints
        format!("{}/{}/chat/completions", self.api_base_url, self.api_version)
    }


    
    
    /// Get authentication method - always use Bearer token in header
    pub fn get_auth_header(&self) -> Option<String> {
        if self.api_key.is_empty() {
            None
        } else {
            Some(format!("Bearer {}", self.api_key))
        }
    }
    

    
    /// Display configuration summary (for debug mode)
    pub fn display_summary(&self) {
        println!("=== AI Provider Configuration ===");
        println!("API Base URL: {}", self.api_base_url);
        println!("API Version: {}", self.api_version);
        println!("Model: {}", self.model);
        println!("API Key: {}***",
            if self.api_key.len() > 4 {
                &self.api_key[..4]
            } else {
                "***"
            }
        );
        println!("Endpoint: {}", self.get_api_endpoint());
        println!("Auth Method: Header (Bearer)");
        println!("================================");
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.openai.com".to_string(),
            api_version: "v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key: "".to_string(),
            smtp_server: "localhost".to_string(),
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            destination_email: "".to_string(),
            sender_email: "".to_string(),
            google_search_api_key: "".to_string(),
            google_search_engine_id: "".to_string(),
            alpha_vantage_api_key: "".to_string(),
        }
    }
}