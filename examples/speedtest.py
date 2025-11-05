#!/usr/bin/env python3
"""
I2P Speedtest Example with tqdm Progress Bar

This script demonstrates how to use the @i2p decorator to download files
through I2P proxies with progress tracking using tqdm.

Usage:
    python examples/speedtest.py [URL] [--requests N]
    
Examples:
    python examples/speedtest.py
    python examples/speedtest.py https://example.com/file.zip
    python examples/speedtest.py https://speed.cloudflare.com/__down?bytes=10485760 --requests 5
    python examples/speedtest.py --requests 5
"""

import argparse
import time
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Tuple, Optional
from i2p_proxy import i2p, I2PProxy
from tqdm import tqdm


def format_bytes(bytes_count: int) -> str:
    """Format bytes to human-readable format"""
    for unit in ['B', 'KB', 'MB', 'GB']:
        if bytes_count < 1024.0:
            return f"{bytes_count:.2f} {unit}"
        bytes_count /= 1024.0
    return f"{bytes_count:.2f} TB"


def format_speed(bytes_per_sec: float) -> str:
    """Format speed to human-readable format"""
    if bytes_per_sec < 1024:
        return f"{bytes_per_sec:.2f} B/s"
    elif bytes_per_sec < 1024 * 1024:
        return f"{bytes_per_sec / 1024:.2f} KB/s"
    else:
        return f"{bytes_per_sec / (1024 * 1024):.2f} MB/s"


def update_progress_bar(pbar: tqdm, start_time: float, stop_event: threading.Event):
    """Update progress bar based on elapsed time while waiting for response"""
    while not stop_event.is_set():
        elapsed = time.time() - start_time
        if pbar:
            # Update description with elapsed time
            pbar.set_description(f"Downloading... ({elapsed:.1f}s)")
            # Refresh to show update
            pbar.refresh()
        time.sleep(0.1)


@i2p
def download_file(url: str, pbar: Optional[tqdm] = None) -> Tuple[int, float, bool]:
    """
    Download a file through I2P proxy using the @i2p decorator.
    
    Args:
        url: URL to download
        pbar: Optional tqdm progress bar to update
        
    Returns:
        Tuple of (bytes_downloaded, elapsed_time, success)
    """
    start_time = time.time()
    success = False
    bytes_downloaded = 0
    stop_event = threading.Event()
    progress_thread = None
    
    try:
        # Start the request
        if pbar:
            pbar.set_description(f"Connecting to {url[:45]}...")
            
            # Start a thread to update progress bar with elapsed time
            progress_thread = threading.Thread(
                target=update_progress_bar,
                args=(pbar, start_time, stop_event),
                daemon=True
            )
            progress_thread.start()
        
        # Make the request (this will go through I2P proxy automatically)
        # Access requests dynamically to ensure we get the thread-safe wrapper
        import sys
        try:
            requests_module = sys.modules.get('requests')
            if requests_module is None:
                # Fallback to the imported requests if not in sys.modules
                import requests as fallback_requests
                requests_module = fallback_requests
        except NameError:
            # If requests is not defined, import it
            import requests as requests_module
        response = requests_module.get(url, timeout=60)
        
        # Stop the progress update thread
        stop_event.set()
        if progress_thread:
            progress_thread.join(timeout=0.5)
        
        # Calculate elapsed time
        elapsed_time = time.time() - start_time
        
        if response.status_code == 200:
            bytes_downloaded = len(response.content)
            success = True
            
            if pbar:
                # Update progress bar to 100%
                if bytes_downloaded > 0:
                    pbar.total = bytes_downloaded
                    pbar.n = bytes_downloaded
                pbar.refresh()
                pbar.set_description("Download complete")
                pbar.set_postfix({
                    'size': format_bytes(bytes_downloaded),
                    'speed': format_speed(bytes_downloaded / elapsed_time) if elapsed_time > 0 else "0 B/s",
                    'time': f"{elapsed_time:.2f}s"
                })
        else:
            if pbar:
                pbar.set_description("Download failed")
                pbar.set_postfix({'error': f'HTTP {response.status_code}'})
            
    except Exception as e:
        stop_event.set()
        if progress_thread:
            progress_thread.join(timeout=0.5)
        elapsed_time = time.time() - start_time
        
        # Check if it's a requests exception
        import sys
        requests_module = sys.modules.get('requests')
        is_requests_exception = False
        if requests_module and hasattr(requests_module, 'exceptions'):
            try:
                is_requests_exception = isinstance(e, requests_module.exceptions.RequestException)
            except (AttributeError, TypeError):
                pass
        
        if pbar:
            pbar.set_description("Download failed")
            error_msg = str(e)[:30]
            pbar.set_postfix({'error': error_msg})
        
        # Only treat as handled if it's a requests exception
        if not is_requests_exception:
            raise
    
    return bytes_downloaded, elapsed_time, success


def check_ip_through_i2p():
    """Check IP address through I2P tunnel"""
    print(f"\n{'='*70}")
    print("Checking IP Address Through I2P Tunnel")
    print(f"{'='*70}\n")
    
    @i2p
    def get_ip():
        import sys
        try:
            requests_module = sys.modules.get('requests')
            if requests_module is None:
                import requests as fallback_requests
                requests_module = fallback_requests
        except NameError:
            import requests as requests_module
        
        response = requests_module.get('https://api.ipify.org?format=json', timeout=30)
        if response.status_code == 200:
            return response.json()
        return None
    
    try:
        print("Fetching IP address through I2P tunnel...")
        result = get_ip()
        if result:
            ip = result.get('ip', 'Unknown')
            print(f"IP Address: {ip}")
            
            # Check if this looks like it might be the user's real IP
            # (This is a heuristic - if the IP is the same as their public IP, 
            # the router might not be configured for outproxies)
            print(f"\n[INFO] IP Address shown: {ip}")
            print(f"[WARNING] If this is your real public IP, the I2P router may not be configured for outproxies.")
            print(f"[INFO] To enable outproxy support:")
            print(f"  1. Open I2P router console (usually http://127.0.0.1:7657)")
            print(f"  2. Go to Configuration > Outproxy")
            print(f"  3. Enable outproxy support and add outproxies")
            print(f"  4. Or configure the router to auto-discover outproxies")
            print(f"\n[NOTE] The @i2p decorator routes requests through the I2P router's proxy,")
            print(f"       but the router must be configured to use outproxies for clearnet sites.")
        else:
            print("[FAIL] Failed to get IP address")
    except Exception as e:
        print(f"[ERROR] {e}")
    
    print(f"\n{'='*70}\n")


def download_with_progress(url: str, request_id: int, num_requests: int) -> Tuple[int, float, bool, int]:
    """
    Wrapper function for parallel downloads with progress tracking.
    
    Returns:
        Tuple of (bytes_downloaded, elapsed_time, success, request_id)
    """
    # Create individual progress bar for this request
    with tqdm(
        total=1,  # Start with 1, will be updated
        unit='B',
        unit_scale=True,
        unit_divisor=1024,
        desc=f"Request {request_id}/{num_requests}",
        position=request_id,
        leave=False,
        miniters=1,
        mininterval=0.1
    ) as download_pbar:
        bytes_downloaded, elapsed_time, success = download_file(url, download_pbar)
        return bytes_downloaded, elapsed_time, success, request_id


def run_speedtest(url: str, num_requests: int = 1, parallel: bool = True):
    """
    Run speedtest with multiple requests (parallel or sequential).
    
    Args:
        url: URL to download
        num_requests: Number of requests to make
        parallel: Whether to run requests in parallel (default: True)
    """
    print(f"\n{'='*70}")
    print(f"I2P Speedtest - {url}")
    print(f"Mode: {'Parallel' if parallel else 'Sequential'}")
    print(f"{'='*70}\n")
    
    results: List[Tuple[int, float, bool]] = []
    total_bytes = 0
    total_time = 0.0
    successful_requests = 0
    start_time = time.time()
    
    if parallel and num_requests > 1:
        # Parallel execution
        with ThreadPoolExecutor(max_workers=num_requests) as executor:
            # Submit all download tasks
            futures = {
                executor.submit(download_with_progress, url, i+1, num_requests): i+1 
                for i in range(num_requests)
            }
            
            # Create overall progress bar
            with tqdm(total=num_requests, desc="Total Progress", unit="req", position=num_requests+1) as req_pbar:
                # Process completed downloads as they finish
                for future in as_completed(futures):
                    try:
                        bytes_downloaded, elapsed_time, success, request_id = future.result()
                        results.append((bytes_downloaded, elapsed_time, success))
                        
                        if success:
                            total_bytes += bytes_downloaded
                            total_time += elapsed_time
                            successful_requests += 1
                            
                            speed = bytes_downloaded / elapsed_time if elapsed_time > 0 else 0
                            req_pbar.set_postfix({
                                'completed': f"{successful_requests}/{num_requests}",
                                'total_speed': format_speed(total_bytes / (time.time() - start_time))
                            })
                        else:
                            req_pbar.set_postfix({'failed': f"Request {request_id}"})
                        
                        req_pbar.update(1)
                    except Exception as e:
                        req_pbar.set_postfix({'error': str(e)[:30]})
                        req_pbar.update(1)
    else:
        # Sequential execution (for single request or when parallel=False)
        with tqdm(total=num_requests, desc="Requests", unit="req", position=0) as req_pbar:
            for i in range(num_requests):
                # Create progress bar for individual download
                with tqdm(
                    total=1,  # Start with 1, will be updated
                    unit='B',
                    unit_scale=True,
                    unit_divisor=1024,
                    desc=f"Request {i+1}/{num_requests}",
                    position=1,
                    leave=False,
                    miniters=1,
                    mininterval=0.1
                ) as download_pbar:
                    # Download file
                    bytes_downloaded, elapsed_time, success = download_file(url, download_pbar)
                    
                    if success:
                        results.append((bytes_downloaded, elapsed_time, success))
                        total_bytes += bytes_downloaded
                        total_time += elapsed_time
                        successful_requests += 1
                        
                        speed = bytes_downloaded / elapsed_time if elapsed_time > 0 else 0
                        req_pbar.set_postfix({
                            'speed': format_speed(speed),
                            'size': format_bytes(bytes_downloaded)
                        })
                    else:
                        results.append((bytes_downloaded, elapsed_time, success))
                        req_pbar.set_postfix({'status': 'Failed'})
                
                req_pbar.update(1)
    
    # Calculate total wall-clock time
    total_wall_time = time.time() - start_time
    
    # Print final statistics
    print(f"\n{'='*70}")
    print("Speedtest Results")
    print(f"{'='*70}")
    print(f"Total Requests:     {num_requests}")
    print(f"Successful:         {successful_requests}")
    print(f"Failed:             {num_requests - successful_requests}")
    
    if successful_requests > 0:
        print(f"\nTotal Downloaded:   {format_bytes(total_bytes)}")
        if parallel and num_requests > 1:
            print(f"Wall-clock Time:    {total_wall_time:.2f} seconds")
            print(f"Combined Speed:     {format_speed(total_bytes / total_wall_time)}")
        else:
            print(f"Total Time:         {total_time:.2f} seconds")
            print(f"Average Speed:      {format_speed(total_bytes / total_time)}")
        
        # Calculate individual request statistics
        speeds = [bytes_dl / elapsed for bytes_dl, elapsed, _ in results if elapsed > 0]
        if speeds:
            print(f"\nFastest Speed:      {format_speed(max(speeds))}")
            print(f"Slowest Speed:      {format_speed(min(speeds))}")
            if len(speeds) > 1:
                avg_speed = sum(speeds) / len(speeds)
                print(f"Average Speed:      {format_speed(avg_speed)}")
    
    print(f"{'='*70}\n")


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(
        description="I2P Speedtest with tqdm progress bar",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s
  %(prog)s https://example.com/file.zip
  %(prog)s https://example.com/file.zip --requests 5
        """
    )
    parser.add_argument(
        'url',
        nargs='?',
        default='https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_272x92dp.png',
        help='URL to download (default: Google logo image). For larger tests, try: https://speed.cloudflare.com/__down?bytes=10485760'
    )
    parser.add_argument(
        '--requests',
        '-r',
        type=int,
        default=3,
        help='Number of requests to make (default: 3)'
    )
    parser.add_argument(
        '--sequential',
        '-s',
        action='store_true',
        help='Run requests sequentially instead of in parallel'
    )
    parser.add_argument(
        '--check-ip',
        action='store_true',
        help='Check IP address before running speedtest (to verify I2P tunnel)'
    )
    
    args = parser.parse_args()
    
    # Check IP if requested
    if args.check_ip:
        check_ip_through_i2p()
    
    try:
        run_speedtest(args.url, args.requests, parallel=not args.sequential)
    except KeyboardInterrupt:
        print("\n\nSpeedtest interrupted by user.")
    except Exception as e:
        print(f"\nError: {e}")
        return 1
    
    return 0


if __name__ == '__main__':
    exit(main())

