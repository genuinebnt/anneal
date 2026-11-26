#!/usr/bin/env python3
"""Drive a real Helix (`hx`) against `pebble-tools lsp`. Standard library only, Unix only.

Usage:  python3 scripts/helix_smoke.py /absolute/path/to/pebble-tools

Helix has no headless mode, so this runs it on a pseudo-terminal with a throw-away
configuration directory, opens a file with a type error, types `:format`, `:write`
and `:quit!`, and then checks Helix's own protocol log and the saved file. Your real
Helix configuration is not read or changed. Exit status 0 only if every check passed.
"""

import fcntl
import os
import pty
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

SOURCE = 'fn add(a: int, b: int) -> int { a + b }\nfn main() {\n    let bad: int = "x";\n    println(to_str(add(1, 2)));\n}\n'

CONFIG = """[language-server.pebble]
command = "{server}"
args = ["lsp"]

[[language]]
name = "pebble"
scope = "source.pebble"
file-types = ["peb"]
comment-tokens = ["//", "///"]
block-comment-tokens = {{ start = "/*", end = "*/" }}
indent = {{ tab-width = 4, unit = "    " }}
language-servers = ["pebble"]
"""


def main():
    if len(sys.argv) != 2 or not os.path.isabs(sys.argv[1]):
        print(__doc__)
        return 2
    if shutil.which("hx") is None:
        print("FAIL hx is not on PATH")
        return 1
    root = tempfile.mkdtemp(prefix="pebble-hx-")
    os.makedirs(os.path.join(root, "config", "helix"))
    with open(os.path.join(root, "config", "helix", "languages.toml"), "w") as f:
        f.write(CONFIG.format(server=sys.argv[1]))
    path = os.path.join(root, "smoke.peb")
    with open(path, "w") as f:
        f.write(SOURCE)
    log = os.path.join(root, "hx.log")
    env = dict(os.environ, XDG_CONFIG_HOME=os.path.join(root, "config"), XDG_CACHE_HOME=os.path.join(root, "cache"), TERM="xterm-256color")

    health = subprocess.run(["hx", "--health", "pebble"], env=env, capture_output=True, text=True, timeout=30)
    print(health.stdout.strip())

    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(root)
        os.execvpe("hx", ["hx", "-vv", "--log", log, path], env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 0, 0))

    def pump(seconds):
        end = time.time() + seconds
        while time.time() < end:
            if select.select([fd], [], [], 0.05)[0]:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    return
                # Answer the terminal queries Helix may send at start-up.
                if b"\x1b[6n" in data:
                    os.write(fd, b"\x1b[1;1R")
                if b"\x1b[c" in data or b"\x1b[0c" in data:
                    os.write(fd, b"\x1b[?62;22c")

    for keys, wait in ((None, 4), (b":format\r", 2), (b":write\r", 2), (b":quit!\r", 3)):
        if keys:
            os.write(fd, keys)
        pump(wait)
    done, status = os.waitpid(pid, os.WNOHANG)
    if done == 0:
        os.kill(pid, 9)
        os.waitpid(pid, 0)

    with open(log, encoding="utf-8", errors="replace") as f:
        traffic = f.read()
    with open(path) as f:
        saved = f.read()
    checks = [
        ("hx quit by itself with status 0", done == pid and status == 0),
        ("initialize answered", '"serverInfo":{"name":"pebble-tools"' in traffic),
        ("diagnostics published with E0301", "publishDiagnostics" in traffic and '"E0301"' in traffic),
        ("formatting requested", '"method":"textDocument/formatting"' in traffic),
        ("formatting edit applied and saved", "fn add(a: int, b: int) -> int {\n    a + b\n}\n" in saved),
        ("shutdown and exit sent", '"method":"shutdown"' in traffic and '"method":"exit"' in traffic),
    ]
    for label, passed in checks:
        print(("ok   " if passed else "FAIL ") + label)
    if all(p for _, p in checks):
        shutil.rmtree(root)
        print("all steps passed")
        return 0
    print(f"log kept at {log}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
