#!/usr/bin/env python3
"""通用 CLI → HTTP 桥：POST body 作为最后一个参数追加到 CMD 后执行，返回 JSON。

用法: cli-bridge.py <port> <cmd...>
例:   cli-bridge.py 7403 brush -c
      POST /  body=echo hi   →  执行  brush -c 'echo hi'
"""
import json
import subprocess
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1])
CMD = sys.argv[2:]


class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        print("[bridge] " + (fmt % a), flush=True)

    def _j(self, code, obj):
        d = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(d)))
        self.end_headers()
        self.wfile.write(d)

    def do_GET(self):
        if self.path == "/health":
            self._j(200, {"ok": True, "cmd": CMD})
        else:
            self._j(404, {"error": "not found"})

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n).decode("utf-8", "replace")
        try:
            r = subprocess.run(CMD + [body], capture_output=True, text=True, timeout=600)
            self._j(200, {"ok": r.returncode == 0, "code": r.returncode,
                          "stdout": r.stdout, "stderr": r.stderr})
        except Exception as e:
            self._j(500, {"ok": False, "error": f"{type(e).__name__}: {e}"})


print(f"[bridge] 监听 127.0.0.1:{PORT} · 命令: {' '.join(CMD)}", flush=True)
ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
