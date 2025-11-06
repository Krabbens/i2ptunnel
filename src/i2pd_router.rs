use std::ffi::CString;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};
use once_cell::sync::Lazy;

// Include generated bindings
include!(concat!(env!("OUT_DIR"), "/i2pd_bindings.rs"));

static ROUTER_STATE: Lazy<Arc<Mutex<RouterState>>> = Lazy::new(|| {
    Arc::new(Mutex::new(RouterState {
        initialized: false,
        running: false,
    }))
});

struct RouterState {
    initialized: bool,
    running: bool,
}

pub struct I2PDRouter {
    config_dir: Option<String>,
}

impl I2PDRouter {
    pub fn new(config_dir: Option<String>) -> Self {
        Self { config_dir }
    }

    pub fn init(&self) -> Result<(), String> {
        let mut state = ROUTER_STATE.lock().unwrap();
        if state.initialized {
            debug!("i2pd router already initialized");
            return Ok(());
        }

        info!("Initializing i2pd router");
        let config_dir_cstr = if let Some(ref dir) = self.config_dir {
            CString::new(dir.clone()).map_err(|e| format!("Invalid config directory: {}", e))?
        } else {
            CString::new(".").unwrap()
        };

        let result = unsafe {
            i2pd_router_init(config_dir_cstr.as_ptr())
        };

        if result == 0 {
            state.initialized = true;
            info!("i2pd router initialized successfully");
            Ok(())
        } else {
            error!("Failed to initialize i2pd router");
            Err("Failed to initialize i2pd router".to_string())
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut state = ROUTER_STATE.lock().unwrap();
        if state.running {
            debug!("i2pd router already running");
            return Ok(());
        }

        if !state.initialized {
            drop(state);
            self.init()?;
            state = ROUTER_STATE.lock().unwrap();
        }

        info!("Starting i2pd router");
        let result = unsafe {
            i2pd_router_start()
        };

        if result == 0 {
            // Start HTTP and HTTPS proxies
            let http_result = unsafe {
                let addr = CString::new("127.0.0.1").unwrap();
                i2pd_http_proxy_start(addr.as_ptr(), 4444)
            };
            
            let https_result = unsafe {
                let addr = CString::new("127.0.0.1").unwrap();
                i2pd_https_proxy_start(addr.as_ptr(), 4447)
            };

            if http_result == 0 && https_result == 0 {
                state.running = true;
                info!("i2pd router started successfully with HTTP (4444) and HTTPS (4447) proxies");
                Ok(())
            } else {
                warn!("i2pd router started but proxy initialization had issues");
                state.running = true;
                Ok(())
            }
        } else {
            error!("Failed to start i2pd router");
            Err("Failed to start i2pd router".to_string())
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut state = ROUTER_STATE.lock().unwrap();
        if !state.running {
            debug!("i2pd router not running");
            return Ok(());
        }

        info!("Stopping i2pd router");
        let result = unsafe {
            i2pd_router_stop()
        };

        if result == 0 {
            state.running = false;
            info!("i2pd router stopped successfully");
            Ok(())
        } else {
            error!("Failed to stop i2pd router");
            Err("Failed to stop i2pd router".to_string())
        }
    }

    pub fn is_running(&self) -> bool {
        let state = ROUTER_STATE.lock().unwrap();
        state.running && unsafe { i2pd_router_is_running() != 0 }
    }

    pub fn ensure_running(&self) -> Result<(), String> {
        if !self.is_running() {
            self.start()?;
        }
        Ok(())
    }
}

impl Drop for I2PDRouter {
    fn drop(&mut self) {
        let _ = self.stop();
        unsafe {
            i2pd_router_cleanup();
        }
    }
}

// Global router instance
static GLOBAL_ROUTER: Lazy<Arc<Mutex<Option<Arc<I2PDRouter>>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

pub fn get_or_init_router() -> Arc<I2PDRouter> {
    let mut router_opt = GLOBAL_ROUTER.lock().unwrap();
    if let Some(ref router) = *router_opt {
        router.clone()
    } else {
        let router = Arc::new(I2PDRouter::new(None));
        *router_opt = Some(router.clone());
        router
    }
}

pub fn ensure_router_running() -> Result<(), String> {
    let router = get_or_init_router();
    router.ensure_running()
}
