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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_tester::ProxyTestResult;

    #[tokio::test]
    async fn test_select_fastest_from_results() {
        let selector = ProxySelector::new(300);
        
        let proxy1 = Proxy::new("proxy1.i2p".to_string(), 443);
        let proxy2 = Proxy::new("proxy2.i2p".to_string(), 443);
        let proxy3 = Proxy::new("proxy3.i2p".to_string(), 443);
        
        let results = vec![
            ProxyTestResult::succeeded(proxy1.clone(), 1000.0, 100.0),
            ProxyTestResult::succeeded(proxy2.clone(), 5000.0, 50.0), // Fastest
            ProxyTestResult::succeeded(proxy3.clone(), 2000.0, 150.0),
        ];
        
        let selected = selector.select_fastest(results).await;
        assert!(selected.is_some());
        let selected = selected.unwrap();
        assert_eq!(selected.proxy.url, proxy2.url);
        assert_eq!(selected.speed_bytes_per_sec, 5000.0);
    }

    #[tokio::test]
    async fn test_select_fastest_no_successful() {
        let selector = ProxySelector::new(300);
        
        let proxy1 = Proxy::new("proxy1.i2p".to_string(), 443);
        let results = vec![
            ProxyTestResult::failed(proxy1.clone(), "Connection failed".to_string()),
        ];
        
        let selected = selector.select_fastest(results).await;
        assert!(selected.is_none());
    }

    #[tokio::test]
    async fn test_select_fastest_multiple() {
        let selector = ProxySelector::new(300);
        
        let proxy1 = Proxy::new("proxy1.i2p".to_string(), 443);
        let proxy2 = Proxy::new("proxy2.i2p".to_string(), 443);
        let proxy3 = Proxy::new("proxy3.i2p".to_string(), 443);
        let proxy4 = Proxy::new("proxy4.i2p".to_string(), 443);
        
        let results = vec![
            ProxyTestResult::succeeded(proxy1.clone(), 1000.0, 100.0),
            ProxyTestResult::succeeded(proxy2.clone(), 5000.0, 50.0), // Fastest
            ProxyTestResult::succeeded(proxy3.clone(), 2000.0, 150.0),
            ProxyTestResult::succeeded(proxy4.clone(), 3000.0, 120.0),
        ];
        
        let selected = selector.select_fastest_multiple(results, 3).await;
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].proxy.url, proxy2.url); // Should be sorted by speed
        assert_eq!(selected[0].speed_bytes_per_sec, 5000.0);
        assert_eq!(selected[1].speed_bytes_per_sec, 3000.0);
        assert_eq!(selected[2].speed_bytes_per_sec, 2000.0);
    }

    #[test]
    fn test_get_current_proxy() {
        let selector = ProxySelector::new(300);
        assert!(selector.get_current_proxy().is_none());
    }

    #[tokio::test]
    async fn test_handle_proxy_failure() {
        let selector = ProxySelector::new(300);
        
        let proxy1 = Proxy::new("proxy1.i2p".to_string(), 443);
        let proxy2 = Proxy::new("proxy2.i2p".to_string(), 443);
        
        // Select a proxy first
        let results = vec![
            ProxyTestResult::succeeded(proxy1.clone(), 1000.0, 100.0),
        ];
        selector.select_fastest(results).await;
        
        assert!(selector.get_current_proxy().is_some());
        
        // Handle failure of current proxy
        selector.handle_proxy_failure(&proxy1).await;
        
        assert!(selector.get_current_proxy().is_none());
        
        // Handle failure of non-current proxy (should not affect current)
        let results = vec![
            ProxyTestResult::succeeded(proxy2.clone(), 2000.0, 100.0),
        ];
        selector.select_fastest(results).await;
        assert!(selector.get_current_proxy().is_some());
        
        selector.handle_proxy_failure(&proxy1).await; // Different proxy
        assert!(selector.get_current_proxy().is_some()); // Should still have current
    }

    #[tokio::test]
    async fn test_select_fastest_empty_results() {
        let selector = ProxySelector::new(300);
        let results = vec![];
        
        let selected = selector.select_fastest(results).await;
        assert!(selected.is_none());
    }

    #[tokio::test]
    async fn test_select_fastest_multiple_empty_results() {
        let selector = ProxySelector::new(300);
        let results = vec![];
        
        let selected = selector.select_fastest_multiple(results, 5).await;
        assert_eq!(selected.len(), 0);
    }

    #[tokio::test]
    async fn test_select_fastest_multiple_request_more_than_available() {
        let selector = ProxySelector::new(300);
        
        let proxy1 = Proxy::new("proxy1.i2p".to_string(), 443);
        let proxy2 = Proxy::new("proxy2.i2p".to_string(), 443);
        
        let results = vec![
            ProxyTestResult::succeeded(proxy1.clone(), 1000.0, 100.0),
            ProxyTestResult::succeeded(proxy2.clone(), 2000.0, 100.0),
        ];
        
        let selected = selector.select_fastest_multiple(results, 10).await;
        // Should return only available proxies
        assert_eq!(selected.len(), 2);
    }

    #[tokio::test]
    async fn test_ensure_fastest_proxy_with_empty_list() {
        let selector = ProxySelector::new(300);
        let proxies = vec![];
        
        let result = selector.ensure_fastest_proxy(proxies).await;
        // Should handle empty list gracefully
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ensure_multiple_proxy_candidates_with_empty_list() {
        let selector = ProxySelector::new(300);
        let proxies = vec![];
        
        let result = selector.ensure_multiple_proxy_candidates(proxies, 5).await;
        // Should handle empty list gracefully
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_selected_proxy_clone() {
        let proxy = Proxy::new("test.i2p".to_string(), 443);
        let selected = SelectedProxy {
            proxy: proxy.clone(),
            speed_bytes_per_sec: 1000.0,
            selected_at: std::time::Instant::now(),
        };
        
        let cloned = selected.clone();
        assert_eq!(selected.proxy.url, cloned.proxy.url);
        assert_eq!(selected.speed_bytes_per_sec, cloned.speed_bytes_per_sec);
    }

    #[tokio::test]
    async fn test_proxy_selector_default() {
        let selector = ProxySelector::default();
        assert!(selector.get_current_proxy().is_none());
    }
}


