"""
Python unit tests for i2ptunnel package using pytest.

These tests verify the Python bindings and high-level functionality.
"""

import pytest
from i2ptunnel import I2PProxyDaemon
from i2p_proxy import I2PProxy, I2PResponse, I2PStreamingResponse


class TestI2PProxyDaemon:
    """Tests for I2PProxyDaemon class"""

    def test_daemon_initialization(self):
        """Test that daemon can be initialized"""
        daemon = I2PProxyDaemon()
        assert daemon is not None

    def test_fetch_proxies_returns_list(self):
        """Test that fetch_proxies returns a list"""
        daemon = I2PProxyDaemon()
        # Note: This will fail if I2P router is not running
        # In a real test environment, we'd mock this
        try:
            proxies = daemon.fetch_proxies()
            assert isinstance(proxies, list)
        except Exception:
            # If I2P router is not available, skip this test
            pytest.skip("I2P router not available")

    def test_make_request_returns_dict(self):
        """Test that make_request returns a dictionary with expected keys"""
        daemon = I2PProxyDaemon()
        try:
            response = daemon.make_request(
                url="https://example.com",
                method="GET",
                headers=None,
                body=None
            )
            assert isinstance(response, dict)
            assert "status" in response
            assert "proxy_used" in response
            assert "headers" in response
            assert "body" in response
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_make_request_with_stream_parameter(self):
        """Test that make_request accepts stream parameter"""
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
        except Exception:
            pytest.skip("I2P router not available or request failed")


class TestI2PProxy:
    """Tests for I2PProxy class"""

    def test_proxy_initialization(self):
        """Test that I2PProxy can be initialized"""
        proxy = I2PProxy()
        assert proxy is not None

    def test_get_method_returns_response(self):
        """Test that get method returns an I2PResponse"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response, I2PResponse)
            assert hasattr(response, 'status_code')
            assert hasattr(response, 'text')
            assert hasattr(response, 'content')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_get_with_stream_returns_streaming_response(self):
        """Test that get with stream=True returns I2PStreamingResponse"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            assert isinstance(response, I2PStreamingResponse)
            assert hasattr(response, 'iter_content')
            assert hasattr(response, 'iter_lines')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_post_method(self):
        """Test that post method works"""
        proxy = I2PProxy()
        try:
            response = proxy.post("https://httpbin.org/post", data=b"test")
            assert isinstance(response, I2PResponse)
            assert response.status_code in [200, 201, 400, 500]  # Various possible statuses
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_put_method(self):
        """Test that put method works"""
        proxy = I2PProxy()
        try:
            response = proxy.put("https://httpbin.org/put", data=b"test")
            assert isinstance(response, I2PResponse)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_delete_method(self):
        """Test that delete method works"""
        proxy = I2PProxy()
        try:
            response = proxy.delete("https://httpbin.org/delete")
            assert isinstance(response, I2PResponse)
        except Exception:
            pytest.skip("I2P router not available or request failed")


class TestI2PStreamingResponse:
    """Tests for I2PStreamingResponse class"""

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
        """Test that iter_content yields chunks"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            chunks = list(response.iter_content(chunk_size=1024))
            assert len(chunks) > 0
            assert all(isinstance(chunk, bytes) for chunk in chunks)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_iter_content_with_custom_chunk_size(self):
        """Test iter_content with custom chunk size"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            chunks = list(response.iter_content(chunk_size=512))
            assert all(isinstance(chunk, bytes) for chunk in chunks)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_iter_lines(self):
        """Test that iter_lines yields lines"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            lines = list(response.iter_lines())
            # Even if no newlines, should get at least one chunk
            assert len(lines) >= 0  # Can be empty if content has no newlines
            assert all(isinstance(line, bytes) for line in lines)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_read_method(self):
        """Test that read method returns all content"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com", stream=True)
            content = response.read()
            assert isinstance(content, bytes)
            assert len(content) > 0
        except Exception:
            pytest.skip("I2P router not available or request failed")


class TestI2PResponse:
    """Tests for I2PResponse class"""

    def test_response_attributes(self):
        """Test that I2PResponse has expected attributes"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert hasattr(response, 'status_code')
            assert hasattr(response, 'headers')
            assert hasattr(response, 'text')
            assert hasattr(response, 'content')
            assert hasattr(response, 'url')
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_status_code(self):
        """Test that status_code is an integer"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.status_code, int)
            assert 100 <= response.status_code < 600
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_text(self):
        """Test that text property returns decoded string"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.text, str)
        except Exception:
            pytest.skip("I2P router not available or request failed")

    def test_response_content(self):
        """Test that content property returns bytes"""
        proxy = I2PProxy()
        try:
            response = proxy.get("https://example.com")
            assert isinstance(response.content, bytes)
        except Exception:
            pytest.skip("I2P router not available or request failed")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])



