"""Test parallel chunk downloading with progress bar"""
import sys
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from i2p_proxy import I2PProxy
from tqdm import tqdm

def download_chunk_range(proxy, url, start_byte, end_byte, chunk_id):
    """Download a specific byte range"""
    try:
        headers = {
            'Range': f'bytes={start_byte}-{end_byte}'
        }
        response = proxy.get(url, headers=headers, stream=True)
        
        if response.status_code in (200, 206):  # 206 = Partial Content
            chunk_data = b''
            for chunk in response.iter_content(chunk_size=8192):
                if chunk:
                    chunk_data += chunk
            return chunk_id, chunk_data, True
        else:
            return chunk_id, b'', False
    except Exception as e:
        print(f"\nError downloading chunk {chunk_id}: {e}")
        return chunk_id, b'', False


def download_parallel(proxy, url, num_threads=4, output_file="test_parallel.mp4"):
    """Download file in parallel chunks with progress bar"""
    print(f"\n{'='*60}")
    print(f"Parallel Download Test")
    print(f"{'='*60}")
    print(f"URL: {url}")
    print(f"Threads: {num_threads}")
    print(f"{'='*60}\n")
    
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
            print(f"Error getting file size: {e2}")
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
    
    print(f"[*] Splitting into {num_threads} chunks:")
    for start, end, idx in chunks:
        print(f"   Chunk {idx}: bytes {start:,} - {end:,} ({(end-start+1)/1024/1024:.2f} MB)")
    print()
    
    # Download chunks in parallel
    start_time = time.time()
    downloaded_data = [None] * num_threads
    completed_chunks = 0
    
    print("[*] Starting parallel download...\n")
    
    with tqdm(total=file_size, unit='B', unit_scale=True, unit_divisor=1024, 
              desc="Downloading", bar_format='{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}]') as pbar:
        
        with ThreadPoolExecutor(max_workers=num_threads) as executor:
            # Submit all chunks
            future_to_chunk = {
                executor.submit(download_chunk_range, proxy, url, start, end, idx): idx
                for start, end, idx in chunks
            }
            
            # Process completed chunks
            for future in as_completed(future_to_chunk):
                chunk_id, chunk_data, success = future.result()
                if success:
                    downloaded_data[chunk_id] = chunk_data
                    completed_chunks += 1
                    pbar.update(len(chunk_data))
                else:
                    print(f"\n[ERROR] Failed to download chunk {chunk_id}")
                    return False
    
    elapsed_time = time.time() - start_time
    
    # Verify all chunks downloaded
    if completed_chunks != num_threads:
        print(f"\n[ERROR] Only downloaded {completed_chunks}/{num_threads} chunks")
        return False
    
    # Combine chunks
    print("\n[*] Combining chunks...")
    combined_data = b''.join(downloaded_data)
    
    if len(combined_data) != file_size:
        print(f"[WARNING] Downloaded {len(combined_data)} bytes, expected {file_size}")
    
    # Save file
    output_path = Path(output_file)
    with open(output_path, 'wb') as f:
        f.write(combined_data)
    
    # Calculate stats
    download_speed = file_size / elapsed_time / 1024 / 1024  # MB/s
    
    print(f"\n{'='*60}")
    print(f"[SUCCESS] Download Complete!")
    print(f"{'='*60}")
    print(f"File: {output_path.absolute()}")
    print(f"Size: {file_size / 1024 / 1024:.2f} MB ({file_size:,} bytes)")
    print(f"Time: {elapsed_time:.2f} seconds")
    print(f"Speed: {download_speed:.2f} MB/s ({download_speed * 8:.2f} Mbps)")
    print(f"Chunks: {num_threads} parallel chunks")
    print(f"{'='*60}\n")
    
    return True


def download_single_stream(proxy, url, output_file="test_single.mp4"):
    """Download file in single stream for comparison"""
    print(f"\n{'='*60}")
    print(f"Single Stream Download (for comparison)")
    print(f"{'='*60}\n")
    
    start_time = time.time()
    
    try:
        response = proxy.get(url, stream=True)
        
        if response.status_code != 200:
            print(f"[ERROR] Status {response.status_code}")
            return False
        
        file_size = int(response.headers.get('Content-Length', 0))
        
        output_path = Path(output_file)
        
        with open(output_path, 'wb') as f:
            with tqdm(total=file_size, unit='B', unit_scale=True, unit_divisor=1024,
                     desc="Downloading", bar_format='{l_bar}{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}, {rate_fmt}]') as pbar:
                for chunk in response.iter_content(chunk_size=8192):
                    if chunk:
                        f.write(chunk)
                        pbar.update(len(chunk))
        
        elapsed_time = time.time() - start_time
        download_speed = file_size / elapsed_time / 1024 / 1024
        
        print(f"\n[SUCCESS] Single stream complete!")
        print(f"Time: {elapsed_time:.2f} seconds")
        print(f"Speed: {download_speed:.2f} MB/s ({download_speed * 8:.2f} Mbps)\n")
        
        return True
    except Exception as e:
        print(f"[ERROR] {e}")
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

