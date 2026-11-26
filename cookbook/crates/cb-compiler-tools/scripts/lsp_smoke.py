#!/usr/bin/env python3
"""Smoke-test a `pebble-tools lsp` setup over real stdio. Standard library only.

Usage:  python3 scripts/lsp_smoke.py [PATH-TO-pebble-tools]
        (default: `pebble-tools` on PATH)

It does what an editor does: initialize, open a file with a type error, wait for
diagnostics, ask for hover, completion and formatting, then shutdown and exit.
Prints one line per step and exits 0 only if every step passed.
"""

import json
import queue
import subprocess
import sys
import threading

URI = "file:///smoke.peb"
SOURCE = (
    "/// Adds two numbers.\n"
    "fn add(a: int, b: int) -> int { a + b }\n"
    "fn main() {\n"
    "    let bad: int = \"x\";\n"
    "    println(to_str(add(1, 2)));\n"
    "}\n"
)
TIMEOUT = 20.0


def frame(msg):
    body = json.dumps(msg).encode()
    return b"Content-Length: %d\r\n\r\n" % len(body) + body


def reader(stream, out):
    while True:
        length = None
        while True:
            line = stream.readline()
            if not line:
                out.put(None)
                return
            line = line.strip()
            if not line:
                break
            name, _, value = line.partition(b":")
            if name.lower() == b"content-length":
                length = int(value)
        if length is None:
            out.put(None)
            return
        out.put(json.loads(stream.read(length)))


def main():
    server = sys.argv[1] if len(sys.argv) > 1 else "pebble-tools"
    try:
        proc = subprocess.Popen([server, "lsp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    except OSError as err:
        print(f"FAIL start: cannot run {server!r}: {err}")
        return 1
    inbox = queue.Queue()
    threading.Thread(target=reader, args=(proc.stdout, inbox), daemon=True).start()
    next_id = [0]

    def send(msg):
        proc.stdin.write(frame(msg))
        proc.stdin.flush()

    def wait(pred, what):
        while True:
            try:
                msg = inbox.get(timeout=TIMEOUT)
            except queue.Empty:
                raise RuntimeError(f"timed out waiting for {what}")
            if msg is None:
                raise RuntimeError(f"server closed the stream while waiting for {what}")
            if pred(msg):
                return msg

    def request(method, params):
        next_id[0] += 1
        rid = next_id[0]
        send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        msg = wait(lambda m: m.get("id") == rid and "method" not in m, method)
        if "error" in msg:
            raise RuntimeError(f"{method}: {msg['error']}")
        return msg.get("result")

    def notify(method, params):
        send({"jsonrpc": "2.0", "method": method, "params": params})

    def check(label, ok, detail):
        if not ok:
            raise RuntimeError(f"{label}: unexpected result {detail}")
        print(f"ok   {label}: {detail}")

    try:
        init = request("initialize", {"capabilities": {}})
        check("initialize", init["serverInfo"]["name"] == "pebble-tools", init["serverInfo"])
        notify("initialized", {})
        notify("textDocument/didOpen", {"textDocument": {"uri": URI, "languageId": "pebble", "version": 1, "text": SOURCE}})
        diags = wait(lambda m: m.get("method") == "textDocument/publishDiagnostics", "diagnostics")["params"]["diagnostics"]
        check("diagnostics", any(d["severity"] == 1 and d["range"]["start"]["line"] == 3 for d in diags),
              [f"{d['code']} line {d['range']['start']['line'] + 1}: {d['message'].splitlines()[0]}" for d in diags])
        doc = {"uri": URI}
        col = SOURCE.splitlines()[4].index("add")
        hover = request("textDocument/hover", {"textDocument": doc, "position": {"line": 4, "character": col}})
        check("hover", "fn add" in hover["contents"]["value"], hover["contents"]["value"].replace("\n", " | "))
        comp = request("textDocument/completion", {"textDocument": doc, "position": {"line": 4, "character": 4}})
        items = comp["items"] if isinstance(comp, dict) else comp
        labels = [i["label"] for i in items]
        check("completion", "add" in labels and "bad" in labels, f"{len(labels)} items, includes add and bad")
        edits = request("textDocument/formatting", {"textDocument": doc, "options": {"tabSize": 4, "insertSpaces": True}})
        check("formatting", isinstance(edits, list), f"{len(edits)} edit(s)")
        request("shutdown", None)
        notify("exit", None)
        code = proc.wait(timeout=TIMEOUT)
        check("exit", code == 0, f"status {code}")
    except (RuntimeError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as err:
        print(f"FAIL {err}")
        proc.kill()
        return 1
    print("all steps passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
