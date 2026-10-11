#!/usr/bin/env python3
"""Mutation check of the BusTub stage tests (an experiment; see docs/EXERCISE_ASSESSMENT.md).

For each reference source file listed in FILES it makes small changes ("mutants": a comparison flipped, a `+ 1` dropped, a call
deleted, ...) inside the solution blocks of that file, one at a time, and runs the stage test binaries that own the file. A mutant
whose tests all still pass SURVIVED: either an equivalent mutant (no behaviour change) or a behaviour no test pins. Read every survivor
before drawing a conclusion.

    cp -R courses/bustub/reference /tmp/mut/ref     # a scratch COPY: the harness edits files in place and restores them
    MUT_ROOT=/tmp/mut/ref MUT_TARGET=/tmp/mut/target MUT_OUT=/tmp/mut/results.jsonl python3 tools/course_mutation.py 6

One build at a time (CARGO_BUILD_JOBS=2, nice 15): a parallel run exhausted memory on the owner's laptop. Never point it at the real
reference directory: a kill in the middle would leave a mutated file behind.
"""
import re, random, subprocess, json, pathlib, os, sys, time
ROOT = pathlib.Path(os.environ.get("MUT_ROOT", "/tmp/mut/ref"))
TARGET = os.environ.get("MUT_TARGET", "/tmp/mut/target")
OUT = open(os.environ.get("MUT_OUT", "/tmp/mut/results.jsonl"), "a")
FILES = ["types/value.rs", "storage/page/table_page.rs", "container/disk/hash/disk_extendible_hash_table.rs", "storage/disk/disk_manager.rs",
         "recovery/store.rs", "storage/page/page_guard.rs", "concurrency/lock_manager.rs"]
PER_FILE = int(sys.argv[1]) if len(sys.argv) > 1 else 7
import re as _re
OPS = [(_re.escape(" <= "), " < "), (_re.escape(" < "), " <= "), (_re.escape(" >= "), " > "), (_re.escape(" > "), " >= "), (_re.escape(" == "), " != "), (_re.escape(" != "), " == "), (_re.escape(" && "), " || "), (_re.escape(" || "), " && "),
       (r"\+ 1(?![0-9_.])", "+ 0"), (r"- 1(?![0-9_.])", "- 0"), (r"\btrue\b", "false"), (r"\bfalse\b", "true"), (r"\.max\(", ".min("), (r"\.min\(", ".max(")]
DEL = re.compile(r"^\s+[\w\.\[\]]+\.(push|push_back|insert|remove|clear|pop|swap|truncate|retain|extend|fill|copy_from_slice|flush|sync_all|sync_data|set_\w+)\(.*\);\s*$")

def sites(rel):
    """(line index, kind, new line) for every mutation inside a marked solution block of the reference file."""
    lines = (ROOT / "src" / rel).read_text().splitlines(keepends=True)
    stack, out, ids = [], [], set()
    for i, l in enumerate(lines):
        m = re.search(r"@begin\s+([0-9a-z]+-[0-9a-z]+)", l)
        if m: stack.append(m.group(1)); ids.add(m.group(1)); continue
        if "@end" in l and stack: stack.pop(); continue
        code = l.split("//")[0]
        if not stack or not code.strip() or "todo!" in l or "assert" in code or '"' in code and code.count('"') % 2: continue
        for pat, rep in OPS:
            for mm in re.finditer(pat, code):
                if mm.end() == mm.start(): continue
                out.append((i, rep.strip(), l[:mm.start()] + rep + l[mm.end():]))
        if DEL.match(code): out.append((i, "delete", "\n"))
    return lines, out, ids

def run(bins):
    cmd = ["nice", "-n", "15", "cargo", "test", "-q"] + [x for b in bins for x in ("--test", b)]
    env = dict(os.environ, CARGO_TARGET_DIR=TARGET, CARGO_BUILD_JOBS="2")
    try:
        p = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True, timeout=480)
    except subprocess.TimeoutExpired:
        return "timeout", ""
    txt = p.stdout + p.stderr
    if p.returncode == 0: return "survived", ""
    if "could not compile" in txt or "error[" in txt or "error: " in txt.split("test result")[0] and "failures:" not in txt: return "invalid", txt[-300:]
    fails = re.findall(r"^\s+(\w+)$", txt.split("failures:")[-1], re.M)[:3] if "failures:" in txt else []
    return "killed", ",".join(fails)

random.seed(11)
for rel in FILES:
    lines, cand, ids = sites(rel)
    codes = sorted({i.split("-")[0] for i in ids})
    bins = [f"stages_{c}" for c in codes if (ROOT / "tests" / f"stages_{c}.rs").exists()]
    if not bins: print("no bins for", rel); continue
    base, _ = run(bins)
    OUT.write(json.dumps(dict(file=rel, kind="baseline", result=base, bins=bins, candidates=len(cand))) + "\n"); OUT.flush()
    if base != "survived": continue
    random.shuffle(cand); done = 0
    orig = (ROOT / "src" / rel).read_text()
    for (i, kind, new) in cand:
        if done >= PER_FILE: break
        mutated = "".join(lines[:i]) + new + "".join(lines[i + 1:])
        (ROOT / "src" / rel).write_text(mutated)
        try: res, info = run(bins)
        finally: (ROOT / "src" / rel).write_text(orig)
        if res == "invalid": continue
        done += 1
        OUT.write(json.dumps(dict(file=rel, line=i + 1, op=kind, was=lines[i].strip()[:110], now=new.strip()[:110], result=res, info=info, bins=bins)) + "\n"); OUT.flush()
OUT.write(json.dumps(dict(kind="done")) + "\n"); OUT.close()
