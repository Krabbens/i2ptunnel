use crate::proxy_manager::Proxy;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

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

        // Create client with proxy
        let client = match reqwest::Proxy::http(&proxy.url)
            .map_err(|e| format!("Failed to create proxy: {}", e))
            .and_then(|p| {
                Client::builder()
                    .proxy(p)
                    .timeout(self.test_timeout)
                    .build()
                    .map_err(|e| format!("Failed to create client: {}", e))
            }) {
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
        let latency_result = client.head(&self.test_url).send().await;
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

