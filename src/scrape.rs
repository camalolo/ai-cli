use anyhow::{anyhow, Result};
use reqwest::StatusCode;
use readability::extractor;

const MAX_RESPONSE_SIZE: usize = 5 * 1024 * 1024;

fn validate_url_scheme(url: &str) -> Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| anyhow!("Invalid URL '{}': {}", url, e))?;

    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(anyhow!(
            "URL scheme '{}' is not allowed. Only http and https are permitted.",
            scheme
        )),
    }
}

fn check_ssrf_risk(url: &reqwest::Url) -> Result<()> {
    use std::net::ToSocketAddrs;

    let host = url.host_str().ok_or_else(|| anyhow!("URL has no host"))?;

    if host == "localhost" || host == "localhost.localdomain" {
        return Err(anyhow!("Access to localhost is not allowed"));
    }

    let addrs: Vec<_> = format!("{}:0", host)
        .to_socket_addrs()
        .map_err(|e| anyhow!("Failed to resolve hostname '{}': {}", host, e))?
        .collect();

    for addr in addrs {
        match addr.ip() {
            std::net::IpAddr::V4(ipv4) => {
                if ipv4.is_loopback() {
                    return Err(anyhow!("URL resolves to loopback address {}, which is not allowed", ipv4));
                }
                if ipv4.is_private() {
                    return Err(anyhow!("URL resolves to private IP address {}, which is not allowed", ipv4));
                }
                if ipv4.is_link_local() {
                    return Err(anyhow!("URL resolves to link-local address {}, which is not allowed", ipv4));
                }
            }
            std::net::IpAddr::V6(ipv6) => {
                if ipv6.is_loopback() {
                    return Err(anyhow!("URL resolves to loopback address {}, which is not allowed", ipv6));
                }
                if (ipv6.segments()[0] & 0xfe00) == 0xfc00 {
                    return Err(anyhow!("URL resolves to private IPv6 address {}, which is not allowed", ipv6));
                }
                if (ipv6.segments()[0] & 0xffc0) == 0xfe80 {
                    return Err(anyhow!("URL resolves to link-local IPv6 address {}, which is not allowed", ipv6));
                }
            }
        }
    }

    Ok(())
}

async fn read_response_limited(mut resp: reqwest::Response) -> Result<String> {
    let mut buf = Vec::with_capacity(64 * 1024);
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| anyhow!("Error reading response: {}", e))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() > MAX_RESPONSE_SIZE {
            return Err(anyhow!(
                "Response body too large (exceeded {} bytes)",
                MAX_RESPONSE_SIZE
            ));
        }
    }
    String::from_utf8(buf).map_err(|e| anyhow!("Response body contained invalid UTF-8: {}", e))
}

pub async fn scrape_url(url: &str, mode: &str, debug: bool) -> Result<String> {
    crate::utils::log_to_file(debug, &format!("Scraping URL: {}", url));

    let parsed_url = validate_url_scheme(url)?;
    check_ssrf_risk(&parsed_url)?;

    let client = crate::http::create_async_http_client();

    let result = match client.get(url).send().await {
        Ok(resp) => {
            match resp.status() {
                StatusCode::OK => {
                    match read_response_limited(resp).await {
                        Ok(text) => {
                            let product = extractor::extract(&mut text.as_bytes(), &parsed_url)?;
                            if product.text.is_empty() {
                                "No readable content found on this page.".to_string()
                            } else {
                                product.text
                            }
                        }
                        Err(e) => return Err(anyhow!("Error reading content: {}", e)),
                    }
                }
                StatusCode::NOT_FOUND => "Skipped: 404 Not Found".to_string(),
                StatusCode::FORBIDDEN => "Skipped: 403 Forbidden".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR => {
                    "Skipped: 500 Internal Server Error".to_string()
                }
                status => format!("Skipped: HTTP status {}", status),
            }
        }
        Err(e) => {
            if e.is_timeout() {
                "Skipped: Request timed out".to_string()
            } else if e.is_connect() {
                "Skipped: Connection error".to_string()
            } else {
                return Err(anyhow!("Error fetching {}: {}", url, e));
            }
        }
    };

    let final_result = if mode == "full" || result.len() <= 1024 {
        result
    } else {
        crate::utils::log_to_file(debug, &format!("Summarizing content from {} chars", result.len()));
        let summary = crate::utils::summarize_text(&result, 3);
        if summary.is_empty() {
            result
        } else {
            summary
        }
    };

    crate::utils::log_to_file(debug, &format!("Final scrape result: {}", final_result));

    Ok(final_result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_scheme_http() {
        assert!(validate_url_scheme("http://example.com").is_ok());
    }

    #[test]
    fn test_validate_url_scheme_https() {
        assert!(validate_url_scheme("https://example.com").is_ok());
    }

    #[test]
    fn test_validate_url_scheme_file() {
        assert!(validate_url_scheme("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_validate_url_scheme_ftp() {
        assert!(validate_url_scheme("ftp://example.com").is_err());
    }

    #[test]
    fn test_validate_url_scheme_invalid() {
        assert!(validate_url_scheme("not-a-url").is_err());
    }

    #[test]
    fn test_max_response_size_constant() {
        assert_eq!(MAX_RESPONSE_SIZE, 5 * 1024 * 1024);
    }
}
