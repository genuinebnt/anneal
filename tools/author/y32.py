"""Y32: the mini Redis capstone, ten steps. Each step's starter holds the reference code of the earlier steps, cut from
y32_parts/template.rs. The preparation parts are in Y31, whose docstring holds the table of what each problem assesses
that no other does.
"""
import importlib
import pkgutil

from author import write_track
from y17_parts.common import rustfmt
import y32_parts

MODULES = [importlib.import_module(f"y32_parts.{m.name}")
           for m in sorted(pkgutil.iter_modules(y32_parts.__path__), key=lambda m: m.name) if m.name[:1] == "s" and m.name[1:3].isdigit()]

problems = []
for m in MODULES:
    p = dict(m.PROBLEM, mode="write", use=m.PROBLEM.get("use", y32_parts.common.USE))
    p.setdefault("crates", [])
    problems.append(p)

write_track(
    "y32-redis-capstone", "Y32", "Mini Redis capstone", "Y", "sde3", 32,
    "Build a small Redis in ten steps: the protocol, five kinds of value, expiry, transactions, blocking pops, publish and subscribe, persistence and a TCP server. Each step's starter carries the earlier steps.",
    [("protocol-and-data", "Protocol and data", "medium"), ("many-clients", "Many clients", "hard"), ("keep-and-serve", "Keep it and serve it", "hard")],
    problems,
)
rustfmt("y32-redis-capstone")
