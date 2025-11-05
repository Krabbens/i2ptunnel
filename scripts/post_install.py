"""
Post-install script to copy i2p_proxy.py to site-packages
"""
import os
import shutil
import site

def post_install():
    # Get site-packages directory
    site_packages = site.getsitepackages()[0] if site.getsitepackages() else None
    if not site_packages:
        # Fallback for virtual environments
        import sysconfig
        site_packages = sysconfig.get_path('purelib')
    
    if site_packages and os.path.exists(site_packages):
        # Find the source file
        package_dir = os.path.dirname(os.path.abspath(__file__))
        src = os.path.join(package_dir, 'i2p_proxy', '__init__.py')
        dst = os.path.join(site_packages, 'i2p_proxy.py')
        
        if os.path.exists(src):
            shutil.copy2(src, dst)
            print(f"Copied i2p_proxy.py to {dst}")

if __name__ == '__main__':
    post_install()

