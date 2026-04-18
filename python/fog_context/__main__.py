import os
import sys
import platform
import subprocess
import urllib.request
import tempfile
import stat

VERSION = "0.5.2"
REPO = "luciusvo/fog-context"

def get_binary_name():
    system = platform.system().lower()
    machine = platform.machine().lower()
    
    if system == "linux":
        if machine in ["x86_64", "amd64"]:
            return "fog-mcp-linux-amd64"
        elif machine in ["aarch64", "arm64"]:
            return "fog-mcp-linux-arm64"
    elif system == "darwin":
        if machine in ["x86_64", "amd64"]:
            return "fog-mcp-macos-amd64"
        elif machine in ["aarch64", "arm64"]:
            return "fog-mcp-macos-arm64"
    elif system == "windows":
        if machine in ["x86_64", "amd64"]:
            return "fog-mcp-windows-amd64.exe"
            
    print(f"Unsupported platform: {system} {machine}", file=sys.stderr)
    sys.exit(1)

def main():
    bin_name = get_binary_name()
    
    # Store binary in ~/.fog-context/bin/
    home = os.path.expanduser("~")
    bin_dir = os.path.join(home, ".fog-context", "bin")
    os.makedirs(bin_dir, exist_ok=True)
    
    bin_path = os.path.join(bin_dir, f"fog-context-{VERSION}")
    if platform.system().lower() == "windows" and not bin_path.endswith(".exe"):
        bin_path += ".exe"

    if not os.path.exists(bin_path):
        url = f"https://github.com/{REPO}/releases/download/v{VERSION}/{bin_name}"
        print(f"Downloading fog-context v{VERSION} for {platform.system()}...", file=sys.stderr)
        try:
            url_resp = urllib.request.urlopen(url)
            with open(bin_path, 'wb') as f:
                f.write(url_resp.read())
            
            # Make executable
            if platform.system().lower() != "windows":
                st = os.stat(bin_path)
                os.chmod(bin_path, st.st_mode | stat.S_IEXEC)
                
            print("Download complete.", file=sys.stderr)
        except Exception as e:
            print(f"Failed to download fog-context binary: {e}", file=sys.stderr)
            if os.path.exists(bin_path):
                os.remove(bin_path)
            sys.exit(1)

    # Launch it
    # os.execv replaces the current process with the binary, avoiding subprocess overhead
    # First arg must be the program name
    if platform.system().lower() == "windows":
        sys.exit(subprocess.call([bin_path] + sys.argv[1:]))
    else:
        os.execv(bin_path, ["fog-context"] + sys.argv[1:])

if __name__ == "__main__":
    main()
