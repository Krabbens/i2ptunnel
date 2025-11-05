from i2ptunnel import I2PProxyDaemon
from i2p_proxy import i2p
import requests

# Test it
daemon = I2PProxyDaemon()
proxies = daemon.fetch_proxies()
print(f"Found {len(proxies)} proxies")

# Or use the decorator
@i2p
def fetch_data():
    response = requests.get("https://example.com")
    return response.text

print(fetch_data())