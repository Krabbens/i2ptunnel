/* C wrapper implementation for i2pd HTTP proxy functionality */
#include "libi2pd_wrapper/capi.h"
#include "libi2pd/api.h"
#include "libi2pd_client/ClientContext.h"
#include "libi2pd_client/HTTPProxy.h"
#include <memory>
#include <string>
#include <mutex>

static std::mutex router_mutex;
static bool router_initialized = false;
static bool router_running = false;
static std::shared_ptr<i2p::proxy::HTTPProxy> http_proxy;
static std::shared_ptr<i2p::proxy::HTTPProxy> https_proxy;

extern "C" {

// Forward declarations
void i2pd_http_proxy_stop(void);
void i2pd_https_proxy_stop(void);

int i2pd_router_init(const char* config_dir) {
    std::lock_guard<std::mutex> lock(router_mutex);
    if (router_initialized) {
        return 0; // Already initialized
    }
    
    // Initialize I2P with minimal arguments
    const char* argv[] = {"i2pd", "-datadir", config_dir ? config_dir : "."};
    i2p::api::InitI2P(3, const_cast<char**>(argv), "i2ptunnel");
    router_initialized = true;
    return 0;
}

int i2pd_router_start(void) {
    std::lock_guard<std::mutex> lock(router_mutex);
    if (router_running) {
        return 0; // Already running
    }
    
    if (!router_initialized) {
        i2pd_router_init(nullptr);
    }
    
    i2p::api::StartI2P(nullptr);
    router_running = true;
    return 0;
}

int i2pd_router_stop(void) {
    std::lock_guard<std::mutex> lock(router_mutex);
    if (!router_running) {
        return 0;
    }
    
    // Stop HTTP proxies first
    i2pd_http_proxy_stop();
    i2pd_https_proxy_stop();
    
    i2p::api::StopI2P();
    router_running = false;
    return 0;
}

void i2pd_router_cleanup(void) {
    std::lock_guard<std::mutex> lock(router_mutex);
    if (router_running) {
        i2pd_router_stop();
    }
    if (router_initialized) {
        i2p::api::TerminateI2P();
        router_initialized = false;
    }
}

int i2pd_http_proxy_start(const char* address, uint16_t port) {
    std::lock_guard<std::mutex> lock(router_mutex);
    if (!router_running) {
        return -1; // Router must be running
    }
    
    if (http_proxy) {
        return 0; // Already started
    }
    
    try {
        auto dest = i2p::api::CreateLocalDestination(false);
        http_proxy = std::make_shared<i2p::proxy::HTTPProxy>(
            "http", 
            address ? address : "127.0.0.1", 
            port ? port : 4444,
            dest
        );
        // Note: HTTPProxy needs to be added to ClientContext, but for now we'll keep it simple
        return 0;
    } catch (...) {
        return -1;
    }
}

int i2pd_https_proxy_start(const char* address, uint16_t port) {
    std::lock_guard<std::mutex> lock(router_mutex);
    if (!router_running) {
        return -1; // Router must be running
    }
    
    if (https_proxy) {
        return 0; // Already started
    }
    
    try {
        auto dest = i2p::api::CreateLocalDestination(false);
        https_proxy = std::make_shared<i2p::proxy::HTTPProxy>(
            "https", 
            address ? address : "127.0.0.1", 
            port ? port : 4447,
            dest
        );
        return 0;
    } catch (...) {
        return -1;
    }
}

void i2pd_http_proxy_stop(void) {
    std::lock_guard<std::mutex> lock(router_mutex);
    http_proxy.reset();
}

void i2pd_https_proxy_stop(void) {
    std::lock_guard<std::mutex> lock(router_mutex);
    https_proxy.reset();
}

int i2pd_router_is_running(void) {
    std::lock_guard<std::mutex> lock(router_mutex);
    return router_running ? 1 : 0;
}

} // extern "C"

