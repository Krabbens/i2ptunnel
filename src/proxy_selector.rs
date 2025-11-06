use crate::proxy_manager::Proxy;
use crate::proxy_tester::{ProxyTestResult, ProxyTester};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct SelectedProxy {
    pub proxy: Proxy,
    pub speed_bytes_per_sec: f64,
    pub selected_at: Instant,
}

pub struct ProxySelector {
    current_proxy: Arc<RwLock<Option<SelectedProxy>>>,
    tester: ProxyTester,
    retest_interval: Duration,
    last_retest: Arc<RwLock<Instant>>,
}

impl ProxySelector {
    pub fn new(retest_interval_secs: u64) -> Self {
        info!(
            "Initializing ProxySelector with retest interval: {}s",
            retest_interval_secs
        );
        Self {
            current_proxy: Arc::new(RwLock::new(None)),
            tester: ProxyTester::new(None),
            retest_interval: Duration::from_secs(retest_interval_secs),
            last_retest: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub async fn select_fastest(
        &self,
        test_results: Vec<ProxyTestResult>,
    ) -> Option<SelectedProxy> {
        info!("Selecting fastest proxy from {} results", test_results.len());

        let successful_results: Vec<&ProxyTestResult> = test_results
            .iter()
            .filter(|r| r.success)
            .collect();

        if successful_results.is_empty() {
            warn!("No successful proxy tests found");
            return None;
        }

        let fastest = successful_results.iter().max_by(|a, b| {
            a.speed_bytes_per_sec
                .partial_cmp(&b.speed_bytes_per_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

        let selected = SelectedProxy {
            proxy: fastest.proxy.clone(),
            speed_bytes_per_sec: fastest.speed_bytes_per_sec,
            selected_at: Instant::now(),
        };

        info!(
            "Selected fastest proxy: {} ({:.2} KB/s)",
            selected.proxy.url,
            selected.speed_bytes_per_sec / 1024.0
        );

        *self.current_proxy.write() = Some(selected.clone());
        Some(selected)
    }

    pub async fn select_fastest_multiple(
        &self,
        test_results: Vec<ProxyTestResult>,
        count: usize,
    ) -> Vec<SelectedProxy> {
        info!("Selecting top {} fastest proxies from {} results", count, test_results.len());

        let mut successful_results: Vec<&ProxyTestResult> = test_results
            .iter()
            .filter(|r| r.success)
            .collect();

        if successful_results.is_empty() {
            warn!("No successful proxy tests found");
            return Vec::new();
        }

        // Sort by speed (descending)
        successful_results.sort_by(|a, b| {
            b.speed_bytes_per_sec
                .partial_cmp(&a.speed_bytes_per_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top N
        let selected: Vec<SelectedProxy> = successful_results
            .iter()
            .take(count)
            .map(|result| SelectedProxy {
                proxy: result.proxy.clone(),
                speed_bytes_per_sec: result.speed_bytes_per_sec,
                selected_at: Instant::now(),
            })
            .collect();

        if !selected.is_empty() {
            info!(
                "Selected top {} proxies, fastest: {} ({:.2} KB/s)",
                selected.len(),
                selected[0].proxy.url,
                selected[0].speed_bytes_per_sec / 1024.0
            );
            // Cache the fastest one
            *self.current_proxy.write() = Some(selected[0].clone());
        }

        selected
    }

    pub fn get_current_proxy(&self) -> Option<SelectedProxy> {
        self.current_proxy.read().as_ref().cloned()
    }

    pub async fn ensure_fastest_proxy(
        &self,
        available_proxies: Vec<Proxy>,
    ) -> Result<Option<SelectedProxy>, Box<dyn std::error::Error>> {
        let now = Instant::now();
        let last_retest_time = *self.last_retest.read();

        // Check if we need to retest
        if now.duration_since(last_retest_time) >= self.retest_interval {
            info!("Retest interval reached, testing proxies again");
            *self.last_retest.write() = now;

            let max_concurrent = (available_proxies.len().min(10)).max(1);
            let test_results = self
                .tester
                .test_proxies_parallel(available_proxies, max_concurrent)
                .await;

            return Ok(self.select_fastest(test_results).await);
        }

        // Return current proxy if we have one
        if let Some(proxy) = self.get_current_proxy() {
            debug!("Using cached fastest proxy: {}", proxy.proxy.url);
            Ok(Some(proxy))
        } else {
            warn!("No current proxy available, testing proxies");
            let max_concurrent = (available_proxies.len().min(10)).max(1);
            let test_results = self
                .tester
                .test_proxies_parallel(available_proxies, max_concurrent)
                .await;

            Ok(self.select_fastest(test_results).await)
        }
    }

    pub async fn ensure_multiple_proxy_candidates(
        &self,
        available_proxies: Vec<Proxy>,
        count: usize,
    ) -> Result<Vec<SelectedProxy>, Box<dyn std::error::Error>> {
        let now = Instant::now();
        let last_retest_time = *self.last_retest.read();

        // Check if we need to retest
        if now.duration_since(last_retest_time) >= self.retest_interval {
            info!("Retest interval reached, testing proxies again");
            *self.last_retest.write() = now;

            let max_concurrent = (available_proxies.len().min(10)).max(1);
            let test_results = self
                .tester
                .test_proxies_parallel(available_proxies, max_concurrent)
                .await;

            return Ok(self.select_fastest_multiple(test_results, count).await);
        }

        // If we have a current proxy, try to return it plus get more if needed
        let current_proxy = self.get_current_proxy();
        if let Some(proxy) = current_proxy {
            debug!("Using cached fastest proxy: {}", proxy.proxy.url);
            // If we only need one, return just this
            if count == 1 {
                return Ok(vec![proxy]);
            }
            // Otherwise, we should test to get multiple candidates
            // But for efficiency, return current + test for more
        }

        // Test to get multiple candidates
        info!("Testing {} proxies to get {} candidates", available_proxies.len(), count);
        let max_concurrent = (available_proxies.len().min(10)).max(1);
        info!("Testing proxies in parallel (max_concurrent={})", max_concurrent);
        let test_results = self
            .tester
            .test_proxies_parallel(available_proxies, max_concurrent)
            .await;
        
        info!("Proxy testing completed: {} results", test_results.len());
        let selected = self.select_fastest_multiple(test_results, count).await;
        info!("Selected {} proxy candidates from test results", selected.len());
        Ok(selected)
    }

    pub async fn handle_proxy_failure(&self, failed_proxy: &Proxy) {
        warn!("Proxy failure detected: {}", failed_proxy.url);
        
        let current = self.current_proxy.read();
        if let Some(ref current_proxy) = *current {
            if current_proxy.proxy.url == failed_proxy.url {
                info!("Failed proxy is the current one, clearing selection");
                drop(current);
                *self.current_proxy.write() = None;
            }
        }
    }
}

impl Default for ProxySelector {
    fn default() -> Self {
        Self::new(300) // 5 minutes default retest interval
    }
}


