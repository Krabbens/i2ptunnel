use crate::proxy_manager::Proxy;
use crate::proxy_selector::ProxySelector;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestConfig {
    pub url: String,
    pub method: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseData {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Vec<u8>,
    pub proxy_used: String,
}

pub struct RequestHandler {
    proxy_selector: Arc<ProxySelector>,
}

impl RequestHandler {
    pub fn new(proxy_selector: Arc<ProxySelector>) -> Self {
        info!("Initializing RequestHandler");
        Self { proxy_selector }
    }

    pub async fn handle_request(
        &self,
        config: RequestConfig,
        available_proxies: Vec<Proxy>,
    ) -> Result<ResponseData, String> {
        info!("Handling request: {} {}", config.method, config.url);

        // Ensure we have a fastest proxy
        let selected_proxy = match self
            .proxy_selector
            .ensure_fastest_proxy(available_proxies)
            .await
        {
            Ok(Some(proxy)) => {
                debug!("Using proxy: {}", proxy.proxy.url);
                proxy
            }
            Ok(None) => {
                return Err("No available proxy found".to_string());
            }
            Err(e) => {
                error!("Failed to ensure fastest proxy: {}", e);
                return Err(format!("Proxy selection failed: {}", e));
            }
        };

        // Create client with proxy
        let client = match Client::builder()
            .proxy(
                reqwest::Proxy::http(&selected_proxy.proxy.url)
                    .map_err(|e| format!("Failed to create proxy: {}", e))?
            )
            .timeout(std::time::Duration::from_secs(60))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to create HTTP client: {}", e);
                return Err(format!("Client creation failed: {}", e));
            }
        };

        // Build request
        let mut request = match config.method.as_str() {
            "GET" => client.get(&config.url),
            "POST" => client.post(&config.url),
            "PUT" => client.put(&config.url),
            "DELETE" => client.delete(&config.url),
            "PATCH" => client.patch(&config.url),
            "HEAD" => client.head(&config.url),
            _ => {
                return Err(format!("Unsupported HTTP method: {}", config.method));
            }
        };

        // Add headers
        if let Some(headers) = config.headers {
            for (key, value) in headers {
                request = request.header(&key, &value);
            }
        }

        // Add body
        if let Some(body) = config.body {
            request = request.body(body);
        }

        debug!("Sending request through proxy: {}", selected_proxy.proxy.url);

        // Send request
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Request failed through proxy {}: {}", selected_proxy.proxy.url, e);
                // Mark proxy as failed
                self.proxy_selector
                    .handle_proxy_failure(&selected_proxy.proxy)
                    .await;
                return Err(format!("Request failed: {}", e));
            }
        };

        let status = response.status().as_u16();
        info!("Received response: status {}", status);

        // Extract headers
        let mut response_headers = std::collections::HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                response_headers.insert(key.to_string(), value_str.to_string());
            }
        }

        // Read body
        let body = match response.bytes().await {
            Ok(b) => b.to_vec(),
            Err(e) => {
                error!("Failed to read response body: {}", e);
                return Err(format!("Failed to read body: {}", e));
            }
        };

        debug!(
            "Request completed: status {}, body size: {} bytes",
            status,
            body.len()
        );

        Ok(ResponseData {
            status,
            headers: response_headers,
            body,
            proxy_used: selected_proxy.proxy.url.clone(),
        })
    }
}


