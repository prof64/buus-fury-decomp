#!/usr/bin/env python3
"""Serve engine/web/dist/ on http://localhost:PORT/ (default 8370) with the right types and no caching.

usage: serve.py [--port N] [--dir DIR] [--bind ADDR]

The page needs no cross-origin isolation (COOP/COEP): it uses no SharedArrayBuffer, and COEP would get in the way
of the online lobby's signaling server. It does need a secure context (localhost or https) and these MIME types.
"""
import argparse
import http.server
import os

ap = argparse.ArgumentParser()
ap.add_argument("--port", type=int, default=8370)
ap.add_argument("--bind", default="127.0.0.1")
ap.add_argument("--dir", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web", "dist"))
a = ap.parse_args()


class H(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".wasm": "application/wasm",
        ".html": "text/html; charset=utf-8",
        ".json": "application/json",
    }

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        self.send_header("X-Content-Type-Options", "nosniff")
        super().end_headers()

    def log_message(self, fmt, *args):
        pass


d = os.path.abspath(a.dir)
if not os.path.exists(os.path.join(d, "app", "index.html")):
    raise SystemExit(f"{d} has no app/index.html: run engine/package/web-dist.sh first")
srv = http.server.ThreadingHTTPServer((a.bind, a.port), lambda *x, **k: H(*x, directory=d, **k))
print(f"Buu's Fury: http://localhost:{a.port}/app/   (serving {d}; Ctrl+C stops)")
try:
    srv.serve_forever()
except KeyboardInterrupt:
    pass
