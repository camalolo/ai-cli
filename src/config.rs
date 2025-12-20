use dirs;
use std::env;
use std::path::Path;

/// Configuration structure holding all settings for the AI CLI
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
    /// Load configuration from ~/.aicli.conf file
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let home_dir = dirs::home_dir()
            .expect("Could not determine home directory")
            .to_string_lossy()
            .to_string();
        
        let config_path = format!("{}/.aicli.conf", home_dir);
        
        // Check if config file exists
        if !Path::new(&config_path).exists() {
            return Err(format!(
                "Configuration file not found at {}\n\
                 Please create ~/.aicli.conf with your settings.\n\
                 See readme.md for configuration examples.",
                config_path
            ).into());
        }
        
        // Load environment variables from config file
        dotenv::from_path(&config_path)
            .map_err(|e| format!("Failed to load config file: {}", e))?;
        
        // Load values with defaults
        let config = Config {
            // AI Provider Configuration
            api_base_url: env::var("API_BASE_URL")
                .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string()),
            api_version: env::var("API_VERSION")
                .unwrap_or_else(|_| "v1beta".to_string()),
            model: env::var("MODEL")
                .unwrap_or_else(|_| "gemini-2.5-flash".to_string()),
            api_key: env::var("API_KEY")
                .unwrap_or_else(|_| "<NO KEY>".to_string()),
            
            // SMTP Configuration with defaults
            smtp_server: env::var("SMTP_SERVER_IP")
                .unwrap_or_else(|_| "localhost".to_string()),
            smtp_username: env::var("SMTP_USERNAME")
                .unwrap_or_else(|_| "".to_string()),
            smtp_password: env::var("SMTP_PASSWORD")
                .unwrap_or_else(|_| "".to_string()),
            destination_email: env::var("DESTINATION_EMAIL")
                .unwrap_or_else(|_| "".to_string()),
            sender_email: env::var("SENDER_EMAIL")
                .unwrap_or_else(|_| "".to_string()),
            
            // Optional: Search APIs (empty if not set)
            google_search_api_key: env::var("GOOGLE_SEARCH_API_KEY")
                .unwrap_or_else(|_| "".to_string()),
            google_search_engine_id: env::var("GOOGLE_SEARCH_ENGINE_ID")
                .unwrap_or_else(|_| "".to_string()),
            alpha_vantage_api_key: env::var("ALPHA_VANTAGE_API_KEY")
                .unwrap_or_else(|_| "".to_string()),
        };
        
        // Validate required fields
        if config.api_key.is_empty() {
            return Err("API_KEY is required in ~/.aicli.conf".into());
        }
        
        Ok(config)
    }
    
    /// Construct the API endpoint URL - always use OpenAI-compatible format
    pub fn get_api_endpoint(&self) -> String {
        // Always use OpenAI-compatible chat/completions endpoint
        // Gemini also supports OpenAI-compatible endpoints
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
    
    /// Get query parameter for authentication - no longer used
    pub fn get_auth_query(&self) -> Option<(&'static str, String)> {
        None
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