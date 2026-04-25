import sys
import json

req = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
        "name": "fog_search",
        "arguments": {
            "query": "return",
            "context_lines": 2,
            "is_regex": False,
            "project": "/home/admin/Downloads/Framework/FoG Framework/MCP/fog-context"
        }
    }
}

payload = json.dumps(req)
sys.stdout.write(f"Content-Length: {len(payload)}\r\n\r\n{payload}")
