use crate::proxy_manager::Proxy;
use crate::proxy_selector::ProxySelector;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use url::Url;

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

    /// Check if a URL points to an I2P domain (.i2p or .b32.i2p)
    fn is_i2p_domain(url: &str) -> bool {
        match Url::parse(url) {
            Ok(parsed_url) => {
                if let Some(host) = parsed_url.host_str() {
                    host.ends_with(".i2p") || host.ends_with(".b32.i2p")
                } else {
                    false
                }
            }
            Err(_) => {
                // Fallback: simple string check if URL parsing fails
                url.contains(".i2p") || url.contains(".b32.i2p")
            }
        }
    }

    pub async fn handle_request(
        &self,
        config: RequestConfig,
        available_proxies: Vec<Proxy>,
    ) -> Result<ResponseData, String> {
        info!("Handling request: {} {}", config.method, config.url);

        // Check if this is an I2P domain
        let is_i2p = Self::is_i2p_domain(&config.url);
        
        // Clone available_proxies for error handling (needed after move for clearnet sites)
        // Only clone if not I2P to avoid unnecessary cloning for I2P requests
        let available_proxies_clone = if !is_i2p {
            Some(available_proxies.clone())
        } else {
            None
        };
        
        // Determine which proxy to use and create client
        let (client, proxy_used) = if is_i2p {
            // For I2P sites, use local I2P proxy
            info!("Detected I2P domain, using local I2P proxy");
            
            // Check if URL uses HTTPS to determine proxy port
            let is_https = config.url.starts_with("https://");
            let proxy_url = if is_https {
                "http://127.0.0.1:4447"  // HTTPS proxy port
            } else {
                "http://127.0.0.1:4444"  // HTTP proxy port
            };
            
            debug!("Using local I2P proxy: {}", proxy_url);
            
            let http_proxy = reqwest::Proxy::http(proxy_url)
                .map_err(|e| format!("Failed to create I2P HTTP proxy: {}", e))?;
            
            let mut builder = Client::builder()
                .proxy(http_proxy)
                .timeout(std::time::Duration::from_secs(60));
            
            // Add HTTPS proxy if needed
            if is_https {
                let https_proxy = reqwest::Proxy::https("http://127.0.0.1:4447")
                    .map_err(|e| format!("Failed to create I2P HTTPS proxy: {}", e))?;
                builder = builder.proxy(https_proxy);
            }
            
            let client = builder.build()
                .map_err(|e| format!("Failed to create I2P client: {}", e))?;
            
            (client, proxy_url.to_string())
        } else {
            // For clearnet sites, use selected outproxy
            debug!("Clearnet site detected, using outproxy");
            
            // Ensure we have a fastest proxy
            let selected_proxy = match self
                .proxy_selector
                .ensure_fastest_proxy(available_proxies)
                .await
            {
                Ok(Some(proxy)) => {
                    debug!("Using outproxy: {}", proxy.proxy.url);
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

            // Create client with outproxy
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
            
            (client, selected_proxy.proxy.url.clone())
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

        debug!("Sending request through proxy: {}", proxy_used);

        // Send request
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Request failed through proxy {}: {}", proxy_used, e);
                // Only mark proxy as failed if it's an outproxy (not local I2P proxy)
                if !is_i2p {
                    // Try to find the proxy in available_proxies_clone to mark it as failed
                    if let Some(ref proxies) = available_proxies_clone {
                        if let Some(failed_proxy) = proxies.iter().find(|p| p.url == proxy_used) {
                            self.proxy_selector
                                .handle_proxy_failure(failed_proxy)
                                .await;
                        }
                    }
                }
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
            proxy_used,
        })
    }
}


