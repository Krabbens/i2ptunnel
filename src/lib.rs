mod proxy_manager;
mod proxy_selector;
mod proxy_tester;
mod request_handler;

pub use proxy_manager::{Proxy, ProxyManager};
pub use proxy_selector::{ProxySelector, SelectedProxy};
pub use proxy_tester::{ProxyTestResult, ProxyTester};
pub use request_handler::{RequestConfig, RequestHandler, ResponseData};

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tracing::{error, info};

static RUNTIME: once_cell::sync::OnceCell<Runtime> = once_cell::sync::OnceCell::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        info!("Initializing Tokio runtime for PyO3");
        Runtime::new().expect("Failed to create Tokio runtime")
    })
}

#[pyclass]
pub struct I2PProxyDaemon {
    manager: Arc<ProxyManager>,
    selector: Arc<ProxySelector>,
    handler: Arc<RequestHandler>,
}

#[pymethods]
impl I2PProxyDaemon {
    #[new]
    fn new() -> PyResult<Self> {
        info!("Creating new I2PProxyDaemon instance");
        let manager = Arc::new(ProxyManager::new());
        let selector = Arc::new(ProxySelector::new(300));
        let handler = Arc::new(RequestHandler::new(selector.clone()));

        Ok(Self {
            manager,
            selector,
            handler,
        })
    }

    fn fetch_proxies(&self) -> PyResult<Vec<String>> {
        info!("Python: fetch_proxies called");
        let rt = get_runtime();
        let manager = self.manager.clone();

        rt.block_on(async move {
            match manager.fetch_proxies().await {
                Ok(proxies) => {
                    let urls: Vec<String> = proxies.iter().map(|p| p.url.clone()).collect();
                    info!("Fetched {} proxies", urls.len());
                    Ok(urls)
                }
                Err(e) => {
                    error!("Failed to fetch proxies: {}", e);
                    Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        format!("Failed to fetch proxies: {}", e),
                    ))
                }
            }
        })
    }

    fn test_proxies(&self, proxy_urls: Vec<String>) -> PyResult<PyObject> {
        info!("Python: test_proxies called with {} proxies", proxy_urls.len());
        let rt = get_runtime();
        let tester = ProxyTester::new(None);

        let proxies: Vec<Proxy> = proxy_urls
            .iter()
            .filter_map(|url| Proxy::from_url(url))
            .collect();

        let results = rt.block_on(async move {
            tester.test_proxies_parallel(proxies, 10).await
        });

        Python::with_gil(|py| {
            let list = PyList::empty(py);
            for result in results {
                let dict = PyDict::new(py);
                dict.set_item("proxy", result.proxy.url.as_str())?;
                dict.set_item("success", result.success)?;
                dict.set_item("speed_bytes_per_sec", result.speed_bytes_per_sec)?;
                dict.set_item("latency_ms", result.latency_ms)?;
                if let Some(ref error) = result.error {
                    dict.set_item("error", error.as_str())?;
                }
                list.append(dict)?;
            }
            Ok(list.to_object(py))
        })
    }

    fn make_request(
        &self,
        url: &str,
        method: &str,
        headers: Option<&PyDict>,
        body: Option<&PyBytes>,
    ) -> PyResult<PyObject> {
        info!("Python: make_request called: {} {}", method, url);
        let rt = get_runtime();
        let handler = self.handler.clone();
        let manager = self.manager.clone();

        // Fetch proxies if needed
        let proxies = rt.block_on(async move {
            manager.fetch_proxies().await.unwrap_or_default()
        });

        let mut request_config = RequestConfig {
            url: url.to_string(),
            method: method.to_string(),
            headers: None,
            body: None,
        };

        // Convert headers
        if let Some(headers_dict) = headers {
            Python::with_gil(|py| {
                let mut headers_map = std::collections::HashMap::new();
                for (key, value) in headers_dict {
                    if let (Ok(k), Ok(v)) = (
                        key.downcast::<PyString>(),
                        value.downcast::<PyString>(),
                    ) {
                        headers_map.insert(k.to_string(), v.to_string());
                    }
                }
                request_config.headers = Some(headers_map);
            });
        }

        // Convert body
        if let Some(body_bytes) = body {
            request_config.body = Some(body_bytes.as_bytes().to_vec());
        }

        let response = rt.block_on(async move {
            handler.handle_request(request_config, proxies).await
        });

        match response {
            Ok(response_data) => Python::with_gil(|py| {
                let dict = PyDict::new(py);
                dict.set_item("status", response_data.status)?;
                dict.set_item("proxy_used", response_data.proxy_used.as_str())?;

                let headers_dict = PyDict::new(py);
                for (key, value) in response_data.headers {
                    headers_dict.set_item(key, value)?;
                }
                dict.set_item("headers", headers_dict)?;

                let body_bytes = PyBytes::new(py, &response_data.body);
                dict.set_item("body", body_bytes)?;

                Ok(dict.to_object(py))
            }),
            Err(e) => {
                error!("Request failed: {}", e);
                Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))
            }
        }
    }

    fn get_fastest_proxy(&self) -> PyResult<Option<String>> {
        info!("Python: get_fastest_proxy called");
        if let Some(selected) = self.selector.get_current_proxy() {
            Ok(Some(selected.proxy.url))
        } else {
            Ok(None)
        }
    }
}

#[pymodule]
fn i2ptunnel(_py: Python, m: &PyModule) -> PyResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("i2ptunnel=debug".parse().unwrap()),
        )
        .init();

    info!("Initializing i2ptunnel Python module");
    m.add_class::<I2PProxyDaemon>()?;
    Ok(())
}

