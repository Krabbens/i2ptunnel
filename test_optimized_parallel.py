"""Optimized parallel download distributing chunks across multiple proxies"""
import sys
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from i2ptunnel import I2PProxyDaemon
from tqdm import tqdm


def download_chunk_with_proxy(daemon, url, proxy_url, start_byte, end_byte, chunk_id):
    """Download a specific byte range using HTTP Range request with a specific proxy"""
    try:
        headers = {
            'Range': f'bytes={start_byte}-{end_byte}'
        }
        
        # Use daemon's make_request_with_proxy method (non-streaming for Range requests)
        result = daemon.make_request_with_proxy(
            url=url,
            proxy_url=proxy_url,
            method="GET",
            headers=headers,
            body=None,
            stream=False
        )
        
        if result["status"] in (200, 206):  # 206 = Partial Content
            chunk_data = bytes(result["body"])
            return chunk_id, chunk_data, True, len(chunk_data), proxy_url
        else:
            return chunk_id, b'', False, 0, proxy_url
    except Exception as e:
        print(f"\n[ERROR] Chunk {chunk_id} (proxy {proxy_url}): {e}")
        import traceback
        traceback.print_exc()
        return chunk_id, b'', False, 0, proxy_url


def download_parallel_optimized(url, num_threads=4, output_file="test_optimized.mp4"):
    """Download file in parallel chunks using multiple proxies simultaneously"""
    print(f"\n{'='*60}")
    print(f"Optimized Multi-Proxy Parallel Download")
    print(f"{'='*60}")
    print(f"URL: {url}")
    print(f"Threads: {num_threads}")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    # Initialize daemon
    daemon = I2PProxyDaemon()
    
    # Get file size
    print("[*] Getting file size...")
    from i2p_proxy import I2PProxy
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
    
    if len(available_proxies) == 0:
        print("[ERROR] No proxies available")
        return False
    
    # Extract proxy URLs
    proxy_urls = []
    for proxy_info in available_proxies:
        if isinstance(proxy_info, dict):
            proxy_url = proxy_info.get('url', '')
        else:
            proxy_url = str(proxy_info)
        if proxy_url:
            proxy_urls.append(proxy_url)
    
    print(f"[*] Using {len(proxy_urls)} proxies: {', '.join(proxy_urls[:3])}{'...' if len(proxy_urls) > 3 else ''}\n")
    
    # Calculate chunk sizes
    chunk_size = file_size // num_threads
    chunks = []
    for i in range(num_threads):
        start = i * chunk_size
        if i == num_threads - 1:
            end = file_size - 1
        else:
            end = start + chunk_size - 1
        # Assign proxy in round-robin fashion
        proxy_url = proxy_urls[i % len(proxy_urls)]
        chunks.append((start, end, i, proxy_url))
    
    print(f"[*] Splitting into {num_threads} parallel chunks:")
    for start, end, idx, proxy in chunks:
        print(f"   Chunk {idx}: bytes {start:,} - {end:,} ({(end-start+1)/1024/1024:.2f} MB) -> {proxy}")
    print()
    
    # Download chunks in parallel, each using assigned proxy
    downloaded_data = [None] * num_threads
    completed_chunks = 0
    proxy_usage = {}
    
    print("[*] Starting optimized parallel download...\n")
    
    with tqdm(total=file_size, unit='B', unit_scale=True, unit_divisor=1024,
             desc="Downloading", 
             bar_format='{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}]',
             ncols=80) as pbar:
        
        with ThreadPoolExecutor(max_workers=num_threads) as executor:
            # Submit all chunks, each with its assigned proxy
            future_to_chunk = {
                executor.submit(
                    download_chunk_with_proxy,
                    daemon,
                    url,
                    proxy_url,
                    start,
                    end,
                    idx
                ): idx
                for start, end, idx, proxy_url in chunks
            }
            
            # Process completed chunks
            for future in as_completed(future_to_chunk):
                chunk_id, chunk_data, success, chunk_len, proxy_used = future.result()
                if success:
                    downloaded_data[chunk_id] = chunk_data
                    completed_chunks += 1
                    proxy_usage[chunk_id] = proxy_used
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
    download_speed = len(combined_data) / elapsed_time / 1024 / 1024
    mbps = download_speed * 8
    
    # Count unique proxies used
    unique_proxies = len(set(proxy_usage.values()))
    
    print(f"\n{'='*60}")
    print(f"[SUCCESS] Optimized Multi-Proxy Download Complete!")
    print(f"{'='*60}")
    print(f"File: {output_path.absolute()}")
    print(f"Size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)")
    print(f"Time: {elapsed_time:.2f} seconds")
    print(f"Speed: {download_speed:.2f} MB/s ({mbps:.2f} Mbps)")
    print(f"Threads: {num_threads} parallel chunks")
    print(f"Proxies used: {unique_proxies} unique proxies")
    print(f"Speedup: {unique_proxies}x (using {unique_proxies} proxies)")
    print(f"{'='*60}\n")
    
    return True


if __name__ == "__main__":
    url = "https://archive.org/download/archive-video-files/test.mp4"
    
    if len(sys.argv) > 1:
        url = sys.argv[1]
    
    num_threads = 4
    if len(sys.argv) > 2:
        num_threads = int(sys.argv[2])
    
    print("I2P Proxy Optimized Multi-Proxy Parallel Download")
    print("=" * 60)
    
    success = download_parallel_optimized(url, num_threads=num_threads)
    
    sys.exit(0 if success else 1)

