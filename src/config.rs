use anyhow::{Context, Result};
use config::{Config as ConfigLoader, Environment, File, FileFormat};
use serde::Deserialize;
use std::env;

/// Configuration structure holding all settings for the AI CLI
#[derive(Debug, Clone, Deserialize)]
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
    pub tavily_api_key: String,
    pub alpha_vantage_api_key: String,
}

impl Config {
    /// Load configuration from ~/.aicli.conf file and environment variables (prefixed with AICLI_)
    pub fn load() -> Result<Self> {
        let home_dir = dirs::home_dir().context("Could not determine home directory")?;

        let config_path = home_dir.join(".aicli.conf");

        let loader = ConfigLoader::builder()
            .add_source(
                File::from(config_path)
                    .format(FileFormat::Ini)
                    .required(false),
            )
            .add_source(Environment::with_prefix("AICLI"))
            .build()
            .context("Failed to build config")?;

        let mut config: Config = loader
            .try_deserialize()
            .context("Failed to deserialize config")?;

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
        format!(
            "{}/{}/chat/completions",
            self.api_base_url, self.api_version
        )
    }
}

const EMPTY_MSG: &str = "<not set>";

pub fn mask_value(value: &str, mask_empty: bool) -> String {
    if mask_empty {
        if value.is_empty() {
            EMPTY_MSG.to_string()
        } else {
            "***masked***".to_string()
        }
    } else {
        value.to_string()
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
            tavily_api_key: "".to_string(),
            alpha_vantage_api_key: "".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert_eq!(config.api_base_url, "https://api.openai.com");
        assert_eq!(config.api_version, "v1");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.api_key, "");
        assert_eq!(config.smtp_server, "localhost");
        assert!(config.smtp_username.is_empty());
        assert!(config.smtp_password.is_empty());
        assert!(config.destination_email.is_empty());
        assert!(config.sender_email.is_empty());
        assert!(config.tavily_api_key.is_empty());
        assert!(config.alpha_vantage_api_key.is_empty());
    }

    #[test]
    fn test_get_api_endpoint() {
        let config = Config::default();
        let endpoint = config.get_api_endpoint();
        assert!(endpoint.contains("api.openai.com"));
        assert!(endpoint.contains("v1"));
        assert!(endpoint.contains("chat/completions"));
    }

    #[test]
    fn test_mask_value_mask_empty_false() {
        assert_eq!(mask_value("hello", false), "hello");
        assert_eq!(mask_value("", false), "");
    }

    #[test]
    fn test_mask_value_mask_empty_true() {
        assert_eq!(mask_value("secret", true), "***masked***");
        assert_eq!(mask_value("", true), "<not set>");
    }
}
