"""
Pytest configuration file for i2ptunnel tests.
This ensures that the i2p_proxy module can be imported.
"""
import sys
from pathlib import Path

# Add the project root to Python path so i2p_proxy can be imported
project_root = Path(__file__).parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))



