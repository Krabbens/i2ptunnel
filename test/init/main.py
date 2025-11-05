from i2ptunnel import I2PProxyDaemon

daemon = I2PProxyDaemon()

# Fetch available proxies
proxies = daemon.fetch_proxies()
print(f"Found {len(proxies)} proxies")

# Test proxies
results = daemon.test_proxies(proxies[:5])  # Test first 5
for result in results:
    if result["success"]:
        print(f"Proxy {result['proxy']}: {result['speed_bytes_per_sec']/1024:.2f} KB/s")

# Make a request through the fastest proxy
response = daemon.make_request(
    url="https://example.com",
    method="GET",
    headers=None,
    body=None
)

print(f"Status: {response['status']}")
print(f"Proxy used: {response['proxy_used']}")
print(f"Body: {response['body']}")