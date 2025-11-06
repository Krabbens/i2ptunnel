use crate::proxy_manager::Proxy;
use crate::proxy_selector::{ProxySelector, SelectedProxy};
use crate::i2pd_router::ensure_router_running;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use url::Url;

/// Format an error with full details including error chain and debug information
fn format_error_full(err: &dyn std::error::Error) -> String {
    let mut error_parts = Vec::new();
    
    // Main error message
    error_parts.push(format!("Error: {}", err));
    
    // Error source chain
    let mut source = err.source();
    if source.is_some() {
        error_parts.push("Source chain:".to_string());
        let mut depth = 0;
        while let Some(src) = source {
            depth += 1;
            error_parts.push(format!("  {}: {}", depth, src));
            source = src.source();
        }
    }
    
    // Debug representation
    error_parts.push(format!("Debug: {:#?}", err));
    
    error_parts.join("\n")
}

/// Log error with full details, splitting long messages to avoid truncation
fn log_error_full(prefix: &str, err: &dyn std::error::Error) {
    // Log the main error message first
    error!("{} Error: {}", prefix, err);
    
    // Log error source chain
    let mut source = err.source();
    let mut depth = 0;
    while let Some(src) = source {
        depth += 1;
        error!("{} Source {}: {}", prefix, depth, src);
        source = src.source();
    }
    
    // Log the debug representation (this gives full error details)
    error!("{} Error debug: {:#?}", prefix, err);
}

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

    /// Verify router SOCKS proxy is reachable by attempting to connect
    async fn verify_router_socks_available(port: u16) -> bool {
        use std::time::Duration;
        
        // Try to actually connect to the port
        match tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        ).await {
            Ok(Ok(_)) => {
                debug!("Router SOCKS proxy on port {} is reachable", port);
                true
            }
            Ok(Err(e)) => {
                debug!("Router SOCKS proxy on port {} not reachable: {}", port, e);
                false
            }
            Err(_) => {
                debug!("Router SOCKS proxy on port {} connection timeout", port);
                false
            }
        }
    }

    /// Create a client from a proxy candidate with optional router port hint
    async fn create_client_from_proxy(
        &self,
        selected_proxy: &SelectedProxy,
        router_port_hint: Option<u16>,
    ) -> Result<(Client, String), String> {
        let is_i2p_outproxy = selected_proxy.proxy.is_i2p_proxy();
        
        let client = if is_i2p_outproxy {
            // Ensure i2pd router is running for I2P outproxies
            if let Err(e) = ensure_router_running() {
                return Err(format!("Failed to ensure i2pd router is running: {}", e));
            }
            
            // For I2P-based outproxies, connect to them through the router's HTTP/HTTPS proxy
            // SOCKS5 cannot handle .b32.i2p addresses, so we skip SOCKS5 entirely
            debug!("Connecting to I2P outproxy {} through router (HTTP/HTTPS only, no SOCKS5)", selected_proxy.proxy.url);
            
            // If router port hint is provided (for parallel downloads), use it
            if let Some(port) = router_port_hint {
                // Try HTTP or HTTPS based on port hint
                if port == 4444 {
                    // HTTP proxy
                    match reqwest::Proxy::http("http://127.0.0.1:4444") {
                        Ok(i2p_proxy) => {
                            match Client::builder()
                                .proxy(i2p_proxy)
                                .timeout(std::time::Duration::from_secs(300))
                                .build()
                            {
                                Ok(client) => {
                                    info!("Using router HTTP proxy on port 4444 for I2P outproxy {} (parallel download)", selected_proxy.proxy.url);
                                    return Ok((client, format!("router-http://127.0.0.1:4444 (for {})", selected_proxy.proxy.url)));
                                }
                                Err(e) => return Err(format!("Failed to create HTTP client: {}", e)),
                            }
                        }
                        Err(e) => return Err(format!("Failed to create HTTP proxy: {}", e)),
                    }
                } else if port == 4447 {
                    // HTTPS proxy (not SOCKS5, as SOCKS5 cannot handle .b32.i2p addresses)
                    match reqwest::Proxy::https("http://127.0.0.1:4447") {
                        Ok(i2p_proxy) => {
                            match Client::builder()
                                .proxy(i2p_proxy)
                                .timeout(std::time::Duration::from_secs(300))
                                .build()
                            {
                                Ok(client) => {
                                    info!("Using router HTTPS proxy on port 4447 for I2P outproxy {} (parallel download)", selected_proxy.proxy.url);
                                    return Ok((client, format!("router-https://127.0.0.1:4447 (for {})", selected_proxy.proxy.url)));
                                }
                                Err(e) => return Err(format!("Failed to create HTTPS client: {}", e)),
                            }
                        }
                        Err(e) => return Err(format!("Failed to create HTTPS proxy: {}", e)),
                    }
                }
            }
            
            // No router port hint: try HTTP proxy first, then HTTPS proxy
            // HTTP proxy is better for streaming large files and can handle .b32.i2p addresses
            match reqwest::Proxy::http("http://127.0.0.1:4444") {
                Ok(i2p_proxy) => {
                    match Client::builder()
                        .proxy(i2p_proxy)
                        .timeout(std::time::Duration::from_secs(300))  // Longer timeout for streaming
                        .build()
                    {
                        Ok(client) => {
                            info!("Using router HTTP proxy on port 4444 for I2P outproxy {} (better for streaming)", selected_proxy.proxy.url);
                            Ok((client, format!("router-http://127.0.0.1:4444 (for {})", selected_proxy.proxy.url)))
                        }
                        Err(e) => {
                            log_error_full("Failed to create client with router HTTP, falling back to HTTPS:", &e);
                            // Fallback to HTTPS
                            reqwest::Proxy::https("http://127.0.0.1:4447")
                                .map_err(|e| {
                                    log_error_full("Failed to create I2P HTTPS proxy (tried HTTP port 4444):", &e);
                                    format!("Failed to create I2P HTTPS proxy: {} (tried HTTP port 4444)", e)
                                })
                                .and_then(|i2p_proxy| {
                                    Client::builder()
                                        .proxy(i2p_proxy)
                                        .timeout(std::time::Duration::from_secs(300))
                                        .build()
                                        .map_err(|e| {
                                            log_error_full("Failed to create HTTPS client:", &e);
                                            format!("Failed to create HTTPS client: {}", e)
                                        })
                                })
                                .map(|client| (client, format!("router-https://127.0.0.1:4447 (for {}, fallback from HTTP)", selected_proxy.proxy.url)))
                        }
                    }
                }
                Err(e) => {
                    log_error_full("Router HTTP proxy not available, falling back to HTTPS:", &e);
                    // Final fallback to HTTPS
                    reqwest::Proxy::https("http://127.0.0.1:4447")
                        .map_err(|e| {
                            log_error_full("Failed to create I2P HTTPS proxy (tried HTTP port 4444):", &e);
                            format!("Failed to create I2P HTTPS proxy: {} (tried HTTP port 4444)", e)
                        })
                        .and_then(|i2p_proxy| {
                            Client::builder()
                                .proxy(i2p_proxy)
                                .timeout(std::time::Duration::from_secs(300))
                                .build()
                                .map_err(|e| {
                                    log_error_full("Failed to create HTTPS client:", &e);
                                    format!("Failed to create HTTPS client: {}", e)
                                })
                        })
                        .map(|client| (client, format!("router-https://127.0.0.1:4447 (for {}, fallback from HTTP)", selected_proxy.proxy.url)))
                }
            }
        } else {
            // For non-I2P outproxies, use them directly based on type
            match &selected_proxy.proxy.proxy_type {
                crate::proxy_manager::ProxyType::Socks => {
                    // Try SOCKS first, fallback to HTTPS if SOCKS fails
                    let socks_url = format!("socks5://{}:{}", selected_proxy.proxy.host, selected_proxy.proxy.port);
                    let https_url = format!("https://{}:{}", selected_proxy.proxy.host, selected_proxy.proxy.port);
                    
                    // Try SOCKS first
                    match reqwest::Proxy::all(&socks_url) {
                        Ok(socks_proxy) => {
                            match Client::builder()
                                .proxy(socks_proxy)
                                .timeout(std::time::Duration::from_secs(60))
                                .build()
                            {
                                Ok(client) => Ok((client, selected_proxy.proxy.url.clone())),
                                Err(e) => {
                                    warn!("SOCKS proxy {} failed to create client, falling back to HTTPS: {}", selected_proxy.proxy.url, e);
                                    // Fallback to HTTPS
                                    reqwest::Proxy::https(&https_url)
                                        .map_err(|e| format!("Failed to create HTTPS fallback proxy for {}: {}", selected_proxy.proxy.url, e))
                                        .and_then(|p| {
                                            Client::builder()
                                                .proxy(p)
                                                .timeout(std::time::Duration::from_secs(60))
                                                .build()
                                                .map_err(|e| format!("Failed to create HTTPS fallback client for {}: {}", selected_proxy.proxy.url, e))
                                        })
                                        .map(|client| (client, format!("https://{}:{} (fallback from SOCKS)", selected_proxy.proxy.host, selected_proxy.proxy.port)))
                                }
                            }
                        }
                        Err(e) => {
                            warn!("SOCKS proxy {} not available, falling back to HTTPS: {}", selected_proxy.proxy.url, e);
                            // Fallback to HTTPS
                            reqwest::Proxy::https(&https_url)
                                .map_err(|e| format!("Failed to create HTTPS fallback proxy for {}: {}", selected_proxy.proxy.url, e))
                                .and_then(|p| {
                                    Client::builder()
                                        .proxy(p)
                                        .timeout(std::time::Duration::from_secs(60))
                                        .build()
                                        .map_err(|e| format!("Failed to create HTTPS fallback client for {}: {}", selected_proxy.proxy.url, e))
                                })
                                .map(|client| (client, format!("https://{}:{} (fallback from SOCKS)", selected_proxy.proxy.host, selected_proxy.proxy.port)))
                        }
                    }
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
            
            // Ensure i2pd router is running
            if let Err(e) = ensure_router_running() {
                return Err(format!("Failed to ensure i2pd router is running: {}", e));
            }
            
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
        info!("Clearnet site detected, trying {} proxy candidates", proxy_candidates.len());
        
        if proxy_candidates.is_empty() {
            error!("No proxy candidates available for clearnet request");
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
            let (client, proxy_used) = match self.create_client_from_proxy(selected_proxy, None).await {
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
                        log_error_full(&format!("Full error details for proxy {}:", proxy_used), &e);
                        // Mark this proxy as failed
                        self.proxy_selector.handle_proxy_failure(&selected_proxy.proxy).await;
                        failed_proxies.push(selected_proxy);
                        last_error = Some(format!("Proxy {}: {}", proxy_used, error_str));
                        // Continue to next proxy
                        continue;
                    } else {
                        // For non-connection errors (like HTTP errors), return immediately
                        // as retrying won't help
                        let prefix = format!("Request failed through proxy {} with non-connection error:", proxy_used);
                        log_error_full(&prefix, &e);
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

    /// Get proxy candidates for a request (public helper method)
    pub async fn get_proxy_candidates_for_request(
        &self,
        available_proxies: Vec<Proxy>,
        count: usize,
    ) -> Result<Vec<SelectedProxy>, Box<dyn std::error::Error>> {
        self.proxy_selector.ensure_multiple_proxy_candidates(available_proxies, count).await
    }

    /// Handle a request using a specific proxy (for parallel downloads)
    pub async fn handle_request_with_specific_proxy(
        &self,
        config: RequestConfig,
        proxy: Proxy,
        router_port_hint: Option<u16>,
    ) -> Result<ResponseData, String> {
        info!("Handling request with specific proxy: {} {} -> {}", config.method, config.url, proxy.url);

        // Create a SelectedProxy from the provided proxy
        let selected_proxy = SelectedProxy {
            proxy: proxy.clone(),
            speed_bytes_per_sec: 1024.0 * 50.0, // Default speed assumption
            selected_at: std::time::Instant::now(),
        };

        // Create client from this specific proxy with optional router port hint
        let (client, proxy_used) = match self.create_client_from_proxy(&selected_proxy, router_port_hint).await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to create client for specific proxy {}: {}", proxy.url, e);
                return Err(format!("Failed to create client: {}", e));
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

        debug!("Sending request through specific proxy: {}", proxy_used);

        // Send request
        let response = request.send().await.map_err(|e| {
            let prefix = format!("Request failed through proxy {}:", proxy_used);
            log_error_full(&prefix, &e);
            format!("Request failed through proxy {}: {}", proxy_used, e)
        })?;

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
                    log_error_full("Failed to read response body:", &e);
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
        let (response, proxy_used, _is_i2p) = self.create_client_and_send_request(&config, proxy_candidates).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_i2p_domain() {
        // Test .i2p domains
        assert!(RequestHandler::is_i2p_domain("http://example.i2p"));
        assert!(RequestHandler::is_i2p_domain("https://example.i2p/path"));
        assert!(RequestHandler::is_i2p_domain("http://site.i2p:8080"));
        
        // Test .b32.i2p domains
        assert!(RequestHandler::is_i2p_domain("http://abc123.b32.i2p"));
        assert!(RequestHandler::is_i2p_domain("https://xyz789.b32.i2p/path"));
        
        // Test non-I2P domains
        assert!(!RequestHandler::is_i2p_domain("http://example.com"));
        assert!(!RequestHandler::is_i2p_domain("https://google.com"));
        assert!(!RequestHandler::is_i2p_domain("http://localhost:8080"));
        
        // Test edge cases
        assert!(!RequestHandler::is_i2p_domain(""));
        assert!(!RequestHandler::is_i2p_domain("i2p"));
        assert!(!RequestHandler::is_i2p_domain("not-i2p.com"));
    }

    #[test]
    fn test_request_config_creation() {
        let config = RequestConfig {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: None,
            body: None,
            stream: false,
        };
        
        assert_eq!(config.url, "https://example.com");
        assert_eq!(config.method, "GET");
        assert!(config.headers.is_none());
        assert!(config.body.is_none());
        assert!(!config.stream);
    }

    #[test]
    fn test_request_config_with_stream() {
        let config = RequestConfig {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: None,
            body: None,
            stream: true,
        };
        
        assert!(config.stream);
    }

    #[test]
    fn test_request_config_with_headers() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("User-Agent".to_string(), "test".to_string());
        
        let config = RequestConfig {
            url: "https://example.com".to_string(),
            method: "GET".to_string(),
            headers: Some(headers),
            body: None,
            stream: false,
        };
        
        assert!(config.headers.is_some());
        let headers = config.headers.unwrap();
        assert_eq!(headers.get("User-Agent"), Some(&"test".to_string()));
    }

    #[test]
    fn test_response_data_creation() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html".to_string());
        
        let response = ResponseData {
            status: 200,
            headers,
            body: b"Hello World".to_vec(),
            proxy_used: "http://proxy.i2p:443".to_string(),
        };
        
        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("Content-Type"), Some(&"text/html".to_string()));
        assert_eq!(response.body, b"Hello World");
        assert_eq!(response.proxy_used, "http://proxy.i2p:443");
    }

    #[test]
    fn test_is_i2p_domain_edge_cases() {
        // Test various edge cases
        assert!(!RequestHandler::is_i2p_domain("http://.i2p")); // Empty host
        assert!(!RequestHandler::is_i2p_domain("http://i2p")); // Just i2p, not .i2p
        assert!(RequestHandler::is_i2p_domain("http://a.b32.i2p")); // Valid b32
        assert!(RequestHandler::is_i2p_domain("https://test.i2p:8080/path?query=1")); // With port and path
        assert!(!RequestHandler::is_i2p_domain("http://i2p.example.com")); // i2p as subdomain
    }

    #[test]
    fn test_is_proxy_connection_error() {
        assert!(RequestHandler::is_proxy_connection_error("Connection unreachable"));
        assert!(RequestHandler::is_proxy_connection_error("connection refused"));
        assert!(RequestHandler::is_proxy_connection_error("Connection timed out"));
        assert!(RequestHandler::is_proxy_connection_error("SOCKS connect error"));
        assert!(!RequestHandler::is_proxy_connection_error("HTTP 404 Not Found"));
        assert!(!RequestHandler::is_proxy_connection_error("Invalid response"));
    }

    #[test]
    fn test_request_config_all_methods() {
        let methods = vec!["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"];
        
        for method in methods {
            let config = RequestConfig {
                url: "https://example.com".to_string(),
                method: method.to_string(),
                headers: None,
                body: None,
                stream: false,
            };
            assert_eq!(config.method, method);
        }
    }

    #[test]
    fn test_request_config_with_body() {
        let body = b"test body data".to_vec();
        let config = RequestConfig {
            url: "https://example.com".to_string(),
            method: "POST".to_string(),
            headers: None,
            body: Some(body.clone()),
            stream: false,
        };
        
        assert!(config.body.is_some());
        assert_eq!(config.body.unwrap(), body);
    }

    #[test]
    fn test_response_data_empty_body() {
        let response = ResponseData {
            status: 204,
            headers: std::collections::HashMap::new(),
            body: vec![],
            proxy_used: "http://proxy.i2p:443".to_string(),
        };
        
        assert_eq!(response.status, 204);
        assert_eq!(response.body.len(), 0);
    }

    #[test]
    fn test_response_data_large_body() {
        let large_body = vec![0u8; 10000];
        let response = ResponseData {
            status: 200,
            headers: std::collections::HashMap::new(),
            body: large_body.clone(),
            proxy_used: "http://proxy.i2p:443".to_string(),
        };
        
        assert_eq!(response.body.len(), 10000);
    }
}


