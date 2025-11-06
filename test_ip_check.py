"""Test IP address before and after going through proxy"""
import sys
import json
import requests
from i2p_proxy import I2PProxy
from i2ptunnel import I2PProxyDaemon


def get_ip_without_proxy():
    """Get IP address without using any proxy (direct connection)"""
    print("\n[*] Checking IP address WITHOUT proxy (direct connection)...")
    try:
        # Try multiple IP checking services
        services = [
            "https://api.ipify.org?format=json",
            "https://api.myip.com",
            "https://ifconfig.me/all.json",
        ]
        
        for service in services:
            try:
                response = requests.get(service, timeout=10)
                if response.status_code == 200:
                    data = response.json()
                    if 'ip' in data:
                        ip = data['ip']
                    elif 'query' in data:
                        ip = data['query']
                    else:
                        ip = list(data.values())[0] if data else None
                    
                    if ip:
                        print(f"   ✓ Direct IP: {ip}")
                        print(f"   Service: {service}")
                        return ip
            except Exception as e:
                continue
        
        # Fallback: try simple text response
        try:
            response = requests.get("https://api.ipify.org", timeout=10)
            if response.status_code == 200:
                ip = response.text.strip()
                print(f"   ✓ Direct IP: {ip}")
                return ip
        except:
            pass
            
        print("   ✗ Failed to get IP without proxy")
        return None
    except Exception as e:
        print(f"   ✗ Error getting IP without proxy: {e}")
        return None


def get_ip_with_proxy(proxy_instance=None):
    """Get IP address through the I2P proxy"""
    print("\n[*] Checking IP address THROUGH proxy...")
    
    if proxy_instance is None:
        proxy_instance = I2PProxy()
    
    try:
        # Try multiple IP checking services
        services = [
            "https://api.ipify.org?format=json",
            "https://api.myip.com",
            "https://ifconfig.me/all.json",
        ]
        
        for service in services:
            try:
                response = proxy_instance.get(service)
                if response.status_code == 200:
                    data = response.json()
                    if 'ip' in data:
                        ip = data['ip']
                    elif 'query' in data:
                        ip = data['query']
                    else:
                        ip = list(data.values())[0] if data else None
                    
                    if ip:
                        print(f"   ✓ Proxy IP: {ip}")
                        print(f"   Service: {service}")
                        return ip
            except Exception as e:
                print(f"   Service {service} failed: {e}")
                continue
        
        # Fallback: try simple text response
        try:
            response = proxy_instance.get("https://api.ipify.org")
            if response.status_code == 200:
                ip = response.text.strip()
                print(f"   ✓ Proxy IP: {ip}")
                return ip
        except Exception as e:
            print(f"   Fallback service failed: {e}")
            
        print("   ✗ Failed to get IP through proxy")
        return None
    except Exception as e:
        print(f"   ✗ Error getting IP through proxy: {e}")
        return None


def get_ip_with_specific_proxy(proxy_url):
    """Get IP address through a specific proxy"""
    print(f"\n[*] Checking IP address through specific proxy: {proxy_url}")
    
    daemon = I2PProxyDaemon()
    
    try:
        # Try multiple IP checking services
        services = [
            ("https://api.ipify.org?format=json", 'ip'),
            ("https://api.myip.com", 'ip'),
            ("https://ifconfig.me/all.json", 'ip_addr'),
        ]
        
        for service_url, ip_key in services:
            try:
                # Make request using specific proxy
                response = daemon.make_request_with_proxy(
                    url=service_url,
                    proxy_url=proxy_url,
                    method="GET"
                )
                
                if response["status"] == 200:
                    # Parse JSON response
                    body_bytes = response["body"]
                    if isinstance(body_bytes, bytes):
                        data = json.loads(body_bytes.decode('utf-8'))
                    else:
                        data = json.loads(str(body_bytes))
                    
                    ip = data.get(ip_key) or data.get('ip') or data.get('query')
                    if ip:
                        print(f"   ✓ Proxy IP: {ip}")
                        print(f"   Service: {service_url}")
                        return ip
            except Exception as e:
                continue
        
        # Fallback: try simple text response
        try:
            response = daemon.make_request_with_proxy(
                url="https://api.ipify.org",
                proxy_url=proxy_url,
                method="GET"
            )
            
            if response["status"] == 200:
                body_bytes = response["body"]
                if isinstance(body_bytes, bytes):
                    ip = body_bytes.decode('utf-8').strip()
                else:
                    ip = str(body_bytes).strip()
                
                if ip:
                    print(f"   ✓ Proxy IP: {ip}")
                    return ip
        except Exception as e:
            pass
            
        print("   ✗ Failed to get IP through specific proxy")
        return None
    except Exception as e:
        print(f"   ✗ Error getting IP through specific proxy: {e}")
        import traceback
        traceback.print_exc()
        return None


def test_all_proxies():
    """Test IP through each available proxy"""
    print("\n" + "="*60)
    print("Testing IP through each available proxy")
    print("="*60)
    
    daemon = I2PProxyDaemon()
    available_proxies = daemon.fetch_proxies()
    
    if not available_proxies:
        print("[!] No proxies available")
        return
    
    print(f"\n[*] Found {len(available_proxies)} available proxies\n")
    
    direct_ip = get_ip_without_proxy()
    
    print("\n[*] Testing each proxy:")
    for i, proxy_url in enumerate(available_proxies):
        print(f"\n--- Proxy {i+1}: {proxy_url} ---")
        proxy_ip = get_ip_with_specific_proxy(proxy_url)
        if proxy_ip and direct_ip:
            if proxy_ip != direct_ip:
                print(f"   ✓ IP changed: {direct_ip} -> {proxy_ip}")
            else:
                print(f"   ⚠ IP unchanged: {proxy_ip} (proxy may not be working)")


def main():
    """Main test function"""
    print("\n" + "="*60)
    print("IP Address Test - Before and After Proxy")
    print("="*60)
    
    # Test 1: Direct IP
    direct_ip = get_ip_without_proxy()
    
    # Test 2: IP through proxy (auto-selected)
    proxy_ip = get_ip_with_proxy()
    
    # Test 3: Compare results
    print("\n" + "="*60)
    print("Comparison Results")
    print("="*60)
    
    if direct_ip and proxy_ip:
        print(f"\nDirect IP (no proxy):  {direct_ip}")
        print(f"Proxy IP:              {proxy_ip}")
        
        if direct_ip != proxy_ip:
            print(f"\n✓ SUCCESS: IP addresses are different!")
            print(f"  The proxy is working correctly.")
        else:
            print(f"\n⚠ WARNING: IP addresses are the same!")
            print(f"  This may indicate the proxy is not working correctly.")
    elif direct_ip:
        print(f"\nDirect IP (no proxy):  {direct_ip}")
        print(f"Proxy IP:              Failed to retrieve")
        print(f"\n✗ ERROR: Could not get IP through proxy")
    elif proxy_ip:
        print(f"\nDirect IP (no proxy):  Failed to retrieve")
        print(f"Proxy IP:              {proxy_ip}")
        print(f"\n⚠ WARNING: Could not get direct IP")
    else:
        print(f"\n✗ ERROR: Could not get IP addresses from either method")
    
    # Test 4: Test each proxy individually
    if len(sys.argv) > 1 and sys.argv[1] == "--all":
        test_all_proxies()
    
    print("\n" + "="*60 + "\n")


if __name__ == "__main__":
    main()

