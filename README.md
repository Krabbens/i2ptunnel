# I2P Tunnel

A Rust daemon with extensive logging that manages I2P HTTPS proxies, tests them in parallel for download speed, selects the fastest one, and provides a Python decorator interface for automatic request proxying.

## Features

- **Embedded i2pd Router**: Uses i2pd library directly (no external router required)
- Fetches proxy list from `http://outproxys.i2p/`
- Tests proxies in parallel for download speed
- Automatically selects and uses the fastest proxy
- Python decorator `@i2p` for seamless integration
- Comprehensive logging with `tracing`
- Automatic proxy rotation on failure
- Periodic re-testing to maintain optimal performance

## Installation

### Prerequisites

- Rust 1.83 or later (required for ICU dependencies)
- Python 3.8+
- uv (fast Python package installer)
- **System Dependencies** (required for building i2pd):
  - **CMake** (3.7 or later)
  - **C++ Compiler** with C++17 support:
    - Windows: MSVC or MinGW-w64
    - Linux: GCC 8+ or Clang 8+
    - macOS: Clang (Xcode Command Line Tools)
  - **OpenSSL** development libraries
  - **Boost** (1.46 or later) with components:
    - filesystem
    - program_options
  - **zlib** development libraries
  - **pthread** (usually included)

#### Installing System Dependencies

**Windows:**
- Install [vcpkg](https://github.com/Microsoft/vcpkg) and install: `vcpkg install openssl boost zlib`
- Or use pre-built binaries from [OpenSSL](https://slproweb.com/products/Win32OpenSSL.html) and [Boost](https://www.boost.org/users/download/)

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install build-essential cmake libssl-dev libboost-filesystem-dev libboost-program-options-dev zlib1g-dev
```

**Linux (Fedora/RHEL):**
```bash
sudo dnf install gcc-c++ cmake openssl-devel boost-devel zlib-devel
```

**macOS:**
```bash
brew install cmake openssl boost zlib
```

### Initial Setup

First, initialize the i2pd git submodule:
```bash
git submodule update --init --recursive
```

Install uv:
```bash
# On Windows (PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"

# On macOS/Linux
curl -LsSf https://astral.sh/uv/install.sh | sh
```

### Build

Build the Rust library and Python extension using uv:
```bash
# Install maturin using uv
uv tool install maturin

# Build and install in development mode
uv run maturin develop --release

# Or build wheel and install
uv run maturin build --release
uv pip install target/wheels/i2ptunnel-*.whl
```

Or use uv's project management:
```bash
# Sync dependencies and build
uv sync
uv run maturin develop --release
```

## Usage

### Python Decorator

Use the `@i2p` decorator to automatically route HTTP requests through the fastest I2P proxy:

```python
from i2p_proxy import i2p
import requests

@i2p
def fetch_data():
    response = requests.get("https://example.com")
    return response.json()

# All requests.get/post/etc. calls inside this function
# will automatically use the fastest I2P proxy
data = fetch_data()
```

### Direct Usage

You can also use the daemon directly:

```python
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
```

### Using the I2PProxy class

```python
from i2p_proxy import I2PProxy

proxy = I2PProxy()

# Make requests directly
response = proxy.get("https://example.com")
print(response.status_code)
print(response.text)

response = proxy.post("https://example.com/api", data=b"data")
```

## Logging

The daemon uses extensive logging via the `tracing` crate. Set the log level with environment variables:

```bash
# Set log level (TRACE, DEBUG, INFO, WARN, ERROR)
export RUST_LOG=i2ptunnel=debug

# Or more verbose
export RUST_LOG=i2ptunnel=trace
```

## Architecture

- **I2PDRouter**: Embedded i2pd router wrapper that manages the I2P router lifecycle
- **ProxyManager**: Fetches and parses proxy list from `http://outproxys.i2p/`
- **ProxyTester**: Tests proxies in parallel, measuring download speed and latency
- **ProxySelector**: Tracks and selects the fastest proxy, handles failures
- **RequestHandler**: Routes HTTP requests through the selected proxy
- **PyO3 Bridge**: Exposes Rust functionality to Python

## Configuration

The daemon automatically:
- Initializes and starts an embedded i2pd router on first use
- Provides HTTP proxy on port 4444 and HTTPS proxy on port 4447
- Fetches proxies from the I2P outproxy list
- Tests proxies in parallel (up to 10 concurrent tests)
- Selects the fastest proxy based on download speed
- Re-tests proxies every 5 minutes (configurable)
- Rotates to a new proxy on failure

## i2pd Integration

This project integrates i2pd as a git submodule and compiles it as a static library. The i2pd router is embedded directly into the application, eliminating the need for an external I2P router installation.

### Building with i2pd

The build process:
1. Compiles i2pd libraries (libi2pd, libi2pdclient, libi2pdlang) using CMake
2. Compiles a C++ wrapper that exposes HTTP proxy functionality
3. Generates Rust FFI bindings using bindgen
4. Links everything together into the final binary

### Troubleshooting Build Issues

**CMake not found:**
- Install CMake from https://cmake.org/download/

**OpenSSL not found:**
- Ensure OpenSSL development headers are installed
- On Windows, set `OPENSSL_DIR` environment variable to OpenSSL installation

**Boost not found:**
- Ensure Boost development libraries are installed
- On Windows, set `BOOST_ROOT` environment variable

**C++17 not supported:**
- Upgrade your compiler (GCC 8+, Clang 8+, MSVC 2017+)

**Link errors:**
- Ensure all system libraries are in your library path
- On Linux, you may need to install `libssl-dev`, `libboost-all-dev`, `zlib1g-dev`

## Development

Build for development:
```bash
cargo build
```

Run tests:
```bash
cargo test
```

Build Python extension in development mode:
```bash
uv run maturin develop
```

Or using uv sync:
```bash
uv sync
uv run maturin develop
```

## License

MIT License

