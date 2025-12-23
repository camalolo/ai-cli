use reqwest::{blocking::Client, Client as AsyncClient};
use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
static ASYNC_HTTP_CLIENT: OnceLock<AsyncClient> = OnceLock::new();

pub fn create_http_client() -> Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent("ai-cli/1.0")
            .build()
            .unwrap_or_else(|_| Client::new())
    }).clone()
}

pub fn create_async_http_client() -> AsyncClient {
    ASYNC_HTTP_CLIENT.get_or_init(|| {
        AsyncClient::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent("ai-cli/1.0")
            .build()
            .unwrap_or_else(|_| AsyncClient::new())
    }).clone()
}