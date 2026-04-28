import sys
import json

req = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
        "name": "fog_scan",
        "arguments": {}
    }
}

payload = json.dumps(req)
sys.stdout.write(f"Content-Length: {len(payload)}\r\n\r\n{payload}")
