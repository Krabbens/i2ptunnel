"""
I2P Proxy Daemon - Python Wrapper

This module provides a Python decorator @i2p that automatically routes
HTTP requests through the fastest I2P proxy.
"""

import functools
import io
from typing import Callable, Any, Optional, Dict, Union
import requests
from i2ptunnel import I2PProxyDaemon


class I2PResponse(requests.Response):
    """Custom Response class that mimics requests.Response"""
    
    def __init__(self, status_code: int, headers: Dict[str, str], body: bytes, url: str):
        super().__init__()
        self.status_code = status_code
        self.url = url
        self.reason = self._get_reason(status_code)
        self.headers = requests.structures.CaseInsensitiveDict(headers)
        self._content = body
        self.raw = io.BytesIO(body)
        self.encoding = 'utf-8'
    
    def _get_reason(self, status_code: int) -> str:
        """Get HTTP reason phrase from status code"""
        reasons = {
            200: "OK",
            201: "Created",
            204: "No Content",
            400: "Bad Request",
            401: "Unauthorized",
            403: "Forbidden",
            404: "Not Found",
            500: "Internal Server Error",
            502: "Bad Gateway",
            503: "Service Unavailable",
        }
        return reasons.get(status_code, "Unknown")
    
    @property
    def text(self) -> str:
        """Get response text"""
        if self._content:
            return self._content.decode(self.encoding or 'utf-8', errors='replace')
        return ""
    
    def json(self, **kwargs):
        """Parse JSON response"""
        import json
        return json.loads(self.text, **kwargs)


class I2PProxy:
    """Wrapper around the Rust I2P proxy daemon for Python"""
    
    def __init__(self):
        """Initialize the I2P proxy daemon"""
        self._daemon = I2PProxyDaemon()
    
    def _make_request(
        self,
        url: str,
        method: str = "GET",
        headers: Optional[Dict[str, str]] = None,
        data: Optional[bytes] = None,
        **kwargs
    ) -> I2PResponse:
        """Make an HTTP request through the I2P proxy daemon"""
        # Convert headers dict to the format expected by Rust
        headers_dict = None
        if headers:
            headers_dict = {k: v for k, v in headers.items()}
        
        # Convert data to bytes if it's a string
        body_bytes = data
        if isinstance(data, str):
            body_bytes = data.encode("utf-8")
        elif data is None and "json" in kwargs:
            import json
            body_bytes = json.dumps(kwargs["json"]).encode("utf-8")
            if headers_dict is None:
                headers_dict = {}
            headers_dict["Content-Type"] = "application/json"
        
        # Call the Rust daemon
        result = self._daemon.make_request(
            url=url,
            method=method,
            headers=headers_dict,
            body=body_bytes
        )
        
        # Create response object
        return I2PResponse(
            status_code=result["status"],
            headers=result["headers"],
            body=bytes(result["body"]),
            url=url
        )
    
    def get(self, url: str, **kwargs) -> I2PResponse:
        """Make a GET request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        return self._make_request(url, "GET", headers, None, **kwargs)
    
    def post(self, url: str, **kwargs) -> I2PResponse:
        """Make a POST request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        if isinstance(data, str):
            data = data.encode("utf-8")
        return self._make_request(url, "POST", headers, data, **kwargs)
    
    def put(self, url: str, **kwargs) -> I2PResponse:
        """Make a PUT request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        if isinstance(data, str):
            data = data.encode("utf-8")
        return self._make_request(url, "PUT", headers, data, **kwargs)
    
    def delete(self, url: str, **kwargs) -> I2PResponse:
        """Make a DELETE request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        return self._make_request(url, "DELETE", headers, None, **kwargs)
    
    def patch(self, url: str, **kwargs) -> I2PResponse:
        """Make a PATCH request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        if isinstance(data, str):
            data = data.encode("utf-8")
        return self._make_request(url, "PATCH", headers, data, **kwargs)
    
    def request(self, method: str, url: str, **kwargs) -> I2PResponse:
        """Make a generic HTTP request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        return self._make_request(url, method.upper(), headers, data, **kwargs)


# Global instance
_i2p_proxy = None


def get_i2p_proxy() -> I2PProxy:
    """Get or create the global I2P proxy instance"""
    global _i2p_proxy
    if _i2p_proxy is None:
        _i2p_proxy = I2PProxy()
    return _i2p_proxy


def i2p(func: Optional[Callable] = None):
    """
    Decorator that automatically routes HTTP requests through I2P proxy.
    
    Usage:
        @i2p
        def fetch_data():
            response = requests.get("https://example.com")
            return response.json()
    
    When used as a decorator, it replaces the requests module's HTTP methods
    with I2P-proxied versions for the duration of the function call.
    """
    def decorator(f: Callable) -> Callable:
        @functools.wraps(f)
        def wrapper(*args, **kwargs):
            # Store original requests module
            import sys
            import importlib
            original_requests = sys.modules.get("requests")
            
            # Create a proxy wrapper for requests module
            class I2PRequestsModule:
                def __init__(self, proxy: I2PProxy, original_module):
                    self._proxy = proxy
                    self._original = original_module
                
                def get(self, url: str, **kwargs):
                    return self._proxy.get(url, **kwargs)
                
                def post(self, url: str, **kwargs):
                    return self._proxy.post(url, **kwargs)
                
                def put(self, url: str, **kwargs):
                    return self._proxy.put(url, **kwargs)
                
                def delete(self, url: str, **kwargs):
                    return self._proxy.delete(url, **kwargs)
                
                def patch(self, url: str, **kwargs):
                    return self._proxy.patch(url, **kwargs)
                
                def request(self, method: str, url: str, **kwargs):
                    return self._proxy.request(method, url, **kwargs)
                
                # Forward other attributes from original requests module
                def __getattr__(self, name):
                    if self._original:
                        return getattr(self._original, name)
                    raise AttributeError(f"module 'requests' has no attribute '{name}'")
            
            # Install the I2P requests module
            i2p_proxy = get_i2p_proxy()
            i2p_requests = I2PRequestsModule(i2p_proxy, original_requests)
            
            # Replace requests module temporarily
            sys.modules["requests"] = i2p_requests
            
            try:
                result = f(*args, **kwargs)
                return result
            finally:
                # Restore original requests module
                if original_requests:
                    sys.modules["requests"] = original_requests
                elif "requests" in sys.modules:
                    # Reload if it was replaced
                    importlib.reload(sys.modules["requests"])
        
        return wrapper
    
    # Support both @i2p and @i2p() syntax
    if func is None:
        return decorator
    else:
        return decorator(func)


# Export the decorator and proxy class
__all__ = ["i2p", "I2PProxy", "get_i2p_proxy", "I2PResponse"]

