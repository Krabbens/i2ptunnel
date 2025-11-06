"""Test parallel chunk downloading with HTTP Range requests and progress bar"""
import sys
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from i2p_proxy import I2PProxy
from i2ptunnel import I2PProxyDaemon
from rich.progress import Progress, SpinnerColumn, BarColumn, TextColumn, TimeElapsedColumn, TimeRemainingColumn, TransferSpeedColumn, DownloadColumn
from rich.console import Console


def download_chunk_range_with_proxy(daemon, url, start_byte, end_byte, chunk_id, proxy_url, router_port=None, progress=None, task_id=None, overall_task_id=None):
    """Download a specific byte range using HTTP Range request through a specific proxy"""
    try:
        # Convert headers dict
        headers_dict = {
            'Range': f'bytes={start_byte}-{end_byte}'
        }
        
        # Use make_request_streaming_with_proxy to use a specific proxy with router port hint
        result = daemon.make_request_streaming_with_proxy(
            url=url,
            proxy_url=proxy_url,
            method="GET",
            headers=headers_dict,
            body=None,
            chunk_size=8192,
            router_port=router_port  # Use specific router port for this chunk
        )
        
        if result["status"] in (200, 206):  # 206 = Partial Content
            chunk_data = b''
            for chunk in result["chunks"]:
                chunk_bytes = bytes(chunk)
                chunk_data += chunk_bytes
                if progress:
                    if task_id is not None:
                        progress.update(task_id, advance=len(chunk_bytes))
                    if overall_task_id is not None:
                        progress.update(overall_task_id, advance=len(chunk_bytes))
            proxy_used = result.get("proxy_used", proxy_url)
            return chunk_id, chunk_data, True, len(chunk_data), proxy_used
        else:
            return chunk_id, b'', False, 0, f"Status {result['status']}"
    except Exception as e:
        return chunk_id, b'', False, 0, str(e)


def download_parallel(proxy, url, num_threads=4, output_file="test_parallel.mp4"):
    """Download file in parallel chunks with progress bar"""
    print(f"\n{'='*60}")
    print(f"Parallel Download Speed Test")
    print(f"{'='*60}")
    print(f"URL: {url}")
    print(f"Threads: {num_threads}")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    # First, get file size with HEAD request
    print("[*] Getting file size...")
    try:
        # Try HEAD request
        response = proxy.request('HEAD', url)
        if response.status_code == 200:
            file_size = int(response.headers.get('Content-Length', 0))
        else:
            # Fallback: try GET with stream (just for headers)
            response = proxy.get(url, stream=True)
            file_size = int(response.headers.get('Content-Length', 0))
    except Exception as e:
        # If HEAD fails, try GET
        try:
            response = proxy.get(url, stream=True)
            file_size = int(response.headers.get('Content-Length', 0))
        except Exception as e2:
            print(f"[ERROR] Could not get file size: {e2}")
            return False
    
    if file_size == 0:
        print("[ERROR] Could not determine file size")
        return False
    
    print(f"[*] File size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)\n")
    
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
    
    # Get available proxies and assign them to chunks
    from i2ptunnel import I2PProxyDaemon
    daemon = I2PProxyDaemon()
    available_proxies = daemon.fetch_proxies()
    
    if not available_proxies:
        print("[ERROR] No proxies available")
        return False
    
    # Extract proxy URLs
    proxy_urls = []
    for proxy in available_proxies:
        if isinstance(proxy, dict):
            proxy_urls.append(proxy.get('url', ''))
        else:
            proxy_urls.append(str(proxy))
    
    # Filter out empty URLs
    proxy_urls = [url for url in proxy_urls if url]
    
    if not proxy_urls:
        print("[ERROR] No valid proxy URLs found")
        return False
    
    print(f"[*] Found {len(proxy_urls)} available proxies:")
    for i, proxy_url in enumerate(proxy_urls):
        print(f"   Proxy {i}: {proxy_url[:60]}")
    print(f"[*] Distributing {num_threads} chunks across {len(proxy_urls)} proxies")
    
    # Available router ports: HTTP (4444) and SOCKS (4447 for i2pd)
    # Use different router ports for different chunks to get true parallelism
    # For i2pd: 4444 = HTTP, 4447 = SOCKS (default)
    router_ports = [4444, 4447]  # HTTP and SOCKS proxy ports
    print(f"[*] Using router ports {router_ports} for parallel connections")
    print(f"    (Port 4444 = HTTP, Port 4447 = SOCKS for i2pd)\n")
    
    # Download chunks in parallel
    downloaded_data = [None] * num_threads
    completed_chunks = 0
    total_downloaded = 0
    proxy_usage = {}
    
    console = Console()
    console.print("[*] Starting parallel download with multiple proxies...\n", style="cyan")
    
    # Create rich progress display with multiple tasks (like uv)
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        DownloadColumn(),
        TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
        TimeElapsedColumn(),
        TimeRemainingColumn(),
        TransferSpeedColumn(),
        console=console,
        expand=True
    ) as progress:
        # Create overall task
        overall_task = progress.add_task(
            "[cyan]Downloading",
            total=file_size,
        )
        
        # Create individual chunk tasks
        chunk_tasks = {}
        for start, end, idx in chunks:
            chunk_size = end - start + 1
            task_id = progress.add_task(
                f"[green]Chunk {idx}",
                total=chunk_size,
            )
            chunk_tasks[idx] = task_id
        
        with ThreadPoolExecutor(max_workers=num_threads) as executor:
            # Submit all chunks with proxy assignment
            # Distribute chunks across available proxies AND router ports for true parallelism
            future_to_chunk = {
                executor.submit(
                    download_chunk_range_with_proxy, 
                    daemon, 
                    url, 
                    start, 
                    end, 
                    idx, 
                    proxy_urls[idx % len(proxy_urls)],
                    router_ports[idx % len(router_ports)],  # Use different router ports
                    progress,  # Pass progress object
                    chunk_tasks[idx],  # Pass task ID for this chunk
                    overall_task  # Pass overall task ID for real-time updates
                ): idx
                for start, end, idx in chunks
            }
            
            # Process completed chunks
            for future in as_completed(future_to_chunk):
                chunk_id, chunk_data, success, chunk_len, proxy_info = future.result()
                if success:
                    downloaded_data[chunk_id] = chunk_data
                    completed_chunks += 1
                    total_downloaded += chunk_len
                    # Mark chunk task as complete (progress already updated in real-time during download)
                    progress.stop_task(chunk_tasks[chunk_id])
                    
                    # Track proxy usage
                    proxy_key = str(proxy_info)[:50]  # Truncate long proxy strings
                    if proxy_key not in proxy_usage:
                        proxy_usage[proxy_key] = 0
                    proxy_usage[proxy_key] += chunk_len
                else:
                    console.print(f"\n[ERROR] Failed to download chunk {chunk_id}: {proxy_info}", style="red")
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
    print(f"[SUCCESS] Parallel Download Complete!")
    print(f"{'='*60}")
    print(f"File: {output_path.absolute()}")
    print(f"Size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)")
    print(f"Time: {elapsed_time:.2f} seconds")
    print(f"Speed: {download_speed:.2f} MB/s ({mbps:.2f} Mbps)")
    print(f"Threads: {num_threads} parallel chunks")
    print(f"Proxies used: {len(proxy_usage)}")
    if proxy_usage:
        print(f"\nProxy usage distribution:")
        for proxy, bytes_transferred in sorted(proxy_usage.items(), key=lambda x: x[1], reverse=True):
            print(f"  {proxy[:60]}: {bytes_transferred / 1024 / 1024:.2f} MB ({bytes_transferred / elapsed_time / 1024 / 1024:.2f} MB/s)")
    print(f"{'='*60}\n")
    
    return True


def download_single_stream(proxy, url, output_file="test_single.mp4"):
    """Download file in single stream for comparison"""
    print(f"\n{'='*60}")
    print(f"Single Stream Download (for comparison)")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    try:
        from i2ptunnel import I2PProxyDaemon
        daemon = I2PProxyDaemon()
        
        # Get file size first
        temp_response = proxy.get(url, stream=True)
        if temp_response.status_code != 200:
            print(f"[ERROR] Status {temp_response.status_code}")
            return False
        
        file_size = int(temp_response.headers.get('Content-Length', 0))
        
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
        
        output_path = Path(output_file)
        downloaded = 0
        
        console = Console()
        with open(output_path, 'wb') as f:
            with Progress(
                SpinnerColumn(),
                TextColumn("[progress.description]{task.description}"),
                BarColumn(),
                DownloadColumn(),
                TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
                TimeElapsedColumn(),
                TimeRemainingColumn(),
                TransferSpeedColumn(),
                console=console,
                expand=True
            ) as progress:
                task = progress.add_task(
                    "[cyan]Downloading",
                    total=file_size,
                )
                for chunk in result["chunks"]:
                    if chunk:
                        chunk_bytes = bytes(chunk)
                        f.write(chunk_bytes)
                        downloaded += len(chunk_bytes)
                        progress.update(task, advance=len(chunk_bytes))
        
        elapsed_time = time.time() - start_time
        download_speed = downloaded / elapsed_time / 1024 / 1024
        
        print(f"\n[SUCCESS] Single stream complete!")
        print(f"Time: {elapsed_time:.2f} seconds")
        print(f"Speed: {download_speed:.2f} MB/s ({download_speed * 8:.2f} Mbps)\n")
        
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
    
    num_threads = 4
    if len(sys.argv) > 2:
        num_threads = int(sys.argv[2])
    
    print("I2P Proxy Parallel Download Speed Test")
    print("=" * 60)
    
    proxy = I2PProxy()
    
    # Test 1: Parallel download
    success1 = download_parallel(proxy, url, num_threads=num_threads)
    
    if success1:
        # Test 2: Single stream for comparison
        download_single_stream(proxy, url)
    
    sys.exit(0 if success1 else 1)

