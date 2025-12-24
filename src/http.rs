use reqwest::Client;
use std::sync::OnceLock;

static ASYNC_HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn create_async_http_client() -> Client {
    ASYNC_HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent("ai-cli/1.0")
            .build()
            .unwrap_or_else(|_| Client::new())
    }).clone()
}