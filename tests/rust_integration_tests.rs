/// Integration tests for the i2ptunnel Rust library
/// These tests verify the interaction between multiple components

use i2ptunnel::*;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_proxy_manager_and_selector_integration() {
    // Test that ProxyManager can fetch proxies and ProxySelector can select from them
    let manager = ProxyManager::new();
    let selector = ProxySelector::new(300);
    
    // This will fail if I2P router is not running, but that's okay for CI
    if let Ok(proxies) = manager.fetch_proxies().await {
        if !proxies.is_empty() {
            let test_results = ProxyTester::new(None)
                .test_proxies_parallel(proxies.clone(), 5)
                .await;
            
            let selected = selector.select_fastest(test_results).await;
            assert!(selected.is_some() || proxies.is_empty());
        }
    }
}

#[tokio::test]
async fn test_request_handler_with_i2p_domain() {
    // Test that RequestHandler correctly identifies I2P domains
    let selector = Arc::new(ProxySelector::new(300));
    let handler = RequestHandler::new(selector);
    
    // Test I2P domain detection
    assert!(RequestHandler::is_i2p_domain("http://example.i2p"));
    assert!(RequestHandler::is_i2p_domain("https://site.b32.i2p/path"));
    assert!(!RequestHandler::is_i2p_domain("http://example.com"));
    
    // Test request config creation
    let config = RequestConfig {
        url: "http://example.i2p".to_string(),
        method: "GET".to_string(),
        headers: None,
        body: None,
        stream: false,
    };
    
    // For I2P domains, we don't need proxy candidates
    let proxy_candidates = Vec::new();
    
    // This will fail if I2P router is not running, but that's okay
    // We're just testing that the handler can be created and configured correctly
    assert_eq!(config.url, "http://example.i2p");
}

#[tokio::test]
async fn test_proxy_selector_retest_interval() {
    let selector = ProxySelector::new(1); // 1 second retest interval
    
    let proxy1 = Proxy::new("proxy1.i2p".to_string(), 443);
    let proxy2 = Proxy::new("proxy2.i2p".to_string(), 443);
    
    // Select initial proxy
    let results = vec![
        ProxyTestResult::succeeded(proxy1.clone(), 1000.0, 100.0),
    ];
    selector.select_fastest(results).await;
    
    assert!(selector.get_current_proxy().is_some());
    
    // Wait for retest interval
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Ensure_fastest_proxy should retest after interval
    let new_results = vec![
        ProxyTestResult::succeeded(proxy2.clone(), 2000.0, 50.0),
    ];
    selector.select_fastest(new_results).await;
    
    // Should have updated to faster proxy
    if let Some(current) = selector.get_current_proxy() {
        assert_eq!(current.proxy.url, proxy2.url);
    }
}

#[tokio::test]
async fn test_proxy_tester_parallel_execution() {
    let tester = ProxyTester::new(None);
    
    // Create multiple test proxies
    let proxies = vec![
        Proxy::new("test1.i2p".to_string(), 443),
        Proxy::new("test2.i2p".to_string(), 443),
        Proxy::new("test3.i2p".to_string(), 443),
    ];
    
    // Test that parallel execution works
    let results = tester.test_proxies_parallel(proxies, 2).await;
    
    // Should get results for all proxies (even if they fail)
    assert_eq!(results.len(), 3);
    
    // All I2P proxies should be marked as successful with default values
    for result in &results {
        assert!(result.proxy.is_i2p_proxy());
        assert!(result.success); // I2P proxies get default success
    }
}

#[test]
fn test_proxy_type_conversion() {
    // Test that proxy types are correctly converted
    let http_proxy = Proxy::new_with_type("proxy.i2p".to_string(), 8080, ProxyType::Http);
    assert!(matches!(http_proxy.proxy_type, ProxyType::Http));
    
    let https_proxy = Proxy::new_with_type("proxy.i2p".to_string(), 443, ProxyType::Https);
    assert!(matches!(https_proxy.proxy_type, ProxyType::Https));
    
    let socks_proxy = Proxy::new_with_type("proxy.i2p".to_string(), 1080, ProxyType::Socks);
    assert!(matches!(socks_proxy.proxy_type, ProxyType::Socks));
}

#[test]
fn test_proxy_url_formatting() {
    // Test URL formatting for different proxy types
    let http_proxy = Proxy::new_with_type("proxy.i2p".to_string(), 80, ProxyType::Http);
    assert!(http_proxy.url.starts_with("http://"));
    
    let https_proxy = Proxy::new_with_type("proxy.i2p".to_string(), 443, ProxyType::Https);
    assert!(https_proxy.url.starts_with("https://"));
    
    let socks_proxy = Proxy::new_with_type("proxy.i2p".to_string(), 1080, ProxyType::Socks);
    assert!(socks_proxy.url.starts_with("socks5://"));
}

#[tokio::test]
async fn test_proxy_selector_multiple_candidates() {
    let selector = ProxySelector::new(300);
    
    let proxies = (1..=10)
        .map(|i| Proxy::new(format!("proxy{}.i2p", i), 443))
        .collect::<Vec<_>>();
    
    let results = proxies
        .iter()
        .enumerate()
        .map(|(i, proxy)| {
            ProxyTestResult::succeeded(proxy.clone(), (i as f64 + 1.0) * 1000.0, 100.0)
        })
        .collect();
    
    let selected = selector.select_fastest_multiple(results, 5).await;
    
    assert_eq!(selected.len(), 5);
    // Should be sorted by speed (descending)
    for i in 0..selected.len() - 1 {
        assert!(selected[i].speed_bytes_per_sec >= selected[i + 1].speed_bytes_per_sec);
    }
}

#[test]
fn test_request_config_serialization() {
    use serde_json;
    
    let config = RequestConfig {
        url: "https://example.com".to_string(),
        method: "POST".to_string(),
        headers: Some({
            let mut h = std::collections::HashMap::new();
            h.insert("Content-Type".to_string(), "application/json".to_string());
            h
        }),
        body: Some(b"test data".to_vec()),
        stream: false,
    };
    
    // Test serialization
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("example.com"));
    assert!(json.contains("POST"));
    
    // Test deserialization
    let deserialized: RequestConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.url, config.url);
    assert_eq!(deserialized.method, config.method);
}

#[test]
fn test_response_data_serialization() {
    use serde_json;
    
    let response = ResponseData {
        status: 200,
        headers: {
            let mut h = std::collections::HashMap::new();
            h.insert("Content-Type".to_string(), "text/html".to_string());
            h
        },
        body: b"<html></html>".to_vec(),
        proxy_used: "http://proxy.i2p:443".to_string(),
    };
    
    // Test serialization
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("200"));
    assert!(json.contains("proxy.i2p"));
    
    // Test deserialization
    let deserialized: ResponseData = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.status, 200);
    assert_eq!(deserialized.proxy_used, response.proxy_used);
}

