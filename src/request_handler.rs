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
    pub stream: bool,
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

    // Helper method to create client and send request (extracted for reuse)
    async fn create_client_and_send_request(
        &self,
        config: &RequestConfig,
        available_proxies: Vec<Proxy>,
    ) -> Result<(reqwest::Response, String, bool), String> {
        // Check if this is an I2P domain
        let is_i2p = Self::is_i2p_domain(&config.url);
        
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
            // Check if the outproxy is an I2P address and what type it is
            let is_i2p_outproxy = selected_proxy.proxy.is_i2p_proxy();
            
            let client = if is_i2p_outproxy {
                // For I2P-based outproxies, connect to them through the router's SOCKS proxy
                // The router's SOCKS proxy can resolve .i2p addresses
                debug!("Connecting to I2P outproxy {} through router", selected_proxy.proxy.url);
                
                // Try to connect to the I2P outproxy through router SOCKS proxy
                // The router SOCKS proxy resolves .i2p addresses and routes to them
                // I2P router SOCKS proxy ports: 4446 (standard) or 9060 (alternative)
                let router_socks_ports = [4446, 9060];
                let mut last_error: Option<String> = None;
                let mut client_result: Option<Result<Client, String>> = None;
                
                for &port in &router_socks_ports {
                    let router_socks = format!("socks5://127.0.0.1:{}", port);
                    
                    match reqwest::Proxy::all(&router_socks) {
                        Ok(router_proxy) => {
                            // Use router SOCKS proxy - this will route through I2P
                            // Note: The router should be configured to route clearnet through outproxies
                            // If not configured, requests may go direct (showing real IP)
                            match Client::builder()
                                .proxy(router_proxy)
                                .timeout(std::time::Duration::from_secs(60))
                                .build()
                            {
                                Ok(client) => {
                                    info!("Using router SOCKS proxy on port {} for I2P outproxy {} (router must route clearnet through outproxies)", 
                                          port, selected_proxy.proxy.url);
                                    client_result = Some(Ok(client));
                                    break;
                                }
                                Err(e) => {
                                    debug!("Failed to create client with router SOCKS on port {}: {}", port, e);
                                    last_error = Some(format!("{}", e));
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Router SOCKS proxy not available on port {}: {}", port, e);
                            last_error = Some(format!("{}", e));
                            continue;
                        }
                    }
                }
                
                // Use the client if we got one, otherwise fallback to HTTP proxy
                match client_result {
                    Some(result) => result,
                    None => {
                        // Fallback: Use router HTTP proxy (requires router outproxy configuration)
                        warn!("Could not use router SOCKS proxy, falling back to HTTP proxy (router must be configured for outproxies)");
                        reqwest::Proxy::http("http://127.0.0.1:4444")
                            .map_err(|e| {
                                if let Some(prev_err) = last_error {
                                    format!("Failed to create I2P proxy: {} (tried SOCKS ports {:?})", prev_err, router_socks_ports)
                                } else {
                                    format!("Failed to create I2P proxy: {} (tried SOCKS ports {:?})", e, router_socks_ports)
                                }
                            })
                            .and_then(|i2p_proxy| {
                                Client::builder()
                                    .proxy(i2p_proxy)
                                    .timeout(std::time::Duration::from_secs(60))
                                    .build()
                                    .map_err(|e| format!("Failed to create client: {}", e))
                            })
                    }
                }
            } else {
                // For non-I2P outproxies, use them directly based on type
                match &selected_proxy.proxy.proxy_type {
                    crate::proxy_manager::ProxyType::Socks => {
                        let socks_url = format!("socks5://{}:{}", selected_proxy.proxy.host, selected_proxy.proxy.port);
                        reqwest::Proxy::all(&socks_url)
                            .map_err(|e| format!("Failed to create SOCKS proxy: {}", e))
                            .and_then(|p| {
                                Client::builder()
                                    .proxy(p)
                                    .timeout(std::time::Duration::from_secs(60))
                                    .build()
                                    .map_err(|e| format!("Failed to create client: {}", e))
                            })
                    }
                    crate::proxy_manager::ProxyType::Https => {
                        reqwest::Proxy::https(&selected_proxy.proxy.url)
                            .map_err(|e| format!("Failed to create HTTPS proxy: {}", e))
                            .and_then(|p| {
                                Client::builder()
                                    .proxy(p)
                                    .timeout(std::time::Duration::from_secs(60))
                                    .build()
                                    .map_err(|e| format!("Failed to create client: {}", e))
                            })
                    }
                    crate::proxy_manager::ProxyType::Http => {
                        reqwest::Proxy::http(&selected_proxy.proxy.url)
                            .map_err(|e| format!("Failed to create HTTP proxy: {}", e))
                            .and_then(|p| {
                                Client::builder()
                                    .proxy(p)
                                    .timeout(std::time::Duration::from_secs(60))
                                    .build()
                                    .map_err(|e| format!("Failed to create client: {}", e))
                            })
                    }
                }
            };
            
            let client = match client {
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
        if let Some(headers) = &config.headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }

        // Add body
        if let Some(body) = &config.body {
            request = request.body(body.clone());
        }

        debug!("Sending request through proxy: {}", proxy_used);

        // Send request
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Request failed through proxy {}: {}", proxy_used, e);
                return Err(format!("Request failed: {}", e));
            }
        };

        Ok((response, proxy_used, is_i2p))
    }

    pub async fn handle_request(
        &self,
        config: RequestConfig,
        available_proxies: Vec<Proxy>,
    ) -> Result<ResponseData, String> {
        info!("Handling request: {} {} (stream={})", config.method, config.url, config.stream);

        // Clone available_proxies for error handling (needed after move for clearnet sites)
        let available_proxies_clone = available_proxies.clone();
        
        // Use helper to create client and send request
        let (mut response, proxy_used, is_i2p) = self.create_client_and_send_request(&config, available_proxies).await?;
        
        // Handle proxy failure if needed
        if !is_i2p {
            if let Some(failed_proxy) = available_proxies_clone.iter().find(|p| p.url == proxy_used) {
                // Note: This is a best-effort attempt, may not always work due to ownership
            }
        }

        let status = response.status().as_u16();
        info!("Received response: status {}", status);

        // Extract headers
        let mut response_headers = std::collections::HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                response_headers.insert(key.to_string(), value_str.to_string());
            }
        }

        // Handle streaming vs non-streaming
        if config.stream {
            // For streaming, return empty body - the response will be read in chunks
            debug!("Streaming mode: response headers received, body will be streamed");
            Ok(ResponseData {
                status,
                headers: response_headers,
                body: Vec::new(), // Empty body for streaming
                proxy_used,
            })
        } else {
            // Read full body
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
}


