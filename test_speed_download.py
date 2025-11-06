"""Test download speed with progress bar"""
import sys
import time
from pathlib import Path
from i2p_proxy import I2PProxy
from tqdm import tqdm


def download_with_progress(proxy, url, output_file="test_speed.mp4"):
    """Download file with progress bar and speed measurement"""
    print(f"\n{'='*60}")
    print(f"Download Speed Test")
    print(f"{'='*60}")
    print(f"URL: {url}")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    try:
        print("[*] Starting download...\n")
        # Use make_request_streaming directly from daemon for better control
        from i2ptunnel import I2PProxyDaemon
        daemon = I2PProxyDaemon()
        
        # Get file size first
        temp_response = proxy.get(url, stream=True)
        if temp_response.status_code != 200:
            print(f"[ERROR] Status {temp_response.status_code}")
            return False
        
        file_size = int(temp_response.headers.get('Content-Length', 0))
        if file_size == 0:
            print("[ERROR] Could not determine file size")
            return False
        
        print(f"[*] File size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)\n")
        
        output_path = Path(output_file)
        downloaded = 0
        chunk_count = 0
        
        # Use streaming request directly
        result = daemon.make_request_streaming(
            url=url,
            method="GET",
            headers=None,
            body=None,
            chunk_size=8192
        )
        
        if result["status"] != 200:
            print(f"[ERROR] Status {result['status']}")
            return False
        
        # Progress bar with speed
        with tqdm(total=file_size, unit='B', unit_scale=True, unit_divisor=1024,
                 desc="Downloading", 
                 bar_format='{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}]',
                 ncols=80) as pbar:
            
            with open(output_path, 'wb') as f:
                for chunk in result["chunks"]:
                    if chunk:
                        chunk_bytes = bytes(chunk)
                        f.write(chunk_bytes)
                        downloaded += len(chunk_bytes)
                        chunk_count += 1
                        pbar.update(len(chunk_bytes))
        
        elapsed_time = time.time() - start_time
        download_speed = downloaded / elapsed_time / 1024 / 1024  # MB/s
        mbps = download_speed * 8  # Mbps
        
        # Verify file
        actual_size = output_path.stat().st_size
        
        print(f"\n{'='*60}")
        print(f"[SUCCESS] Download Complete!")
        print(f"{'='*60}")
        print(f"File: {output_path.absolute()}")
        print(f"Expected size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)")
        print(f"Actual size:   {actual_size / 1024 / 1024:.2f} MB ({actual_size:,} bytes)")
        print(f"Time: {elapsed_time:.2f} seconds")
        print(f"Speed: {download_speed:.2f} MB/s ({mbps:.2f} Mbps)")
        print(f"Chunks downloaded: {chunk_count:,}")
        print(f"Average chunk size: {downloaded / chunk_count / 1024:.2f} KB")
        print(f"{'='*60}\n")
        
        if actual_size != file_size:
            print(f"[WARNING] Size mismatch! Expected {file_size}, got {actual_size}")
        
        return True
        
    except Exception as e:
        print(f"[ERROR] {e}")
        import traceback
        traceback.print_exc()
        return False


if __name__ == "__main__":
    url = "https://archive.org/download/archive-video-files/test.mp4"
    
    if len(sys.argv) > 1:
        url = sys.argv[1]
    
    print("I2P Proxy Download Speed Test")
    print("=" * 60)
    
    proxy = I2PProxy()
    
    success = download_with_progress(proxy, url)
    
    sys.exit(0 if success else 1)

