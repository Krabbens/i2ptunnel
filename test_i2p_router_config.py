"""Test I2P router configuration and available proxy ports"""
import socket
import sys
from i2ptunnel import I2PProxyDaemon

def test_port(host, port, timeout=2):
    """Test if a port is open and accepting connections"""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(timeout)
        result = sock.connect_ex((host, port))
        sock.close()
        return result == 0
    except Exception as e:
        return False

def check_i2p_router_ports():
    """Check which I2P router proxy ports are available"""
    print("=" * 60)
    print("I2P Router Proxy Port Check")
    print("=" * 60)
    print()
    
    ports_to_check = [
        (4444, "HTTP Proxy"),
        (4445, "HTTP Proxy (alt)"),
        (4446, "SOCKS Proxy (Java I2P)"),
        (4447, "SOCKS Proxy (i2pd) / HTTPS Proxy"),
        (7657, "Router Console (Java I2P)"),
        (7070, "Router Console (i2pd)"),
        (9060, "SOCKS Proxy (alt)"),
    ]
    
    print("Testing router proxy ports on 127.0.0.1:")
    print("-" * 60)
    
    available_ports = []
    for port, description in ports_to_check:
        is_open = test_port("127.0.0.1", port)
        status = "OPEN" if is_open else "CLOSED/TIMEOUT"
        print(f"Port {port:5d} ({description:25s}): {status}")
        if is_open:
            available_ports.append((port, description))
    
    print()
    print("=" * 60)
    print(f"Summary: {len(available_ports)} ports available")
    print("=" * 60)
    
    if available_ports:
        print("\nAvailable ports:")
        for port, desc in available_ports:
            print(f"  - Port {port}: {desc}")
    else:
        print("\n[WARNING] No router proxy ports detected!")
        print("Make sure the I2P router is running.")
    
    return available_ports

def test_socks_connection(port):
    """Try to make a simple SOCKS connection test"""
    print(f"\nTesting SOCKS5 connection on port {port}...")
    try:
        # Try to connect and send SOCKS5 greeting
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(3)
        sock.connect(("127.0.0.1", port))
        
        # Send SOCKS5 greeting: version 5, 1 auth method (no auth)
        greeting = bytes([0x05, 0x01, 0x00])
        sock.send(greeting)
        
        # Read response
        response = sock.recv(2)
        sock.close()
        
        if len(response) == 2 and response[0] == 0x05:
            print(f"  [OK] SOCKS5 proxy responding on port {port}")
            return True
        else:
            print(f"  [FAIL] Invalid SOCKS5 response on port {port}")
            return False
    except Exception as e:
        print(f"  [FAIL] SOCKS5 connection failed on port {port}: {e}")
        return False

def check_router_console():
    """Check if router console is accessible"""
    print("\n" + "=" * 60)
    print("I2P Router Console Check")
    print("=" * 60)
    
    # Check for both Java I2P and i2pd console ports
    java_console = 7657
    i2pd_console = 7070
    
    if test_port("127.0.0.1", java_console):
        print(f"[OK] Java I2P router console is accessible on port {java_console}")
        print(f"     You can access it at: http://127.0.0.1:{java_console}")
        print("\nTo configure SOCKS proxy in Java I2P:")
        print("  1. Go to http://127.0.0.1:7657")
        print("  2. Navigate to 'I2PTunnel' or 'Clients'")
        print("  3. Look for 'SOCKS Proxy' or 'Client Tunnels'")
        print("  4. Enable SOCKS proxy on port 4446 or 9060")
        return True
    elif test_port("127.0.0.1", i2pd_console):
        print(f"[OK] i2pd router console is accessible on port {i2pd_console}")
        print(f"     You can access it at: http://127.0.0.1:{i2pd_console}")
        print("\nFor i2pd:")
        print("  - Default SOCKS proxy port: 4447")
        print("  - Default HTTP proxy port: 4444")
        print("  - Configure in i2pd.conf or enable via web console")
        return True
    else:
        print(f"[FAIL] Router console not accessible on ports {java_console} (Java I2P) or {i2pd_console} (i2pd)")
        print("       Make sure I2P router is running")
        return False

if __name__ == "__main__":
    print("I2P Router Configuration Diagnostic")
    print("=" * 60)
    print()
    
    # Check available ports
    available_ports = check_i2p_router_ports()
    
    # Test SOCKS ports specifically (i2pd uses 4447, Java I2P uses 4446/9060)
    socks_ports = [4447, 4446, 9060]
    print("\n" + "=" * 60)
    print("SOCKS Proxy Detailed Test")
    print("=" * 60)
    
    for port in socks_ports:
        if test_port("127.0.0.1", port):
            test_socks_connection(port)
        else:
            print(f"Port {port}: Not accessible (connection timeout)")
    
    # Check router console
    check_router_console()
    
    print("\n" + "=" * 60)
    print("Recommendations")
    print("=" * 60)
    
    socks_ports_checked = [4447, 4446, 9060]
    if not any(port in [p[0] for p in available_ports] for port in socks_ports_checked):
        print("\n[IMPORTANT] SOCKS proxy is not enabled!")
        print("For true parallel downloads, you need to:")
        print("  - i2pd: SOCKS proxy is typically on port 4447 (check i2pd.conf)")
        print("  - Java I2P: Enable SOCKS proxy on port 4446 or 9060")
        print("  - SOCKS allows multiple independent connections per proxy")
        print("\nCurrent limitation: All chunks use same HTTP proxy (port 4444)")
        print("This creates a bottleneck - all traffic goes through one connection")
    else:
        print("\n[OK] SOCKS proxy is available!")
        print("The code should automatically use it for better parallelism")
    
    sys.exit(0)

