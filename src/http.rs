use reqwest::blocking::Client;

pub fn create_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .user_agent("ai-cli/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}