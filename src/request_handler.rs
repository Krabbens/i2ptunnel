use crate::proxy_manager::Proxy;
use crate::proxy_selector::{ProxySelector, SelectedProxy};
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
    pub fn is_i2p_domain(url: &str) -> bool {
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

    /// Check if an error is a proxy connection error (unreachable, timeout, etc.)
    fn is_proxy_connection_error(error: &str) -> bool {
        let error_lower = error.to_lowercase();
        error_lower.contains("unreachable") 
            || error_lower.contains("connection refused")
            || error_lower.contains("connection reset")
            || error_lower.contains("connection timed out")
            || error_lower.contains("timeout")
            || error_lower.contains("socks connect error")
            || error_lower.contains("proxy server unreachable")
    }

    /// Verify router SOCKS proxy is reachable by attempting to create a client
    async fn verify_router_socks_available(port: u16) -> bool {
        let router_socks = format!("socks5://127.0.0.1:{}", port);
        match reqwest::Proxy::all(&router_socks) {
            Ok(_) => {
                debug!("Router SOCKS proxy on port {} appears to be available", port);
                true
            }
            Err(e) => {
                debug!("Router SOCKS proxy on port {} not available: {}", port, e);
                false
            }
        }
    }

    /// Create a client from a proxy candidate
    async fn create_client_from_proxy(
        &self,
        selected_proxy: &SelectedProxy,
    ) -> Result<(Client, String), String> {
        let is_i2p_outproxy = selected_proxy.proxy.is_i2p_proxy();
        
        let client = if is_i2p_outproxy {
            // For I2P-based outproxies, connect to them through the router's SOCKS proxy
            debug!("Connecting to I2P outproxy {} through router", selected_proxy.proxy.url);
            
            // Verify router SOCKS proxies are available before trying
            let router_socks_ports = [4446, 9060];
            let mut available_port: Option<u16> = None;
            
            for &port in &router_socks_ports {
                if Self::verify_router_socks_available(port).await {
                    available_port = Some(port);
                    break;
                }
            }
            
            if let Some(port) = available_port {
                let router_socks = format!("socks5://127.0.0.1:{}", port);
                match reqwest::Proxy::all(&router_socks) {
                    Ok(router_proxy) => {
                        match Client::builder()
                            .proxy(router_proxy)
                            .timeout(std::time::Duration::from_secs(60))
                            .build()
                        {
                            Ok(client) => {
                                info!("Using router SOCKS proxy on port {} for I2P outproxy {} (router must route clearnet through outproxies)", 
                                      port, selected_proxy.proxy.url);
                                Ok((client, format!("router-socks5://127.0.0.1:{} (for {})", port, selected_proxy.proxy.url)))
                            }
                            Err(e) => {
                                Err(format!("Failed to create client with router SOCKS on port {}: {}", port, e))
                            }
                        }
                    }
                    Err(e) => {
                        Err(format!("Router SOCKS proxy not available on port {}: {}", port, e))
                    }
                }
            } else {
                // Fallback: Use router HTTP proxy (requires router outproxy configuration)
                warn!("No router SOCKS proxy available (tried ports {:?}), falling back to HTTP proxy (router must be configured for outproxies)", router_socks_ports);
                reqwest::Proxy::http("http://127.0.0.1:4444")
                    .map_err(|e| format!("Failed to create I2P HTTP proxy: {} (tried SOCKS ports {:?})", e, router_socks_ports))
                    .and_then(|i2p_proxy| {
                        Client::builder()
                            .proxy(i2p_proxy)
                            .timeout(std::time::Duration::from_secs(60))
                            .build()
                            .map_err(|e| format!("Failed to create client: {}", e))
                    })
                    .map(|client| (client, format!("router-http://127.0.0.1:4444 (for {})", selected_proxy.proxy.url)))
            }
        } else {
            // For non-I2P outproxies, use them directly based on type
            match &selected_proxy.proxy.proxy_type {
                crate::proxy_manager::ProxyType::Socks => {
                    let socks_url = format!("socks5://{}:{}", selected_proxy.proxy.host, selected_proxy.proxy.port);
                    reqwest::Proxy::all(&socks_url)
                        .map_err(|e| format!("Failed to create SOCKS proxy for {}: {}", selected_proxy.proxy.url, e))
                        .and_then(|p| {
                            Client::builder()
                                .proxy(p)
                                .timeout(std::time::Duration::from_secs(60))
                                .build()
                                .map_err(|e| format!("Failed to create client for {}: {}", selected_proxy.proxy.url, e))
                        })
                        .map(|client| (client, selected_proxy.proxy.url.clone()))
                }
                crate::proxy_manager::ProxyType::Https => {
                    reqwest::Proxy::https(&selected_proxy.proxy.url)
                        .map_err(|e| format!("Failed to create HTTPS proxy for {}: {}", selected_proxy.proxy.url, e))
                        .and_then(|p| {
                            Client::builder()
                                .proxy(p)
                                .timeout(std::time::Duration::from_secs(60))
                                .build()
                                .map_err(|e| format!("Failed to create client for {}: {}", selected_proxy.proxy.url, e))
                        })
                        .map(|client| (client, selected_proxy.proxy.url.clone()))
                }
                crate::proxy_manager::ProxyType::Http => {
                    reqwest::Proxy::http(&selected_proxy.proxy.url)
                        .map_err(|e| format!("Failed to create HTTP proxy for {}: {}", selected_proxy.proxy.url, e))
                        .and_then(|p| {
                            Client::builder()
                                .proxy(p)
                                .timeout(std::time::Duration::from_secs(60))
                                .build()
                                .map_err(|e| format!("Failed to create client for {}: {}", selected_proxy.proxy.url, e))
                        })
                        .map(|client| (client, selected_proxy.proxy.url.clone()))
                }
            }
        };

        client
    }

    // Helper method to create client and send request (extracted for reuse)
    pub async fn create_client_and_send_request(
        &self,
        config: &RequestConfig,
        proxy_candidates: Vec<SelectedProxy>,
    ) -> Result<(reqwest::Response, String, bool), String> {
        // Check if this is an I2P domain
        let is_i2p = Self::is_i2p_domain(&config.url);
        
        // For I2P sites, use local I2P proxy (no retry needed)
        if is_i2p {
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

            debug!("Sending request through I2P proxy: {}", proxy_url);

            // Send request
            let response = request.send().await
                .map_err(|e| format!("Request failed through I2P proxy {}: {}", proxy_url, e))?;

            return Ok((response, proxy_url.to_string(), true));
        }

        // For clearnet sites, try multiple proxy candidates with retry logic
        debug!("Clearnet site detected, trying {} proxy candidates", proxy_candidates.len());
        
        if proxy_candidates.is_empty() {
            return Err("No proxy candidates available for clearnet request".to_string());
        }

        let mut last_error: Option<String> = None;
        let mut failed_proxies: Vec<&SelectedProxy> = Vec::new();

        // Try each proxy candidate in order (fastest first)
        for (idx, selected_proxy) in proxy_candidates.iter().enumerate() {
            info!("Trying proxy {} of {}: {} ({:.2} KB/s)", 
                  idx + 1, proxy_candidates.len(), 
                  selected_proxy.proxy.url,
                  selected_proxy.speed_bytes_per_sec / 1024.0);

            // Create client from this proxy
            let (client, proxy_used) = match self.create_client_from_proxy(selected_proxy).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("Failed to create client for proxy {}: {}", selected_proxy.proxy.url, e);
                    last_error = Some(format!("Proxy {}: {}", selected_proxy.proxy.url, e));
                    failed_proxies.push(selected_proxy);
                    continue;
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

            // Try to send request
            match request.send().await {
                Ok(response) => {
                    info!("Request succeeded through proxy: {}", proxy_used);
                    // Mark any previously failed proxies
                    for failed_proxy in failed_proxies {
                        self.proxy_selector.handle_proxy_failure(&failed_proxy.proxy).await;
                    }
                    return Ok((response, proxy_used, false));
                }
                Err(e) => {
                    let error_str = format!("{}", e);
                    let is_connection_error = Self::is_proxy_connection_error(&error_str);
                    
                    if is_connection_error {
                        warn!("Proxy {} unreachable or connection error: {}", proxy_used, error_str);
                        // Mark this proxy as failed
                        self.proxy_selector.handle_proxy_failure(&selected_proxy.proxy).await;
                        failed_proxies.push(selected_proxy);
                        last_error = Some(format!("Proxy {}: {}", proxy_used, error_str));
                        // Continue to next proxy
                        continue;
                    } else {
                        // For non-connection errors (like HTTP errors), return immediately
                        // as retrying won't help
                        warn!("Request failed through proxy {} with non-connection error: {}", proxy_used, error_str);
                        return Err(format!("Request failed through proxy {}: {}", proxy_used, error_str));
                    }
                }
            }
        }

        // All proxies failed
        let error_msg = if let Some(err) = last_error {
            format!("All {} proxy candidates failed. Last error: {}", proxy_candidates.len(), err)
        } else {
            format!("All {} proxy candidates failed with unknown errors", proxy_candidates.len())
        };
        
        error!("{}", error_msg);
        Err(error_msg)
    }

    pub async fn handle_request(
        &self,
        config: RequestConfig,
        available_proxies: Vec<Proxy>,
    ) -> Result<ResponseData, String> {
        info!("Handling request: {} {} (stream={})", config.method, config.url, config.stream);

        // Check if this is an I2P domain
        let is_i2p = Self::is_i2p_domain(&config.url);
        
        // Get proxy candidates (for clearnet sites, get multiple candidates for retry)
        let proxy_candidates = if is_i2p {
            // For I2P sites, we don't need proxy candidates
            Vec::new()
        } else {
            // Get top 5 proxy candidates for clearnet sites
            match self.proxy_selector
                .ensure_multiple_proxy_candidates(available_proxies, 5)
                .await
            {
                Ok(candidates) => {
                    if candidates.is_empty() {
                        return Err("No available proxy candidates found".to_string());
                    }
                    info!("Got {} proxy candidates for request", candidates.len());
                    candidates
                }
                Err(e) => {
                    error!("Failed to get proxy candidates: {}", e);
                    return Err(format!("Proxy selection failed: {}", e));
                }
            }
        };
        
        // Use helper to create client and send request
        let (response, proxy_used, is_i2p_result) = self.create_client_and_send_request(&config, proxy_candidates).await?;

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


