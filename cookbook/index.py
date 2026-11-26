#!/usr/bin/env python3
"""Generate chapter READMEs (examples/<chapter>/README.md) and the root INDEX.md (chapters, quick-find tasks, crate index).
Usage:  python3 index.py        (re-run after adding or moving examples; see also gen_manifest.py)"""
import collections, os
from layout import CHAPTERS, parse, folder_for, all_files

files = [parse(f) for f in all_files() if f.count("/") == 2]
by_id = {d["id"]: d for d in files}
chapter_of = {d["id"]: d["path"].split("/")[1] for d in files}

# "I want to ..." quick find (ids are validated; unknown ids are skipped with a warning)
QUICK = [
 ("Parse JSON/TOML-style data into typed structs", "r01 r66"),
 ("Report errors well (library vs application)", "r02 r60 r61 r63"),
 ("Understand lifetimes and the borrow checker", "r32 r36 r82"),
 ("Write unsafe code soundly", "r99 r100 r101 r102 r103"),
 ("Understand Pin, futures, wakers", "r52 r78 r103 c09"),
 ("Share data between threads", "r18 r50 r51 r146 r147"),
 ("Understand atomics and memory ordering", "r16 r110 r111 r192"),
 ("Implement a lock, channel or semaphore", "r193 r144"),
 ("Run at most N async jobs at once", "r07 r79 r94"),
 ("Cancel or time out async work", "r13 r81 r53"),
 ("Build an HTTP/REST service", "r27 r28 r29 r30 r31 c06"),
 ("Send HTTP requests and read responses", "r300 r308"),
 ("Cache HTTP responses (ETag, ranges, gzip)", "r301"),
 ("Cookies, sessions, CSRF", "r302"),
 ("Authenticate requests (Basic, Bearer, JWT, OAuth2)", "r303 r307"),
 ("CORS and security headers", "r304"),
 ("Design REST APIs (pagination, PATCH, errors)", "r305 r28"),
 ("Webhooks with signatures", "r306"),
 ("Write a network server", "r176 r177 r234"),
 ("Parse a binary protocol or file format", "r42 r155 r156 r158 r188 r189 r190"),
 ("Test protocol logic without sockets (sans-IO)", "r215"),
 ("Parse text with a grammar", "r19 r20 r64 r65 r92 r93"),
 ("Write a CLI", "r54"),
 ("Log and trace a service", "r55"),
 ("Find where time goes / speed code up", "r56 r57 r91"),
 ("Avoid allocations", "r57 r90 r170 r214"),
 ("Make code cache-friendly", "r84 r85 r87 r58"),
 ("Count distinct or frequent items cheaply", "r124 r125"),
 ("Cheap membership tests", "r24 r123"),
 ("Shard keys across nodes", "r25 r122"),
 ("Cache with an eviction policy", "r95 r230"),
 ("Order work by priority", "r05 r160 r232 r180"),
 ("Search text", "r23 r139 c07"),
 ("Nearest-neighbour and vector search", "r26 r140 r141 r142 r143"),
 ("Shortest paths", "r69 r134"),
 ("Connected components, cycles, bridges", "r67 r68 r135"),
 ("Dynamic programming patterns", "r71 r72 r73 r74 r75 r137"),
 ("Diff two sequences / 'did you mean?'", "r74 r72"),
 ("Retry with backoff; rate limit; circuit break", "r14 r15 c04 r216"),
 ("Persist data safely (WAL, atomic replace)", "r98 r43 r178 c11"),
 ("Memory-map a file; understand virtual memory", "r175 r240"),
 ("Build a worker pool or scheduler", "r94 r181 r180 c09"),
 ("Replicate state across nodes (CRDT, OT, logs)", "r150 r151 r157"),
 ("Detect failures in a cluster", "r153"),
 ("Fence stale lease holders", "c05"),
 ("Event sourcing, sagas, undo/redo", "c10 r237 r238"),
 ("Verify data integrity (hash trees)", "r154"),
 ("Encrypt and authenticate data (educational)", "r220 r221 r222"),
 ("Store passwords", "r226"),
 ("Compress data", "r183"),
 ("Encode data (base64, hex, UTF-8, varint)", "r185 r42"),
 ("Render images, ray trace, rasterize", "c12 r253 r251"),
 ("Simulate physics", "r258"),
 ("Procedural generation, mazes, fractals", "r256 r260"),
 ("SQL engines and query optimisation", "c02 c08 r211"),
 ("Design patterns, high and low level", "r213 r214 r47 r76"),
 ("Write macros", "r108"),
 ("Call C / expose Rust to C", "r105 r233"),
 ("Bit tricks and number theory", "r274 r275"),
 ("SAT, backtracking, constraint solving", "r272 r271"),
 ("Computational geometry", "r257 r277"),
 ("What's new in Rust", "r200 r201 r203"),
]

LEVEL_ORDER = {"simple": 0, "intermediate": 1, "advanced": 2, "capstone": 3}
def link(d, base=""):
    return f"[`{d['name']}`]({base}{chapter_of[d['id']]}/{d['file']})"

# --- chapter READMEs ---
for folder, title, intro in CHAPTERS:
    items = sorted([d for d in files if chapter_of[d["id"]] == folder], key=lambda d: (d["kind"] == "CAPSTONE", d["num"]))
    if not items: continue
    os.makedirs(f"examples/{folder}", exist_ok=True)
    out = [f"# {title}", "", intro, "", "Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` "
           "(the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.", "",
           f"{len(items)} examples.", "", "| Example | What it shows | Level | Common uses | Crates |", "|---|---|---|---|---|"]
    for d in items:
        crates = ", ".join(d["crates"]) if d["crates"] else "std only"
        common = d.get("common", "").replace("|", "/")
        if len(common) > 150: common = common[:147].rstrip() + "..."
        out.append(f"| [`{d['name']}`]({d['file']}) | {d['title'].rstrip('.')} | {d.get('level','?')} | {common} | {crates} |")
    out.append("")
    open(f"examples/{folder}/README.md", "w").write("\n".join(out))

# --- root INDEX.md ---
o = ["# FlowForge Cookbook: Index", "",
     "Generated by `python3 index.py` (do not edit by hand). Structure follows the Rust Cookbook idea: **chapters** (folders) of short, runnable recipes,",
     "a **task index** (\"I want to...\") and a **crate index**. Ours are concept recipes (`rNN_*`) plus **capstone** mini-projects (`cNN_*`).", "",
     "Levels: **simple** = one concept; **intermediate** = a pattern; **advanced** = combines several ideas; **capstone** = a large mini-project.", "",
     "## Chapters", "", "| Chapter | Examples | What is inside |", "|---|---|---|"]
for folder, title, intro in CHAPTERS:
    n = sum(1 for d in files if chapter_of[d["id"]] == folder)
    if n: o.append(f"| [{title}](examples/{folder}/README.md) | {n} | {intro} |")
o += ["", "## I want to...", "", "| Task | Start with |", "|---|---|"]
for task, ids in QUICK:
    links = []
    for i in ids.split():
        if i in by_id: links.append(link(by_id[i], "examples/"))
        else: print("quick-find: skipping missing", i)
    if links: o.append(f"| {task} | {' '.join(links)} |")
import glob as _g, re as _re
crate_dirs = sorted(_g.glob("crates/*/Cargo.toml"))
if crate_dirs:
    o += ["", "## Multi-crate recipes (`crates/`)", "", "| Crate | What it shows |", "|---|---|"]
    for ct in crate_dirs:
        txt = open(ct).read()
        name = _re.search(r'name\s*=\s*"([^"]+)"', txt); desc = _re.search(r'description\s*=\s*"([^"]+)"', txt)
        o.append(f"| [`{name.group(1) if name else ct}`]({os.path.dirname(ct)}/README.md) | {desc.group(1) if desc else ''} |")
o += ["", "## Crates used", "", "| Crate | Examples |", "|---|---|"]
crate_map = collections.defaultdict(list)
for d in files:
    for c in d["crates"]: crate_map[c].append(d)
for c in sorted(crate_map):
    ds = sorted(crate_map[c], key=lambda d: d["id"])
    shown = " ".join(link(d, "examples/") for d in ds[:12]) + (f" ... (+{len(ds) - 12})" if len(ds) > 12 else "")
    o.append(f"| `{c}` | {shown} |")
std_only = sum(1 for d in files if not d["crates"])
o += ["", f"{std_only} of {len(files)} examples use only the standard library.", "", "## Everything, by level", ""]
for lvl in ["simple", "intermediate", "advanced", "capstone"]:
    ds = sorted([d for d in files if d.get("level") == lvl], key=lambda d: d["id"])
    o += [f"### {lvl} ({len(ds)})", "", " ".join(link(d, "examples/") for d in ds), ""]
o += ["## Old id lookup", "", "Recipes were first numbered (`RECIPE NN` in each header). Files are now named by topic; this maps the old id to the file.", "",
      "| id | example | chapter |", "|---|---|---|"]
for d in sorted(files, key=lambda d: (d["id"][0], d["num"])):
    o.append(f"| {d['id']} | [`{d['name']}`](examples/{chapter_of[d['id']]}/{d['file']}) | {chapter_of[d['id']]} |")
open("INDEX.md", "w").write("\n".join(o))
print(f"{len(files)} examples, {len([c for c in CHAPTERS if any(chapter_of[d['id']] == c[0] for d in files)])} chapters")

# --- interview question bank: collect `//! Q<n>. text [Difficulty] [freq: x]` lines from examples/interview/*.rs ---
import re as _re2
qs = []
for d in sorted([d for d in files if chapter_of[d["id"]] == "interview"], key=lambda d: d["name"]):
    for line in open(d["path"]).read().split("\n")[:400]:
        m = _re2.match(r"//!\s*(Q\d+)\.\s*(.*)", line)
        if m: qs.append((d, m.group(1), m.group(2).strip()))
if qs:
    out = ["# Interview question bank", "", "Generated by `python3 index.py`. Every question is answered by a runnable, self-checking function in the file linked on its row.", "",
           "| # | Question | File |", "|---|---|---|"]
    for i, (d, q, text) in enumerate(qs, 1):
        out.append(f"| {i} | {text.replace('|', '/')} | [`{d['name']}`]({d['file']}) |")
    open("examples/interview/QUESTIONS.md", "w").write("\n".join(out) + "\n")
    print(len(qs), "interview questions indexed")

import subprocess as _sp
_sp.run(["python3", "courses.py"], check=False)

import subprocess, sys as _s
subprocess.run([_s.executable, 'paths.py'], check=True)
