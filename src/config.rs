use anyhow::{Context, Result};
use serde::Deserialize;

/// Configuration structure holding all settings for the AI CLI
#[derive(Deserialize)]
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
    /// Load configuration from ~/.aicli.conf file and environment variables
    pub fn load() -> Result<Self> {
        let home_dir = dirs::home_dir()
            .context("Could not determine home directory")?;

        let config_path = home_dir.join(".aicli.conf");

        // Load .env style config file if it exists
        if config_path.exists() {
            dotenv::from_path(&config_path)
                .context("Failed to load config file")?;
        }

        // Read environment variables with defaults
        let api_base_url = std::env::var("API_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_string());
        let api_version = std::env::var("API_VERSION").unwrap_or_else(|_| "v1".to_string());
        let model = std::env::var("MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let api_key = std::env::var("API_KEY").unwrap_or_else(|_| "".to_string());
        let smtp_server = std::env::var("SMTP_SERVER_IP").unwrap_or_else(|_| "localhost".to_string());
        let smtp_username = std::env::var("SMTP_USERNAME").unwrap_or_else(|_| "".to_string());
        let smtp_password = std::env::var("SMTP_PASSWORD").unwrap_or_else(|_| "".to_string());
        let destination_email = std::env::var("DESTINATION_EMAIL").unwrap_or_else(|_| "".to_string());
        let sender_email = std::env::var("SENDER_EMAIL").unwrap_or_else(|_| "".to_string());
        let google_search_api_key = std::env::var("GOOGLE_SEARCH_API_KEY").unwrap_or_else(|_| "".to_string());
        let google_search_engine_id = std::env::var("GOOGLE_SEARCH_ENGINE_ID").unwrap_or_else(|_| "".to_string());
        let alpha_vantage_api_key = std::env::var("ALPHA_VANTAGE_API_KEY").unwrap_or_else(|_| "".to_string());

        Ok(Config {
            api_base_url,
            api_version,
            model,
            api_key,
            smtp_server,
            smtp_username,
            smtp_password,
            destination_email,
            sender_email,
            google_search_api_key,
            google_search_engine_id,
            alpha_vantage_api_key,
        })
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