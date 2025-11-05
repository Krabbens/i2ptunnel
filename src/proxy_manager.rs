use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use tracing::{debug, error, info, warn};
use url::Url;
use regex;

#[derive(Debug, Clone)]
pub struct Proxy {
    pub host: String,
    pub port: u16,
    pub url: String,
}

impl Proxy {
    pub fn new(host: String, port: u16) -> Self {
        let url = format!("http://{}:{}", host, port);
        Self { host, port, url }
    }

    pub fn from_url(url_str: &str) -> Option<Self> {
        match Url::parse(url_str) {
            Ok(url) => {
                let host = url.host_str()?.to_string();
                let port = url.port().unwrap_or(80);
                Some(Self::new(host, port))
            }
            Err(e) => {
                warn!("Failed to parse proxy URL {}: {}", url_str, e);
                None
            }
        }
    }
}

pub struct ProxyManager {
    client: Client,
}

impl ProxyManager {
    pub fn new() -> Self {
        info!("Initializing ProxyManager");
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub async fn fetch_proxies(&self) -> Result<Vec<Proxy>, Box<dyn std::error::Error>> {
        info!("Fetching proxy list from http://outproxys.i2p/");
        
        let url = "http://outproxys.i2p/";
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
        
        // Try to find proxy addresses in various formats
        // Common patterns: host:port, http://host:port, etc.
        let text = document.root_element().text().collect::<String>();
        
        // Pattern 1: Look for host:port patterns
        let host_port_pattern = regex::Regex::new(r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}):(\d{2,5})")?;
        for cap in host_port_pattern.captures_iter(&text) {
            let host = cap[1].to_string();
            let port: u16 = cap[2].parse().unwrap_or(0);
            if port > 0 && port < 65536 {
                let key = format!("{}:{}", host, port);
                if seen.insert(key.clone()) {
                    debug!("Found proxy: {}", key);
                    proxies.push(Proxy::new(host, port));
                }
            }
        }

        // Pattern 2: Look for URLs in links
        let link_selector = Selector::parse("a[href]").unwrap_or_else(|_| {
            warn!("Failed to create link selector");
            Selector::parse("a").unwrap()
        });

        for element in document.select(&link_selector) {
            if let Some(href) = element.value().attr("href") {
                if let Some(proxy) = Proxy::from_url(href) {
                    let key = format!("{}:{}", proxy.host, proxy.port);
                    if seen.insert(key.clone()) {
                        debug!("Found proxy from link: {}", key);
                        proxies.push(proxy);
                    }
                }
            }
        }

        // Pattern 3: Look for text content that might be proxy addresses
        let url_pattern = regex::Regex::new(r"https?://([^/\s:]+):?(\d{2,5})?")?;
        for cap in url_pattern.captures_iter(&text) {
            if let Some(host) = cap.get(1) {
                let host = host.as_str().to_string();
                let port: u16 = cap
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(4444); // Default I2P outproxy port

                let key = format!("{}:{}", host, port);
                if seen.insert(key.clone()) {
                    debug!("Found proxy from URL pattern: {}", key);
                    proxies.push(Proxy::new(host, port));
                }
            }
        }

        // Pattern 4: Look for .i2p domains
        let i2p_pattern = regex::Regex::new(r"([a-z0-9-]+\.i2p)(?::(\d{2,5}))?")?;
        for cap in i2p_pattern.captures_iter(&text) {
            if let Some(host) = cap.get(1) {
                let host = host.as_str().to_string();
                let port: u16 = cap
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(4444);

                let key = format!("{}:{}", host, port);
                if seen.insert(key.clone()) {
                    debug!("Found I2P proxy: {}", key);
                    proxies.push(Proxy::new(host, port));
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

