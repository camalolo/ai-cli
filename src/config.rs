use std::env;
use std::path::{Path, PathBuf};

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
    fn get_env_or_default(key: &str, default: &str) -> String {
        env::var(key).unwrap_or_else(|_| default.to_string())
    }

    /// Load configuration from ~/.aicli.conf file
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let home_dir = ::dirs::home_dir()
            .expect("Could not determine home directory")
            .to_string_lossy()
            .to_string();

        let config_path = PathBuf::from(&home_dir).join(".aicli.conf").to_string_lossy().to_string();

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
            api_base_url: Self::get_env_or_default("API_BASE_URL", "https://generativelanguage.googleapis.com"),
            api_version: Self::get_env_or_default("API_VERSION", "v1beta"),
            model: Self::get_env_or_default("MODEL", "gemini-2.5-flash"),
            api_key: Self::get_env_or_default("API_KEY", ""),

            // SMTP Configuration with defaults
            smtp_server: Self::get_env_or_default("SMTP_SERVER_IP", "localhost"),
            smtp_username: Self::get_env_or_default("SMTP_USERNAME", ""),
            smtp_password: Self::get_env_or_default("SMTP_PASSWORD", ""),
            destination_email: Self::get_env_or_default("DESTINATION_EMAIL", ""),
            sender_email: Self::get_env_or_default("SENDER_EMAIL", ""),

            // Optional: Search APIs (empty if not set)
            google_search_api_key: Self::get_env_or_default("GOOGLE_SEARCH_API_KEY", ""),
            google_search_engine_id: Self::get_env_or_default("GOOGLE_SEARCH_ENGINE_ID", ""),
            alpha_vantage_api_key: Self::get_env_or_default("ALPHA_VANTAGE_API_KEY", ""),
        };
        
        // API key validation moved to runtime on 401 error
        
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