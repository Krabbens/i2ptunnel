/* C wrapper for i2pd HTTP proxy functionality */
#ifndef I2PD_WRAPPER_H__
#define I2PD_WRAPPER_H__

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Router lifecycle
int i2pd_router_init(const char* config_dir);
int i2pd_router_start(void);
int i2pd_router_stop(void);
void i2pd_router_cleanup(void);

// HTTP Proxy server management
int i2pd_http_proxy_start(const char* address, uint16_t port);
int i2pd_https_proxy_start(const char* address, uint16_t port);
void i2pd_http_proxy_stop(void);
void i2pd_https_proxy_stop(void);

// Check if router is running
int i2pd_router_is_running(void);

#ifdef __cplusplus
}
#endif

#endif

