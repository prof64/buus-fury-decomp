"""Tiny MCP stdio client for the wf-mcp server (play tests, scripts).

    from mcp_client import MCP
    m = MCP()                      # spawns engine/target/release/wf-mcp (or $WF_MCP)
    r = m.call("new_battle", fighter="goku", level=80, enemy="t68", count=3)
    r = m.call("attack", session=r["session"], stop_on=["hurt"])

Speaks the real protocol (initialize, tools/list, tools/call over newline-delimited JSON-RPC).
"""
import base64
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_BIN = os.path.normpath(os.path.join(HERE, "../../../target/release/wf-mcp"))


class ToolError(Exception):
    pass


class MCP:
    def __init__(self, binary=None, env=None):
        e = dict(os.environ)
        e.update(env or {})
        self.p = subprocess.Popen([binary or os.environ.get("WF_MCP", DEFAULT_BIN)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=e, text=True, bufsize=1)
        self.n = 0
        init = self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "mcp_client.py", "version": "0"}})
        self.server_info = init["serverInfo"]
        self.notify("notifications/initialized")
        self.tools = {t["name"]: t for t in self.rpc("tools/list", {})["tools"]}

    def notify(self, method, params=None):
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params or {}}) + "\n")
        self.p.stdin.flush()

    def rpc(self, method, params):
        self.n += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        line = self.p.stdout.readline()
        if not line:
            raise RuntimeError("server closed")
        r = json.loads(line)
        if "error" in r:
            raise RuntimeError(r["error"])
        return r["result"]

    def call_raw(self, _tool, **args):
        return self.rpc("tools/call", {"name": _tool, "arguments": args})

    def call(self, _tool, **args):
        r = self.call_raw(_tool, **args)
        texts = [c["text"] for c in r["content"] if c["type"] == "text"]
        if r.get("isError"):
            raise ToolError(texts[0] if texts else "error")
        imgs = [c for c in r["content"] if c["type"] == "image"]
        if imgs:
            cap = json.loads(texts[0]) if texts else {}
            cap["png"] = base64.b64decode(imgs[0]["data"])
            return cap
        return json.loads(texts[0])

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=10)


if __name__ == "__main__":
    # python3 mcp_client.py <tool> '<json args>'  (one-shot; sessions do not persist between invocations)
    m = MCP()
    out = m.call(sys.argv[1], **(json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}))
    out.pop("png", None)
    print(json.dumps(out, indent=1))
