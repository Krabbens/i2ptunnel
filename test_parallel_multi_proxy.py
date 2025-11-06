"""Test parallel chunk downloading with multiple proxies simultaneously"""
import sys
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from i2p_proxy import I2PProxy
from i2ptunnel import I2PProxyDaemon
from tqdm import tqdm


def download_chunk_with_proxy(proxy_instance, url, start_byte, end_byte, chunk_id, proxy_name=""):
    """Download a specific byte range using HTTP Range request with a specific proxy instance"""
    try:
        headers = {
            'Range': f'bytes={start_byte}-{end_byte}'
        }
        response = proxy_instance.get(url, headers=headers, stream=True)
        
        if response.status_code in (200, 206):  # 206 = Partial Content
            chunk_data = b''
            for chunk in response.iter_content(chunk_size=8192):
                if chunk:
                    chunk_data += chunk
            return chunk_id, chunk_data, True, len(chunk_data), proxy_name
        else:
            return chunk_id, b'', False, 0, proxy_name
    except Exception as e:
        print(f"\n[ERROR] Chunk {chunk_id} (proxy {proxy_name}): {e}")
        return chunk_id, b'', False, 0, proxy_name


def download_parallel_multi_proxy(url, num_threads=4, output_file="test_parallel_multi.mp4"):
    """Download file in parallel chunks using multiple proxies simultaneously"""
    print(f"\n{'='*60}")
    print(f"Multi-Proxy Parallel Download Speed Test")
    print(f"{'='*60}")
    print(f"URL: {url}")
    print(f"Threads: {num_threads}")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    # Initialize daemon to get available proxies
    daemon = I2PProxyDaemon()
    
    # Get file size
    print("[*] Getting file size...")
    proxy = I2PProxy()
    try:
        response = proxy.request('HEAD', url)
        if response.status_code == 200:
            file_size = int(response.headers.get('Content-Length', 0))
        else:
            response = proxy.get(url, stream=True)
            file_size = int(response.headers.get('Content-Length', 0))
    except Exception as e:
        print(f"[ERROR] Could not get file size: {e}")
        return False
    
    if file_size == 0:
        print("[ERROR] Could not determine file size")
        return False
    
    print(f"[*] File size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)\n")
    
    # Get available proxies
    print("[*] Fetching available proxies...")
    available_proxies = daemon.fetch_proxies()
    print(f"[*] Found {len(available_proxies)} available proxies")
    
    # Create multiple proxy instances - one per thread
    # Each proxy instance will use the shared daemon but can route through different proxies
    print(f"[*] Creating {num_threads} proxy instances for parallel downloads...\n")
    proxy_instances = [I2PProxy() for _ in range(num_threads)]
    
    # Calculate chunk sizes
    chunk_size = file_size // num_threads
    chunks = []
    for i in range(num_threads):
        start = i * chunk_size
        if i == num_threads - 1:
            end = file_size - 1  # Last chunk gets remainder
        else:
            end = start + chunk_size - 1
        chunks.append((start, end, i))
    
    print(f"[*] Splitting into {num_threads} parallel chunks:")
    for start, end, idx in chunks:
        print(f"   Chunk {idx}: bytes {start:,} - {end:,} ({(end-start+1)/1024/1024:.2f} MB)")
    print()
    
    # Download chunks in parallel, each using a different proxy instance
    downloaded_data = [None] * num_threads
    completed_chunks = 0
    total_downloaded = 0
    proxy_usage = {}
    
    print("[*] Starting parallel download with multiple proxies...\n")
    
    # Create progress bar
    with tqdm(total=file_size, unit='B', unit_scale=True, unit_divisor=1024,
             desc="Downloading", 
             bar_format='{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}]',
             ncols=80) as pbar:
        
        with ThreadPoolExecutor(max_workers=num_threads) as executor:
            # Submit all chunks, each using a different proxy instance
            future_to_chunk = {
                executor.submit(
                    download_chunk_with_proxy, 
                    proxy_instances[idx], 
                    url, 
                    start, 
                    end, 
                    idx,
                    f"proxy-{idx}"
                ): idx
                for start, end, idx in chunks
            }
            
            # Process completed chunks
            for future in as_completed(future_to_chunk):
                chunk_id, chunk_data, success, chunk_len, proxy_name = future.result()
                if success:
                    downloaded_data[chunk_id] = chunk_data
                    completed_chunks += 1
                    total_downloaded += chunk_len
                    proxy_usage[chunk_id] = proxy_name
                    pbar.update(chunk_len)
                else:
                    print(f"\n[ERROR] Failed to download chunk {chunk_id}")
                    return False
    
    elapsed_time = time.time() - start_time
    
    # Verify all chunks downloaded
    if completed_chunks != num_threads:
        print(f"\n[ERROR] Only downloaded {completed_chunks}/{num_threads} chunks")
        return False
    
    # Combine chunks in order
    print("\n[*] Combining chunks...")
    combined_data = b''.join(downloaded_data)
    
    if len(combined_data) != file_size:
        print(f"[WARNING] Downloaded {len(combined_data)} bytes, expected {file_size}")
    
    # Save file
    output_path = Path(output_file)
    with open(output_path, 'wb') as f:
        f.write(combined_data)
    
    # Calculate stats
    download_speed = len(combined_data) / elapsed_time / 1024 / 1024  # MB/s
    mbps = download_speed * 8  # Mbps
    
    print(f"\n{'='*60}")
    print(f"[SUCCESS] Multi-Proxy Parallel Download Complete!")
    print(f"{'='*60}")
    print(f"File: {output_path.absolute()}")
    print(f"Size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)")
    print(f"Time: {elapsed_time:.2f} seconds")
    print(f"Speed: {download_speed:.2f} MB/s ({mbps:.2f} Mbps)")
    print(f"Threads: {num_threads} parallel chunks")
    print(f"Proxies used: {len(set(proxy_usage.values()))} different proxy instances")
    print(f"{'='*60}\n")
    
    return True


def download_parallel_single_proxy(url, num_threads=4, output_file="test_parallel_single.mp4"):
    """Download file in parallel chunks using single proxy (for comparison)"""
    print(f"\n{'='*60}")
    print(f"Single-Proxy Parallel Download (for comparison)")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    proxy = I2PProxy()
    
    # Get file size
    try:
        response = proxy.request('HEAD', url)
        if response.status_code == 200:
            file_size = int(response.headers.get('Content-Length', 0))
        else:
            response = proxy.get(url, stream=True)
            file_size = int(response.headers.get('Content-Length', 0))
    except Exception as e:
        print(f"[ERROR] Could not get file size: {e}")
        return False
    
    if file_size == 0:
        print("[ERROR] Could not determine file size")
        return False
    
    # Calculate chunk sizes
    chunk_size = file_size // num_threads
    chunks = []
    for i in range(num_threads):
        start = i * chunk_size
        if i == num_threads - 1:
            end = file_size - 1
        else:
            end = start + chunk_size - 1
        chunks.append((start, end, i))
    
    # Download chunks in parallel, all using same proxy
    downloaded_data = [None] * num_threads
    completed_chunks = 0
    
    with tqdm(total=file_size, unit='B', unit_scale=True, unit_divisor=1024,
             desc="Downloading", 
             bar_format='{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}]',
             ncols=80) as pbar:
        
        with ThreadPoolExecutor(max_workers=num_threads) as executor:
            future_to_chunk = {
                executor.submit(download_chunk_with_proxy, proxy, url, start, end, idx, "single"): idx
                for start, end, idx in chunks
            }
            
            for future in as_completed(future_to_chunk):
                chunk_id, chunk_data, success, chunk_len, _ = future.result()
                if success:
                    downloaded_data[chunk_id] = chunk_data
                    completed_chunks += 1
                    pbar.update(chunk_len)
                else:
                    print(f"\n[ERROR] Failed to download chunk {chunk_id}")
                    return False
    
    elapsed_time = time.time() - start_time
    
    if completed_chunks != num_threads:
        print(f"\n[ERROR] Only downloaded {completed_chunks}/{num_threads} chunks")
        return False
    
    combined_data = b''.join(downloaded_data)
    output_path = Path(output_file)
    with open(output_path, 'wb') as f:
        f.write(combined_data)
    
    download_speed = len(combined_data) / elapsed_time / 1024 / 1024
    
    print(f"\n[SUCCESS] Single-proxy parallel download complete!")
    print(f"Time: {elapsed_time:.2f} seconds")
    print(f"Speed: {download_speed:.2f} MB/s ({download_speed * 8:.2f} Mbps)\n")
    
    return True


if __name__ == "__main__":
    url = "https://archive.org/download/archive-video-files/test.mp4"
    
    if len(sys.argv) > 1:
        url = sys.argv[1]
    
    num_threads = 4
    if len(sys.argv) > 2:
        num_threads = int(sys.argv[2])
    
    print("I2P Proxy Multi-Proxy Parallel Download Speed Test")
    print("=" * 60)
    
    # Test 1: Multi-proxy parallel download
    success1 = download_parallel_multi_proxy(url, num_threads=num_threads)
    
    if success1:
        # Test 2: Single-proxy parallel download for comparison
        download_parallel_single_proxy(url, num_threads=num_threads)
    
    sys.exit(0 if success1 else 1)

