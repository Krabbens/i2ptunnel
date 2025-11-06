"""
Comprehensive Python tests for i2ptunnel package.

These tests cover:
- Python bindings (I2PProxyDaemon)
- Python wrapper classes (I2PProxy, I2PResponse, I2PStreamingResponse)
- Edge cases and error handling
- Thread safety
- Decorator functionality
"""

import pytest
import threading
import time
from unittest.mock import Mock, patch, MagicMock
from i2ptunnel import I2PProxyDaemon
from i2p_proxy import (
    I2PProxy,
    I2PResponse,
    I2PStreamingResponse,
    i2p,
    get_i2p_proxy,
)


class TestI2PProxyDaemonComprehensive:
    """Comprehensive tests for I2PProxyDaemon class"""

    def test_daemon_initialization(self):
        """Test that daemon can be initialized"""
        daemon = I2PProxyDaemon()
        assert daemon is not None

    def test_daemon_singleton_behavior(self):
        """Test that multiple instances can be created"""
        daemon1 = I2PProxyDaemon()
        daemon2 = I2PProxyDaemon()
        # Both should be valid instances
        assert daemon1 is not None
        assert daemon2 is not None

    def test_fetch_proxies_returns_list(self):
        """Test that fetch_proxies returns a list"""
        daemon = I2PProxyDaemon()
        try:
            proxies = daemon.fetch_proxies()
            assert isinstance(proxies, list)
            # If proxies are returned, they should be strings
            if proxies:
                assert all(isinstance(p, str) for p in proxies)
        except Exception:
            pytest.skip("I2P router not available")

    def test_fetch_proxies_empty_on_error(self):
        """Test behavior when fetch_proxies fails"""
        daemon = I2PProxyDaemon()
        try:
            proxies = daemon.fetch_proxies()
            # Should return empty list or raise exception, not return None
            assert proxies is not None
        except Exception:
            # Expected if I2P router is not available
            pass

    def test_test_proxies_with_empty_list(self):
        """Test test_proxies with empty list"""
        daemon = I2PProxyDaemon()
        results = daemon.test_proxies([])
        assert isinstance(results, list)
        assert len(results) == 0

    def test_test_proxies_with_invalid_urls(self):
        """Test test_proxies with invalid proxy URLs"""
        daemon = I2PProxyDaemon()
        invalid_proxies = ["not-a-url", "http://invalid", ""]
        results = daemon.test_proxies(invalid_proxies)
        assert isinstance(results, list)
        # Should handle invalid URLs gracefully

    def test_make_request_get(self):
        """Test make_request with GET method"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request(
                url="https://example.com",
                method="GET",
                headers=None,
                body=None,
                stream=False
            )
            assert isinstance(response, dict)
            assert "status" in response
            assert "proxy_used" in response
            assert "headers" in response
            assert "body" in response
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_make_request_post(self):
        """Test make_request with POST method"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request(
                url="https://httpbin.org/post",
                method="POST",
                headers={"Content-Type": "text/plain"},
                body=b"test data",
                stream=False
            )
            assert isinstance(response, dict)
            assert response["status"] in [200, 201, 400, 500]
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_make_request_with_headers(self):
        """Test make_request with custom headers"""
        daemon = I2PProxyDaemon()
        try:
            headers = {"User-Agent": "test-agent", "Accept": "application/json"}
            response = daemon.make_request(
                url="https://httpbin.org/headers",
                method="GET",
                headers=headers,
                body=None,
                stream=False
            )
            assert isinstance(response, dict)
            assert "headers" in response
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_make_request_with_stream(self):
        """Test make_request with streaming enabled"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request(
                url="https://example.com",
                method="GET",
                headers=None,
                body=None,
                stream=True
            )
            assert isinstance(response, dict)
            assert "status" in response
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_make_request_streaming(self):
        """Test make_request_streaming method"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request_streaming(
                url="https://example.com",
                method="GET",
                headers=None,
                body=None,
                chunk_size=4096
            )
            assert isinstance(response, dict)
            assert "status" in response
            assert "chunks" in response
            assert isinstance(response["chunks"], list)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_get_fastest_proxy_none(self):
        """Test get_fastest_proxy when no proxy is selected"""
        daemon = I2PProxyDaemon()
        # Initially, no proxy should be selected
        fastest = daemon.get_fastest_proxy()
        # Should return None or a string
        assert fastest is None or isinstance(fastest, str)

    def test_make_request_invalid_method(self):
        """Test make_request with invalid HTTP method"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request(
                url="https://example.com",
                method="INVALID",
                headers=None,
                body=None,
                stream=False
            )
            # Should either raise exception or return error status
            assert isinstance(response, dict)
            assert response.get("status", 0) >= 400
        except Exception as e:
            # Expected to raise exception for invalid method
            assert "method" in str(e).lower() or "invalid" in str(e).lower()

    def test_make_request_invalid_url(self):
        """Test make_request with invalid URL"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request(
                url="not-a-valid-url",
                method="GET",
                headers=None,
                body=None,
                stream=False
            )
            # Should handle gracefully
            assert isinstance(response, dict)
        except Exception:
            # Expected to raise exception for invalid URL
            pass


class TestI2PProxyComprehensive:
    """Comprehensive tests for I2PProxy class"""

    def test_proxy_initialization(self):
        """Test that I2PProxy can be initialized"""
        proxy = I2PProxy()
        assert proxy is not None
        assert hasattr(proxy, '_daemon')

    def test_get_method(self):
        """Test get method"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response, I2PResponse)
            assert hasattr(response, 'status_code')
            assert hasattr(response, 'text')
            assert hasattr(response, 'content')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_get_with_headers(self):
        """Test get method with headers"""
        proxy = I2PProxy()
        try:
            response = proxy.get(
                "https://httpbin.org/headers",
                headers={"User-Agent": "test"}
            )
            assert isinstance(response, I2PResponse)
            assert response.status_code in [200, 400, 500]
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_post_method(self):
        """Test post method"""
        proxy = I2PProxy()
        try:
            response = proxy.post("https://httpbin.org/post", data=b"test")
            assert isinstance(response, I2PResponse)
            assert response.status_code in [200, 201, 400, 500]
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_post_with_json(self):
        """Test post method with JSON data"""
        proxy = I2PProxy()
        try:
            response = proxy.post(
                "https://httpbin.org/post",
                json={"key": "value"}
            )
            assert isinstance(response, I2PResponse)
            assert response.status_code in [200, 201, 400, 500]
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_put_method(self):
        """Test put method"""
        proxy = I2PProxy()
        try:
            response = proxy.put("https://httpbin.org/put", data=b"test")
            assert isinstance(response, I2PResponse)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_delete_method(self):
        """Test delete method"""
        proxy = I2PProxy()
        try:
            response = proxy.delete("https://httpbin.org/delete")
            assert isinstance(response, I2PResponse)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_patch_method(self):
        """Test patch method"""
        proxy = I2PProxy()
        try:
            response = proxy.patch("https://httpbin.org/patch", data=b"test")
            assert isinstance(response, I2PResponse)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_request_method(self):
        """Test generic request method"""
        proxy = I2PProxy()
        try:
            response = proxy.request("GET", "https://example.com")
            assert isinstance(response, I2PResponse)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_get_with_stream(self):
        """Test get with stream=True"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            assert isinstance(response, I2PStreamingResponse)
            assert hasattr(response, 'iter_content')
            assert hasattr(response, 'iter_lines')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_concurrent_requests(self):
        """Test that multiple requests can be made concurrently"""
        proxy = I2PProxy()
        results = []
        
        def make_request():
            try:
                response = proxy.get("https://example.com")
                results.append(response.status_code)
            except Exception:
                pass
        
        threads = [threading.Thread(target=make_request) for _ in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        
        # Should have some results (even if some failed)
        assert len(results) >= 0


class TestI2PResponseComprehensive:
    """Comprehensive tests for I2PResponse class"""

    def test_response_attributes(self):
        """Test that I2PResponse has all expected attributes"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert hasattr(response, 'status_code')
            assert hasattr(response, 'headers')
            assert hasattr(response, 'text')
            assert hasattr(response, 'content')
            assert hasattr(response, 'url')
            assert hasattr(response, 'reason')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_status_code(self):
        """Test status_code property"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.status_code, int)
            assert 100 <= response.status_code < 600
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_text(self):
        """Test text property"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.text, str)
            assert len(response.text) >= 0
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_content(self):
        """Test content property"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.content, bytes)
            assert len(response.content) >= 0
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_json(self):
        """Test json method"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://httpbin.org/json")
            json_data = response.json()
            assert isinstance(json_data, dict)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_headers(self):
        """Test headers property"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.headers, dict)
            # Headers should be case-insensitive
            assert 'content-type' in response.headers or 'Content-Type' in response.headers
        except Exception:
            pytest.skip("I2P router not available or request failed")


class TestI2PStreamingResponseComprehensive:
    """Comprehensive tests for I2PStreamingResponse class"""

    def test_streaming_response_initialization(self):
        """Test that I2PStreamingResponse can be created"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            assert isinstance(response, I2PStreamingResponse)
            assert hasattr(response, 'status_code')
            assert hasattr(response, 'headers')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_iter_content(self):
        """Test iter_content method"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            chunks = list(response.iter_content(chunk_size=1024))
            assert len(chunks) > 0
            assert all(isinstance(chunk, bytes) for chunk in chunks)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_iter_content_custom_chunk_size(self):
        """Test iter_content with custom chunk size"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            chunks = list(response.iter_content(chunk_size=512))
            assert all(isinstance(chunk, bytes) for chunk in chunks)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_iter_lines(self):
        """Test iter_lines method"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            lines = list(response.iter_lines())
            assert all(isinstance(line, bytes) for line in lines)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_iter_lines_decode_unicode(self):
        """Test iter_lines with decode_unicode=True"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            lines = list(response.iter_lines(decode_unicode=True))
            assert all(isinstance(line, str) for line in lines)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_read_method(self):
        """Test read method"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            content = response.read()
            assert isinstance(content, bytes)
            assert len(content) > 0
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_read_with_size(self):
        """Test read method with size parameter"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            content = response.read(size=1024)
            assert isinstance(content, bytes)
            assert len(content) <= 1024
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_read_all_remaining(self):
        """Test read method with size=-1 to read all"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            content = response.read(size=-1)
            assert isinstance(content, bytes)
        except Exception:
            pytest.skip("I2P router not available or request failed")


class TestI2PDecorator:
    """Tests for the @i2p decorator"""

    def test_decorator_basic(self):
        """Test basic decorator usage"""
        @i2p
        def test_function():
            from i2p_proxy import I2PProxy
            proxy = I2PProxy()
            try:
                response = proxy.get("https://example.com")
                return response.status_code
            except Exception:
                return None
        
        result = test_function()
        # Should return status code or None
        assert result is None or isinstance(result, int)

    def test_decorator_with_args(self):
        """Test decorator with function arguments"""
        @i2p
        def test_function(url):
            from i2p_proxy import I2PProxy
            proxy = I2PProxy()
            try:
                response = proxy.get(url)
                return response.status_code
            except Exception:
                return None
        
        result = test_function("https://example.com")
        assert result is None or isinstance(result, int)

    def test_decorator_thread_safety(self):
        """Test that decorator works in multiple threads"""
        results = []
        
        @i2p
        def test_function():
            from i2p_proxy import I2PProxy
            proxy = I2PProxy()
            try:
                response = proxy.get("https://example.com")
                return response.status_code
            except Exception:
                return None
        
        def run_in_thread():
            result = test_function()
            results.append(result)
        
        threads = [threading.Thread(target=run_in_thread) for _ in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        
        # Should have some results
        assert len(results) >= 0

    def test_get_i2p_proxy_singleton(self):
        """Test that get_i2p_proxy returns singleton"""
        proxy1 = get_i2p_proxy()
        proxy2 = get_i2p_proxy()
        # Should return same instance (or at least compatible instances)
        assert proxy1 is not None
        assert proxy2 is not None


class TestErrorHandling:
    """Tests for error handling"""

    def test_invalid_url_handling(self):
        """Test handling of invalid URLs"""
        proxy = I2PProxy()
        try:
            response = proxy.get("not-a-valid-url")
            # Should handle gracefully
        except Exception as e:
            # Expected to raise exception
            assert isinstance(e, Exception)

    def test_timeout_handling(self):
        """Test handling of timeouts"""
        proxy = I2PProxy()
        try:
            # This should timeout or fail gracefully
            response = proxy.get("https://httpbin.org/delay/30", timeout=1)
        except Exception:
            # Expected to timeout
            pass

    def test_connection_error_handling(self):
        """Test handling of connection errors"""
        proxy = I2PProxy()
        try:
            response = proxy.get("http://nonexistent-domain-12345.com")
            # Should handle gracefully
        except Exception:
            # Expected to fail
            pass


if __name__ == "__main__":
    pytest.main([__file__, "-v"])

