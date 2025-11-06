"""
I2P Proxy Daemon - Python Wrapper

This module provides a Python decorator @i2p that automatically routes
HTTP requests through the fastest I2P proxy.
"""

import functools
import io
import threading
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


class I2PStreamingResponse(requests.Response):
    """Streaming Response class that mimics requests.Response with streaming support"""
    
    def __init__(self, status_code: int, headers: Dict[str, str], url: str, daemon, request_info: Dict):
        # Don't call super().__init__() as it tries to set raw=None
        # Instead, set attributes manually
        self.status_code = status_code
        self.url = url
        self.reason = self._get_reason(status_code)
        self.headers = requests.structures.CaseInsensitiveDict(headers)
        self.encoding = 'utf-8'
        self._content = None  # Will be None for streaming
        self._content_consumed = False
        self._daemon = daemon
        self._request_info = request_info
        self._stream_started = False
        self._chunk_iterator = None
        self._raw = None  # Store raw stream reference
    
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
    def raw(self):
        """Return a raw stream-like object"""
        # For streaming, we'll create a custom stream
        return self
    
    def iter_content(self, chunk_size: int = 8192):
        """
        Iterate over response content in chunks.
        
        Args:
            chunk_size: Size of each chunk in bytes (default: 8192)
        
        Yields:
            bytes: Chunks of response content
        """
        if not self._stream_started:
            # Make the streaming request
            result = self._daemon.make_request_streaming(
                url=self._request_info["url"],
                method=self._request_info["method"],
                headers=self._request_info.get("headers"),
                body=self._request_info.get("body"),
                chunk_size=chunk_size
            )
            self._chunk_iterator = iter(result["chunks"])
            self._stream_started = True
        
        yield from self._chunk_iterator
    
    def iter_lines(self, chunk_size: int = 8192, decode_unicode: bool = False):
        """
        Iterate over response content, line by line.
        
        Args:
            chunk_size: Size of each chunk in bytes (default: 8192)
            decode_unicode: Whether to decode unicode (default: False)
        
        Yields:
            str or bytes: Lines of response content
        """
        pending = b""
        for chunk in self.iter_content(chunk_size=chunk_size):
            if chunk:
                pending += chunk
                while b"\n" in pending:
                    line, pending = pending.split(b"\n", 1)
                    if decode_unicode:
                        yield line.decode(self.encoding or 'utf-8', errors='replace')
                    else:
                        yield line
        # Yield remaining data
        if pending:
            if decode_unicode:
                yield pending.decode(self.encoding or 'utf-8', errors='replace')
            else:
                yield pending
    
    def read(self, size: int = -1) -> bytes:
        """
        Read content from the stream.
        
        Args:
            size: Number of bytes to read (-1 for all remaining)
        
        Returns:
            bytes: Read content
        """
        if size == -1:
            # Read all remaining content
            return b"".join(self.iter_content())
        else:
            # Read up to size bytes
            chunks = []
            remaining = size
            for chunk in self.iter_content(chunk_size=min(8192, remaining)):
                if len(chunk) <= remaining:
                    chunks.append(chunk)
                    remaining -= len(chunk)
                    if remaining == 0:
                        break
                else:
                    chunks.append(chunk[:remaining])
                    break
            return b"".join(chunks)


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
    ):
        """Make an HTTP request through the I2P proxy daemon"""
        # Check if streaming is requested
        stream = kwargs.pop("stream", False)
        
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
            body=body_bytes,
            stream=stream
        )
        
        # If streaming, return a streaming response
        if stream:
            return I2PStreamingResponse(
                status_code=result["status"],
                headers=result["headers"],
                url=url,
                daemon=self._daemon,
                request_info={
                    "url": url,
                    "method": method,
                    "headers": headers_dict,
                    "body": body_bytes
                }
            )
        
        # Create regular response object
        return I2PResponse(
            status_code=result["status"],
            headers=result["headers"],
            body=bytes(result["body"]),
            url=url
        )
    
    def get(self, url: str, **kwargs):
        """Make a GET request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        return self._make_request(url, "GET", headers, None, **kwargs)
    
    def post(self, url: str, **kwargs):
        """Make a POST request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        if isinstance(data, str):
            data = data.encode("utf-8")
        return self._make_request(url, "POST", headers, data, **kwargs)
    
    def put(self, url: str, **kwargs):
        """Make a PUT request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        if isinstance(data, str):
            data = data.encode("utf-8")
        return self._make_request(url, "PUT", headers, data, **kwargs)
    
    def delete(self, url: str, **kwargs):
        """Make a DELETE request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        return self._make_request(url, "DELETE", headers, None, **kwargs)
    
    def patch(self, url: str, **kwargs):
        """Make a PATCH request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        if isinstance(data, str):
            data = data.encode("utf-8")
        return self._make_request(url, "PATCH", headers, data, **kwargs)
    
    def request(self, method: str, url: str, **kwargs):
        """Make a generic HTTP request through I2P proxy"""
        headers = kwargs.pop("headers", None)
        data = kwargs.pop("data", None)
        return self._make_request(url, method.upper(), headers, data, **kwargs)


# Global instance
_i2p_proxy = None
_i2p_proxy_lock = threading.Lock()

# Thread-local storage for tracking active @i2p decorators
_thread_local = threading.local()


def get_i2p_proxy() -> I2PProxy:
    """Get or create the global I2P proxy instance (thread-safe)"""
    global _i2p_proxy
    if _i2p_proxy is None:
        with _i2p_proxy_lock:
            # Double-check pattern
            if _i2p_proxy is None:
                _i2p_proxy = I2PProxy()
    return _i2p_proxy


def _get_thread_local_requests():
    """Get the thread-local requests module wrapper"""
    if not hasattr(_thread_local, 'i2p_requests'):
        return None
    return getattr(_thread_local, 'i2p_requests')


def _set_thread_local_requests(i2p_requests):
    """Set the thread-local requests module wrapper"""
    _thread_local.i2p_requests = i2p_requests


def _clear_thread_local_requests():
    """Clear the thread-local requests module wrapper"""
    if hasattr(_thread_local, 'i2p_requests'):
        delattr(_thread_local, 'i2p_requests')


# Thread-safe requests module wrapper
class ThreadSafeI2PRequestsModule:
    """Thread-safe wrapper that uses thread-local storage for I2P proxying"""
    
    def __init__(self, original_module):
        self._original = original_module
        self._i2p_proxy = None
    
    def _get_proxy(self):
        """Get the I2P proxy instance (lazy initialization)"""
        if self._i2p_proxy is None:
            self._i2p_proxy = get_i2p_proxy()
        return self._i2p_proxy
    
    def get(self, url: str, **kwargs):
        """Make a GET request - use I2P if in an @i2p decorated function, else use original"""
        thread_local_i2p = _get_thread_local_requests()
        if thread_local_i2p:
            return thread_local_i2p.get(url, **kwargs)
        return self._original.get(url, **kwargs)
    
    def post(self, url: str, **kwargs):
        """Make a POST request - use I2P if in an @i2p decorated function, else use original"""
        thread_local_i2p = _get_thread_local_requests()
        if thread_local_i2p:
            return thread_local_i2p.post(url, **kwargs)
        return self._original.post(url, **kwargs)
    
    def put(self, url: str, **kwargs):
        """Make a PUT request - use I2P if in an @i2p decorated function, else use original"""
        thread_local_i2p = _get_thread_local_requests()
        if thread_local_i2p:
            return thread_local_i2p.put(url, **kwargs)
        return self._original.put(url, **kwargs)
    
    def delete(self, url: str, **kwargs):
        """Make a DELETE request - use I2P if in an @i2p decorated function, else use original"""
        thread_local_i2p = _get_thread_local_requests()
        if thread_local_i2p:
            return thread_local_i2p.delete(url, **kwargs)
        return self._original.delete(url, **kwargs)
    
    def patch(self, url: str, **kwargs):
        """Make a PATCH request - use I2P if in an @i2p decorated function, else use original"""
        thread_local_i2p = _get_thread_local_requests()
        if thread_local_i2p:
            return thread_local_i2p.patch(url, **kwargs)
        return self._original.patch(url, **kwargs)
    
    def request(self, method: str, url: str, **kwargs):
        """Make a generic request - use I2P if in an @i2p decorated function, else use original"""
        thread_local_i2p = _get_thread_local_requests()
        if thread_local_i2p:
            return thread_local_i2p.request(method, url, **kwargs)
        return self._original.request(method, url, **kwargs)
    
    # Forward other attributes from original requests module
    def __getattr__(self, name):
        return getattr(self._original, name)


# Initialize thread-safe requests wrapper (only once)
_thread_safe_requests = None
_thread_safe_requests_lock = threading.Lock()


def _get_thread_safe_requests_module():
    """Get or create the thread-safe requests module wrapper"""
    global _thread_safe_requests
    import sys
    if _thread_safe_requests is None:
        with _thread_safe_requests_lock:
            if _thread_safe_requests is None:
                # Get the original requests module
                original_requests = sys.modules.get('requests')
                if original_requests is None:
                    # Import requests if not already imported
                    try:
                        import requests
                        original_requests = requests
                    except ImportError:
                        original_requests = __import__('requests')
                _thread_safe_requests = ThreadSafeI2PRequestsModule(original_requests)
                # Install it in sys.modules
                sys.modules["requests"] = _thread_safe_requests
    return _thread_safe_requests


def i2p(func: Optional[Callable] = None):
    """
    Thread-safe decorator that automatically routes HTTP requests through I2P proxy.
    
    Usage:
        @i2p
        def fetch_data():
            response = requests.get("https://example.com")
            return response.json()
    
    When used as a decorator, it routes requests through I2P proxy using thread-local
    storage, making it safe for use in multi-threaded environments.
    """
    def decorator(f: Callable) -> Callable:
        @functools.wraps(f)
        def wrapper(*args, **kwargs):
            import sys
            import importlib
            
            # Ensure requests module is imported
            if 'requests' not in sys.modules:
                importlib.import_module('requests')
            
            # Get original requests module (before any wrapper)
            original_requests = sys.modules.get("requests")
            # If it's already our wrapper, get the original from it
            if isinstance(original_requests, ThreadSafeI2PRequestsModule):
                original_requests = original_requests._original
            
            # Create I2P proxy wrapper for this thread
            i2p_proxy = get_i2p_proxy()
            
            class I2PRequestsModule:
                """I2P proxied requests module for this thread"""
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
            
            # Create thread-local I2P requests module
            i2p_requests = I2PRequestsModule(i2p_proxy, original_requests)
            
            # Store in thread-local storage
            _set_thread_local_requests(i2p_requests)
            
            # Ensure thread-safe requests module wrapper is installed
            thread_safe_requests = _get_thread_safe_requests_module()
            sys.modules["requests"] = thread_safe_requests
            
            # Also update the module in the function's globals if possible
            # This ensures that any references to 'requests' in the function use our wrapper
            try:
                # Get the function's module
                func_module = sys.modules.get(f.__module__)
                if func_module:
                    # Update the requests reference in the function's module
                    setattr(func_module, 'requests', thread_safe_requests)
            except Exception:
                pass  # Ignore errors, sys.modules replacement should be enough
            
            try:
                result = f(*args, **kwargs)
                return result
            finally:
                # Clear thread-local storage
                _clear_thread_local_requests()
        
        return wrapper
    
    # Support both @i2p and @i2p() syntax
    if func is None:
        return decorator
    else:
        return decorator(func)


# Export the decorator and proxy class
__all__ = ["i2p", "I2PProxy", "get_i2p_proxy", "I2PResponse", "I2PStreamingResponse"]

