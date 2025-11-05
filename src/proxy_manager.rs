use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use tracing::{debug, error, info, warn};
use url::Url;
use regex;

#[derive(Debug, Clone)]
pub enum ProxyType {
    Http,
    Https,
    Socks,
}

#[derive(Debug, Clone)]
pub struct Proxy {
    pub host: String,
    pub port: u16,
    pub url: String,
    pub proxy_type: ProxyType,
}

impl Proxy {
    pub fn new(host: String, port: u16) -> Self {
        let url = format!("http://{}:{}", host, port);
        // Default to HTTPS for I2P proxies (most common)
        let proxy_type = if port == 1080 || port == 9050 {
            ProxyType::Socks
        } else if port == 443 {
            ProxyType::Https
        } else {
            ProxyType::Http
        };
        Self { host, port, url, proxy_type }
    }
    
    pub fn new_with_type(host: String, port: u16, proxy_type: ProxyType) -> Self {
        let url = match proxy_type {
            ProxyType::Socks => format!("socks5://{}:{}", host, port),
            ProxyType::Https => format!("https://{}:{}", host, port),
            ProxyType::Http => format!("http://{}:{}", host, port),
        };
        Self { host, port, url, proxy_type }
    }

    pub fn from_url(url_str: &str) -> Option<Self> {
        match Url::parse(url_str) {
            Ok(url) => {
                let host = url.host_str()?.to_string();
                let port = url.port().unwrap_or(80);
                let proxy_type = if url_str.starts_with("socks5://") || port == 1080 || port == 9050 {
                    ProxyType::Socks
                } else if url_str.starts_with("https://") || port == 443 {
                    ProxyType::Https
                } else {
                    ProxyType::Http
                };
                Some(Self::new_with_type(host, port, proxy_type))
            }
            Err(e) => {
                warn!("Failed to parse proxy URL {}: {}", url_str, e);
                None
            }
        }
    }
    
    pub fn is_i2p_proxy(&self) -> bool {
        self.host.ends_with(".i2p") || self.host.ends_with(".b32.i2p")
    }
}

pub struct ProxyManager {
    client: Client,
}

impl ProxyManager {
    pub fn new() -> Self {
        info!("Initializing ProxyManager");
        
        // Use I2P HTTP proxy to access .i2p domains
        // Default I2P HTTP proxy ports: 4444 (HTTP) or 4447 (HTTPS)
        let i2p_proxy_http = reqwest::Proxy::http("http://127.0.0.1:4444")
            .unwrap_or_else(|_| {
                warn!("Failed to set I2P HTTP proxy on port 4444, trying alternative port");
                reqwest::Proxy::http("http://127.0.0.1:4447")
                    .unwrap_or_else(|_| {
                        error!("Failed to set I2P proxy on both ports 4444 and 4447");
                        panic!("Cannot initialize ProxyManager without I2P proxy");
                    })
            });
        
        // Also set HTTPS proxy for HTTPS I2P sites
        let i2p_proxy_https = reqwest::Proxy::https("http://127.0.0.1:4447")
            .unwrap_or_else(|_| {
                warn!("Failed to set I2P HTTPS proxy on port 4447, using HTTP proxy port");
                reqwest::Proxy::https("http://127.0.0.1:4444")
                    .unwrap_or_else(|_| {
                        warn!("Failed to set I2P HTTPS proxy, continuing without it");
                        // Create a dummy proxy that will fail gracefully
                        reqwest::Proxy::http("http://127.0.0.1:4444").unwrap()
                    })
            });
        
        Self {
            client: Client::builder()
                .proxy(i2p_proxy_http)
                .proxy(i2p_proxy_https)
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub async fn fetch_proxies(&self) -> Result<Vec<Proxy>, Box<dyn std::error::Error>> {
        info!("Fetching proxy list from I2P proxy address");
        
        let url = "http://proxygwdhg5z7mn326hfqqzsbnkrbzea4xrss2v7exrjx4c65uka.b32.i2p/";
        debug!("Making request to {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to fetch proxy list: {}", e);
                e
            })?;

        info!("Received response with status: {}", response.status());
        
        let html = response.text().await.map_err(|e| {
            error!("Failed to read response body: {}", e);
            e
        })?;

        debug!("Response body length: {} bytes", html.len());
        
        let proxies = self.parse_proxies(&html)?;
        info!("Parsed {} unique proxies", proxies.len());
        
        Ok(proxies)
    }

    fn parse_proxies(&self, html: &str) -> Result<Vec<Proxy>, Box<dyn std::error::Error>> {
        debug!("Parsing HTML for proxy addresses");
        let mut proxies = Vec::new();
        let mut seen = HashSet::new();

        // Parse HTML
        let document = Html::parse_document(html);
        
        // Pattern 0: Parse HTML table structure (primary method for outproxys.i2p)
        // The table has rows with: <td>address</td><td>port</td><td>uptime</td><td>type</td>
        let row_selector = Selector::parse("table tr").unwrap_or_else(|_| {
            warn!("Failed to create table row selector");
            Selector::parse("tr").unwrap()
        });
        
        for row in document.select(&row_selector) {
            let cells: Vec<_> = row.select(&Selector::parse("td").unwrap()).collect();
            if cells.len() >= 4 {
                // Extract address (first cell), port (second cell), and type (fourth cell)
                let address = cells[0].text().collect::<String>().trim().to_string();
                let port_str = cells[1].text().collect::<String>().trim().to_string();
                let proxy_type = cells[3].text().collect::<String>().trim().to_lowercase();
                
                // Only include HTTPS and SOCKS proxies, exclude HTTP
                if proxy_type == "https" || proxy_type == "socks" {
                    // Check if address is a valid I2P domain
                    if address.ends_with(".i2p") || address.ends_with(".b32.i2p") {
                        if let Ok(port) = port_str.parse::<u16>() {
                            let key = format!("{}:{}", address, port);
                            if seen.insert(key.clone()) {
                                debug!("Found {} proxy from table: {}:{}", proxy_type, address, port);
                                let pt = if proxy_type == "socks" {
                                    ProxyType::Socks
                                } else {
                                    ProxyType::Https
                                };
                                proxies.push(Proxy::new_with_type(address, port, pt));
                            }
                        }
                    }
                }
            }
        }
        
        // Try to find proxy addresses in various formats
        // Common patterns: host:port, http://host:port, etc.
        let text = document.root_element().text().collect::<String>();
        
        // Pattern 1: Look for host:port patterns (IPv4 addresses)
        // Skip this pattern as it's for clearnet proxies, not I2P proxies
        // We only want I2P proxies (which are in .i2p or .b32.i2p domains)

        // Pattern 2: Look for URLs in links (only HTTPS)
        let link_selector = Selector::parse("a[href]").unwrap_or_else(|_| {
            warn!("Failed to create link selector");
            Selector::parse("a").unwrap()
        });

        for element in document.select(&link_selector) {
            if let Some(href) = element.value().attr("href") {
                // Only process HTTPS URLs
                if href.starts_with("https://") {
                    if let Some(proxy) = Proxy::from_url(href) {
                        // Only include I2P domains
                        if proxy.host.ends_with(".i2p") || proxy.host.ends_with(".b32.i2p") {
                            let key = format!("{}:{}", proxy.host, proxy.port);
                            if seen.insert(key.clone()) {
                                debug!("Found HTTPS proxy from link: {}", key);
                                // Ensure it's marked as HTTPS type
                                let proxy = Proxy::new_with_type(proxy.host.clone(), proxy.port, ProxyType::Https);
                                proxies.push(proxy);
                            }
                        }
                    }
                }
            }
        }

        // Pattern 3: Look for HTTPS URLs (skip HTTP URLs)
        let url_pattern = regex::Regex::new(r"https://([^/\s:]+):?(\d{2,5})?")?;
        for cap in url_pattern.captures_iter(&text) {
            if let Some(host) = cap.get(1) {
                let host = host.as_str().to_string();
                // Only process I2P domains
                if host.ends_with(".i2p") || host.ends_with(".b32.i2p") {
                    let port: u16 = cap
                        .get(2)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(443); // Default HTTPS port

                    let key = format!("{}:{}", host, port);
                    if seen.insert(key.clone()) {
                        debug!("Found HTTPS proxy from URL pattern: {}", key);
                        proxies.push(Proxy::new_with_type(host, port, ProxyType::Https));
                    }
                }
            }
        }

        // Pattern 4: Look for .i2p domains with common HTTPS/SOCKS ports
        // This is a fallback pattern, but we prefer table parsing which has type information
        // Only include ports that are commonly used for HTTPS (443) or SOCKS (1080, 9050)
        let i2p_pattern = regex::Regex::new(r"([a-z0-9-]+\.i2p)(?::(\d{2,5}))?")?;
        for cap in i2p_pattern.captures_iter(&text) {
            if let Some(host) = cap.get(1) {
                let host = host.as_str().to_string();
                let port: u16 = cap
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                
                // Only include common HTTPS/SOCKS ports: 443 (HTTPS), 1080 (SOCKS), 9050 (SOCKS/Tor)
                // Skip default ports that are typically HTTP (80, 4444, 8080)
                if port == 443 || port == 1080 || port == 9050 {
                    let key = format!("{}:{}", host, port);
                    if seen.insert(key.clone()) {
                        debug!("Found I2P proxy from pattern (port {}): {}", port, key);
                        let pt = if port == 1080 || port == 9050 {
                            ProxyType::Socks
                        } else {
                            ProxyType::Https
                        };
                        proxies.push(Proxy::new_with_type(host, port, pt));
                    }
                }
            }
        }

        if proxies.is_empty() {
            warn!("No proxies found in HTML, returning empty list");
        }

        Ok(proxies)
    }
}

impl Default for ProxyManager {
    fn default() -> Self {
        Self::new()
    }
}

