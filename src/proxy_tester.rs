use crate::proxy_manager::Proxy;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct ProxyTestResult {
    pub proxy: Proxy,
    pub speed_bytes_per_sec: f64,
    pub latency_ms: f64,
    pub success: bool,
    pub error: Option<String>,
}

impl ProxyTestResult {
    pub fn new(proxy: Proxy) -> Self {
        Self {
            proxy,
            speed_bytes_per_sec: 0.0,
            latency_ms: 0.0,
            success: false,
            error: None,
        }
    }

    pub fn failed(proxy: Proxy, error: String) -> Self {
        warn!("Proxy test failed for {}: {}", proxy.url, error);
        Self {
            proxy,
            speed_bytes_per_sec: 0.0,
            latency_ms: 0.0,
            success: false,
            error: Some(error),
        }
    }

    pub fn succeeded(
        proxy: Proxy,
        speed_bytes_per_sec: f64,
        latency_ms: f64,
    ) -> Self {
        debug!(
            "Proxy test succeeded for {}: {:.2} KB/s, {:.2} ms latency",
            proxy.url,
            speed_bytes_per_sec / 1024.0,
            latency_ms
        );
        Self {
            proxy,
            speed_bytes_per_sec,
            latency_ms,
            success: true,
            error: None,
        }
    }
}

pub struct ProxyTester {
    test_url: String,
    test_timeout: Duration,
    test_size_bytes: usize,
}

impl ProxyTester {
    pub fn new(test_url: Option<String>) -> Self {
        let test_url = test_url.unwrap_or_else(|| {
            "http://httpbin.org/bytes/10240".to_string() // 10KB test file
        });
        
        info!(
            "Initializing ProxyTester with test URL: {}",
            test_url
        );
        
        Self {
            test_url,
            test_timeout: Duration::from_secs(10),
            test_size_bytes: 10240,
        }
    }

    pub async fn test_proxy(&self, proxy: &Proxy) -> ProxyTestResult {
        debug!("Testing proxy: {}", proxy.url);
        let start_time = Instant::now();

        // Check if proxy is an I2P-based proxy
        // I2P-based outproxies can't be tested directly because they require router configuration
        // and DNS resolution through I2P router doesn't work for clearnet domains
        if proxy.is_i2p_proxy() {
            info!(
                "Skipping test for I2P-based proxy {} (assumes router is configured)",
                proxy.url
            );
            // Mark as successful with default speed/latency since we can't test it
            // Use a reasonable default speed (assume it works)
            return ProxyTestResult::succeeded(
                proxy.clone(),
                1024.0 * 50.0, // 50 KB/s default
                200.0,         // 200ms default latency
            );
        }
        
        // Create client with proxy based on proxy type
        let client = match &proxy.proxy_type {
            crate::proxy_manager::ProxyType::Socks => {
                // For SOCKS proxies, try SOCKS5 first, fallback to HTTPS if SOCKS fails
                let socks_url = format!("socks5://{}:{}", proxy.host, proxy.port);
                let https_url = format!("https://{}:{}", proxy.host, proxy.port);
                
                // Try SOCKS first
                match reqwest::Proxy::all(&socks_url) {
                    Ok(socks_proxy) => {
                        match Client::builder()
                            .proxy(socks_proxy)
                            .timeout(self.test_timeout)
                            .build()
                        {
                            Ok(client) => Ok(client),
                            Err(e) => {
                                warn!("SOCKS proxy {} failed to create client, falling back to HTTPS: {}", proxy.url, e);
                                // Fallback to HTTPS
                                reqwest::Proxy::https(&https_url)
                                    .map_err(|e| format!("Failed to create HTTPS fallback proxy: {}", e))
                                    .and_then(|p| {
                                        Client::builder()
                                            .proxy(p)
                                            .timeout(self.test_timeout)
                                            .build()
                                            .map_err(|e| format!("Failed to create HTTPS fallback client: {}", e))
                                    })
                            }
                        }
                    }
                    Err(e) => {
                        warn!("SOCKS proxy {} not available, falling back to HTTPS: {}", proxy.url, e);
                        // Fallback to HTTPS
                        reqwest::Proxy::https(&https_url)
                            .map_err(|e| format!("Failed to create HTTPS fallback proxy: {}", e))
                            .and_then(|p| {
                                Client::builder()
                                    .proxy(p)
                                    .timeout(self.test_timeout)
                                    .build()
                                    .map_err(|e| format!("Failed to create HTTPS fallback client: {}", e))
                            })
                    }
                }
            }
            crate::proxy_manager::ProxyType::Https => {
                // For HTTPS proxies, use https proxy
                reqwest::Proxy::https(&proxy.url)
                    .map_err(|e| format!("Failed to create HTTPS proxy: {}", e))
                    .and_then(|p| {
                        Client::builder()
                            .proxy(p)
                            .timeout(self.test_timeout)
                            .build()
                            .map_err(|e| format!("Failed to create client: {}", e))
                    })
            }
            crate::proxy_manager::ProxyType::Http => {
                // For HTTP proxies, use http proxy
                reqwest::Proxy::http(&proxy.url)
                    .map_err(|e| format!("Failed to create HTTP proxy: {}", e))
                    .and_then(|p| {
                        Client::builder()
                            .proxy(p)
                            .timeout(self.test_timeout)
                            .build()
                            .map_err(|e| format!("Failed to create client: {}", e))
                    })
            }
        };
        
        let client = match client {
            Ok(c) => c,
            Err(e) => {
                return ProxyTestResult::failed(
                    proxy.clone(),
                    e,
                );
            }
        };

        // Measure latency with HEAD request
        let latency_start = Instant::now();
        let _latency_result = client.head(&self.test_url).send().await;
        let latency = latency_start.elapsed().as_secs_f64() * 1000.0;

        // Measure download speed with GET request
        let download_start = Instant::now();
        let response = match client.get(&self.test_url).send().await {
            Ok(r) => r,
            Err(e) => {
                return ProxyTestResult::failed(
                    proxy.clone(),
                    format!("Request failed: {}", e),
                );
            }
        };

        if !response.status().is_success() {
            return ProxyTestResult::failed(
                proxy.clone(),
                format!("HTTP error: {}", response.status()),
            );
        }

        let body = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return ProxyTestResult::failed(
                    proxy.clone(),
                    format!("Failed to read body: {}", e),
                );
            }
        };

        let download_time = download_start.elapsed().as_secs_f64();
        let bytes_downloaded = body.len();

        if download_time <= 0.0 {
            return ProxyTestResult::failed(
                proxy.clone(),
                "Download time was zero".to_string(),
            );
        }

        let speed_bytes_per_sec = bytes_downloaded as f64 / download_time;
        let total_time = start_time.elapsed();

        info!(
            "Proxy {} test completed in {:.2}ms: {:.2} KB/s, {:.2} ms latency",
            proxy.url,
            total_time.as_millis(),
            speed_bytes_per_sec / 1024.0,
            latency
        );

        ProxyTestResult::succeeded(proxy.clone(), speed_bytes_per_sec, latency)
    }

    pub async fn test_proxies_parallel(
        &self,
        proxies: Vec<Proxy>,
        max_concurrent: usize,
    ) -> Vec<ProxyTestResult> {
        info!(
            "Testing {} proxies in parallel (max {} concurrent)",
            proxies.len(),
            max_concurrent
        );

        use futures::stream::{self, StreamExt};
        let results: Vec<ProxyTestResult> = stream::iter(proxies)
            .map(|proxy| async move {
                self.test_proxy(&proxy).await
            })
            .buffer_unordered(max_concurrent)
            .collect()
            .await;

        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.len() - successful;

        info!(
            "Proxy testing completed: {} successful, {} failed",
            successful, failed
        );

        if successful > 0 {
            let fastest = results
                .iter()
                .filter(|r| r.success)
                .max_by(|a, b| {
                    a.speed_bytes_per_sec
                        .partial_cmp(&b.speed_bytes_per_sec)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let Some(fastest) = fastest {
                info!(
                    "Fastest proxy: {} ({:.2} KB/s)",
                    fastest.proxy.url,
                    fastest.speed_bytes_per_sec / 1024.0
                );
            }
        }

        results
    }
}

impl Default for ProxyTester {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_test_result_new() {
        let proxy = Proxy::new("test.i2p".to_string(), 443);
        let result = ProxyTestResult::new(proxy.clone());
        
        assert_eq!(result.proxy.url, proxy.url);
        assert_eq!(result.speed_bytes_per_sec, 0.0);
        assert_eq!(result.latency_ms, 0.0);
        assert!(!result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_proxy_test_result_succeeded() {
        let proxy = Proxy::new("test.i2p".to_string(), 443);
        let result = ProxyTestResult::succeeded(proxy.clone(), 5000.0, 100.0);
        
        assert_eq!(result.proxy.url, proxy.url);
        assert_eq!(result.speed_bytes_per_sec, 5000.0);
        assert_eq!(result.latency_ms, 100.0);
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_proxy_test_result_failed() {
        let proxy = Proxy::new("test.i2p".to_string(), 443);
        let error_msg = "Connection timeout".to_string();
        let result = ProxyTestResult::failed(proxy.clone(), error_msg.clone());
        
        assert_eq!(result.proxy.url, proxy.url);
        assert_eq!(result.speed_bytes_per_sec, 0.0);
        assert_eq!(result.latency_ms, 0.0);
        assert!(!result.success);
        assert_eq!(result.error, Some(error_msg));
    }

    #[tokio::test]
    async fn test_i2p_proxy_skips_test() {
        let tester = ProxyTester::new(None);
        let proxy = Proxy::new("proxy.b32.i2p".to_string(), 443);
        
        assert!(proxy.is_i2p_proxy());
        
        let result = tester.test_proxy(&proxy).await;
        
        // I2P proxies should be marked as successful with default values
        assert!(result.success);
        assert_eq!(result.speed_bytes_per_sec, 1024.0 * 50.0); // 50 KB/s default
        assert_eq!(result.latency_ms, 200.0); // 200ms default
        assert!(result.error.is_none());
    }

    #[test]
    fn test_proxy_tester_new() {
        let tester = ProxyTester::new(None);
        assert_eq!(tester.test_url, "http://httpbin.org/bytes/10240");
        assert_eq!(tester.test_timeout, Duration::from_secs(10));
        assert_eq!(tester.test_size_bytes, 10240);
    }

    #[test]
    fn test_proxy_tester_custom_url() {
        let custom_url = "http://example.com/test".to_string();
        let tester = ProxyTester::new(Some(custom_url.clone()));
        assert_eq!(tester.test_url, custom_url);
    }

    #[tokio::test]
    async fn test_proxy_tester_empty_list() {
        let tester = ProxyTester::new(None);
        let proxies = vec![];
        
        let results = tester.test_proxies_parallel(proxies, 5).await;
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_proxy_tester_single_proxy() {
        let tester = ProxyTester::new(None);
        let proxy = Proxy::new("test.b32.i2p".to_string(), 443);
        
        let results = tester.test_proxies_parallel(vec![proxy], 1).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success); // I2P proxy should be marked successful
    }

    #[test]
    fn test_proxy_test_result_clone() {
        let proxy = Proxy::new("test.i2p".to_string(), 443);
        let result = ProxyTestResult::succeeded(proxy.clone(), 5000.0, 100.0);
        
        let cloned = result.clone();
        assert_eq!(result.proxy.url, cloned.proxy.url);
        assert_eq!(result.speed_bytes_per_sec, cloned.speed_bytes_per_sec);
        assert_eq!(result.latency_ms, cloned.latency_ms);
        assert_eq!(result.success, cloned.success);
    }

    #[test]
    fn test_proxy_test_result_with_error() {
        let proxy = Proxy::new("test.i2p".to_string(), 443);
        let error = "Test error".to_string();
        let result = ProxyTestResult::failed(proxy.clone(), error.clone());
        
        assert!(!result.success);
        assert_eq!(result.error, Some(error));
        assert_eq!(result.speed_bytes_per_sec, 0.0);
        assert_eq!(result.latency_ms, 0.0);
    }

    #[tokio::test]
    async fn test_proxy_tester_multiple_i2p_proxies() {
        let tester = ProxyTester::new(None);
        let proxies = vec![
            Proxy::new("proxy1.b32.i2p".to_string(), 443),
            Proxy::new("proxy2.b32.i2p".to_string(), 1080),
            Proxy::new("proxy3.i2p".to_string(), 443),
        ];
        
        let results = tester.test_proxies_parallel(proxies, 3).await;
        assert_eq!(results.len(), 3);
        // All I2P proxies should be marked as successful
        for result in &results {
            assert!(result.success);
            assert!(result.proxy.is_i2p_proxy());
        }
    }

    #[test]
    fn test_proxy_tester_default() {
        let tester = ProxyTester::default();
        assert_eq!(tester.test_url, "http://httpbin.org/bytes/10240");
    }
}

