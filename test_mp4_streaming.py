"""Test MP4 streaming through I2P proxy"""
import sys
from pathlib import Path
from i2p_proxy import I2PProxy

def test_mp4_streaming(url: str, output_file: str = "test_output.mp4"):
    """Test streaming an MP4 file through I2P proxy"""
    print(f"Testing MP4 streaming from: {url}")
    print("=" * 60)
    
    try:
        proxy = I2PProxy()
        
        print("\n1. Making streaming request (stream=True)...")
        response = proxy.get(url, stream=True)
        
        print(f"   Status Code: {response.status_code}")
        print(f"   Headers: {dict(list(response.headers.items())[:5])}")
        
        if response.status_code != 200:
            print(f"   Error: Received status {response.status_code}")
            return False
        
        print("\n2. Streaming content to file...")
        output_path = Path(output_file)
        total_bytes = 0
        chunk_count = 0
        
        with open(output_path, 'wb') as f:
            for chunk in response.iter_content(chunk_size=8192):
                if chunk:
                    f.write(chunk)
                    total_bytes += len(chunk)
                    chunk_count += 1
                    if chunk_count % 100 == 0:
                        print(f"   Received {chunk_count} chunks, {total_bytes / 1024 / 1024:.2f} MB")
        
        file_size = output_path.stat().st_size
        print(f"\n3. Download complete!")
        print(f"   Total chunks: {chunk_count}")
        print(f"   Total size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)")
        print(f"   Saved to: {output_path.absolute()}")
        
        if file_size > 0:
            print("\n[SUCCESS] MP4 streaming test passed!")
            return True
        else:
            print("\n[ERROR] Downloaded file is empty")
            return False
            
    except Exception as e:
        print(f"\n[ERROR] Streaming test failed: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_mp4_streaming_small_chunks(url: str):
    """Test streaming with smaller chunks"""
    print(f"\nTesting with smaller chunks (512 bytes)...")
    print("=" * 60)
    
    try:
        proxy = I2PProxy()
        response = proxy.get(url, stream=True)
        
        if response.status_code != 200:
            print(f"Error: Status {response.status_code}")
            return False
        
        total = 0
        chunks = 0
        for chunk in response.iter_content(chunk_size=512):
            if chunk:
                total += len(chunk)
                chunks += 1
                if chunks % 500 == 0:
                    print(f"  Received {chunks} chunks, {total / 1024:.2f} KB")
        
        print(f"\nSmall chunk test complete: {chunks} chunks, {total / 1024:.2f} KB")
        return True
        
    except Exception as e:
        print(f"Error: {e}")
        return False


if __name__ == "__main__":
    # Test with a publicly available small MP4 file
    # Using a test video URL - you can replace this with any MP4 URL
    test_url = "https://sample-videos.com/video321/mp4/720/big_buck_bunny_720p_1mb.mp4"
    
    if len(sys.argv) > 1:
        test_url = sys.argv[1]
    
    print("I2P Proxy MP4 Streaming Test")
    print("=" * 60)
    print(f"URL: {test_url}\n")
    
    # Test 1: Stream to file
    success = test_mp4_streaming(test_url)
    
    # Test 2: Stream with small chunks
    if success:
        test_mp4_streaming_small_chunks(test_url)
    
    sys.exit(0 if success else 1)



