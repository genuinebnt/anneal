"""Shared layout for the cookbook: chapters (folders), category mapping, header parsing.
Modelled on the Rust Cookbook (chapters of 'how do I...' recipes, a task index and a crate index), tweaked for concept
recipes (rNN) and capstone mini-projects (cNN)."""
import glob, os, re

# (folder, title, intro)
CHAPTERS = [
 ("language", "Language", "Ownership, lifetimes, generics, traits, closures, strings, iterators, pattern matching, macros and the newest Rust features."),
 ("std_tour", "Std and Core by Example", "Look-up chapter: how to use the methods, types and traits of std/core/alloc with realistic use cases (Option, Result, str, slices, iterators, collections, io, fs, time, sync, num, ops, mem, marker, net, fmt, convert, borrow, any, hash, cmp). Each file lists METHODS covered."),
 ("crate_tour", "Popular Crates by Example", "chrono, time, serde attributes, sqlx, itertools, regex, uuid, rand-style RNG, bytes, indexmap, url, base64 and more, each with a realistic use case and tests."),
 ("unsafe_ffi", "Unsafe and FFI", "Raw pointers, MaybeUninit, UnsafeCell, NonNull, Pin, layout, and calling C. Every unsafe block states its safety argument."),
 ("memory", "Memory Management", "Arenas, slabs, allocators and buffer pools."),
 ("errors", "Error Handling", "Error enums, context chains, retryable errors, diagnostics with spans, Option/Result combinators."),
 ("patterns", "Design Patterns", "Typestate, state machines, builders, command/undo, middleware, sans-IO, type erasure, event buses."),
 ("concurrency", "Concurrency", "Threads, locks, atomics and memory ordering, lock-free structures, channels, thread pools and data parallelism."),
 ("async", "Async", "Futures by hand, executors and wakers, streams, cancellation safety, async traits and a mini runtime."),
 ("data_structures", "Data Structures", "Heaps, trees, tries, skip lists, hash tables, probabilistic structures, caches, ring buffers, bitsets."),
 ("algorithms", "Algorithms", "Graphs, dynamic programming, string algorithms, combinatorics, SAT, sorting and searching, modelling by reduction."),
 ("math_science", "Math and Science", "Numerics, number theory, computational geometry, physics simulation, randomness."),
 ("graphics", "Graphics", "3D math, image formats, rasterization, ray tracing, 2D drawing, fractals."),
 ("compilers_runtimes", "Compilers and Runtimes", "WebAssembly decoding and interpretation, SSA and register allocation, type inference, parser generators, bytecode VMs."),
 ("text_and_parsing", "Text Processing and Parsing", "Lexers, parser combinators, recursive descent, Pratt parsing, JSON/CSV, bytecode compilers."),
 ("encoding_compression", "Encoding and Compression", "serde, base64/hex/UTF-8, varints, LZ77/Huffman, bitmaps."),
 ("cryptography", "Cryptography (educational)", "Hashes, MACs, AEADs, AES, RSA, signatures, constant-time code, password storage, Merkle trees. Not for production."),
 ("networking", "Networking and Protocols", "TCP servers, event loops, kqueue/epoll, io_uring model, HTTP/1.1, DNS, WebSocket, RESP."),
 ("web", "Web Programming", "axum routing, state, errors, SSE, middleware, idempotency; HTTP clients, caching, cookies, auth, CORS, REST patterns."),
 ("os_and_storage", "OS, Files and Storage", "Processes, signals, mmap, virtual memory, syscalls, buffered IO, file locking, disks and RAID, write-ahead logs."),
 ("database", "Databases", "Buffer pools, execution models, query optimisation, SQL, event sourcing."),
 ("microservices", "Microservice Patterns", "Transactional outbox and inbox, API gateway and discovery, strangler fig, leader election, idempotency and deduplication."),
 ("distributed", "Distributed Systems", "Consistent hashing, CRDTs, OT, clocks, SWIM, Kafka-style logs, leases and fencing, sagas, retries, rate limiting."),
 ("search_vectors", "Search and Vectors", "Inverted and trigram indexes, autocomplete, ANN (IVF, HNSW, Vamana, PQ), kNN."),
 ("hpc", "High-Performance Computing", "Parallel scans and reductions, stencils, scaling laws, roofline, MPI-style collectives, sample sort, sparse kernels."),
 ("perf", "Performance", "Measuring, allocation counting, cache behaviour, layout, inlining, branches, hashing speed, perf workflow."),
 ("interview", "Interview Questions", "Common Rust conceptual questions and coding questions (lists, trees, tries, caches, concurrency, systems design) answered as runnable recipes with Rust-specific pitfalls and follow-ups."),
 ("dev_tools", "Development Tools", "Testing patterns, property testing, CLI parsing, structured logging."),
 ("capstones", "Capstones", "150-2300 line mini-projects that combine many recipes: LSM store, SQL engine, regex engine, query optimiser, filesystem, async runtime, ray tracer, search engine."),
]
FOLDERS = [c[0] for c in CHAPTERS]

CAT = {}
def _m(folder, *cats):
    for c in cats: CAT[c] = folder
_m("language", "language", "types", "traits", "macros", "api-design", "rust-features", "iterators", "collections")
_m("unsafe_ffi", "unsafe", "ffi")
_m("memory", "memory")
_m("errors", "errors")
_m("patterns", "patterns")
_m("concurrency", "concurrency")
_m("async", "async")
_m("data_structures", "data-structures", "trees", "caching", "hashing", "probabilistic")
_m("algorithms", "algorithms", "graphs", "dp", "combinatorics", "bits", "strings")
_m("math_science", "numerics", "geometry", "simulation", "randomness")
_m("graphics", "graphics")
_m("text_and_parsing", "parsing", "automata")
_m("compilers_runtimes", "compilers", "runtime")
_m("distributed", "streaming")
_m("encoding_compression", "encoding", "compression", "data")
_m("cryptography", "crypto", "integrity")
_m("networking", "network", "protocols")
_m("web", "axum", "web")
_m("os_and_storage", "os", "systems", "storage")
_m("database", "database")
_m("distributed", "distributed", "messaging", "rate-limiting", "resilience")
_m("search_vectors", "search", "vectors")
_m("perf", "perf")
_m("std_tour", "std")
_m("crate_tour", "crates")
_m("hpc", "hpc")
_m("microservices", "microservices")
_m("dev_tools", "testing", "tooling", "observability")
_m("interview", "interview")
OVERRIDE = {94: "async", 95: "data_structures", 96: "patterns", 97: "text_and_parsing", 98: "os_and_storage"}

HEADER_RE = re.compile(r"//! (RECIPE|CAPSTONE) (\d+) \(([^)]*)\): (.*)")
EXTERNAL = ["serde", "serde_json", "thiserror", "anyhow", "tokio", "axum", "tower", "tower_http", "http_body_util", "futures_util",
            "clap", "tracing", "tracing_subscriber", "libc", "memmap2", "mio", "rayon", "crossbeam_channel", "crossbeam_deque",
            "crossbeam_utils", "bytes", "parking_lot", "smallvec", "reqwest"]

def parse(path):
    text = open(path).read()
    head = text.split("\n")[:60]
    d = {"path": path, "file": os.path.basename(path), "name": os.path.basename(path)[:-3]}
    for l in head:
        m = HEADER_RE.match(l)
        if m:
            d.update(kind=m.group(1), num=int(m.group(2)), cat=m.group(3), title=m.group(4).strip()); break
    for l in head:
        if l.startswith("//! LEVEL:"): d["level"] = l.split(":", 1)[1].strip()
        if l.startswith("//! Use in:"): d["use"] = re.sub(r"\s*Run:.*$", "", l.split(":", 1)[1]).strip()
        if l.startswith("//! COMMON USES:"): d["common"] = l.split(":", 1)[1].strip()
    code = "\n".join(l for l in text.split("\n") if not l.lstrip().startswith("//"))
    d["crates"] = sorted({c.replace("_", "-") for c in EXTERNAL if re.search(r"\b%s::|\buse %s\b|\buse %s;" % (c, c, c), code)})
    d["id"] = ("c" if d.get("kind") == "CAPSTONE" else "r") + (f"{d['num']:02d}" if d.get("num", 0) < 100 else str(d["num"]))
    return d

def folder_for(d):
    if d["kind"] == "CAPSTONE": return "capstones"
    if d["num"] in OVERRIDE: return OVERRIDE[d["num"]]
    return CAT.get(d["cat"], "misc")

def all_files():
    return sorted(f for f in glob.glob("examples/*/*.rs") + glob.glob("examples/*.rs") if "/scratch/" not in f)
