"""Lays out the notes of a code file so they can be scanned: the words are kept, only their shape and capitals change.

- A bullet that opens with a term in capitals becomes a row: the term, then what is said about it.
- Capitals used for emphasis become bold in ordinary case; abbreviations (MVCC, SSI, SQL) stay.
- A long paragraph is split between sentences, and a label such as "Consequences:" starts its own.
- A line of comma-separated uses becomes a list.

The English words that tell emphasis from an abbreviation are in tools/cookbook/words.txt.
"""
import html, pathlib, re

# words the system dictionary does not hold: newer ones, plurals it misses and British spellings
EXTRA = """verified timestamp serializability serializable orderings committed bytes checkpoint shortest backpressure footguns bitmap overlapping pushdown database runtime iterator linearizability backoff idempotency bitvector mapping metadata cardinality catalog associativity largest vectorized optimisation rollup gotcha minimises scheduler colour checklist checksum boolean memtable dedup multi updater antidependency optimised nondeterministic workspace cooperative linearised autoscaling commutativity bytecode baselines inlined mergeable planning materialisation tuple online downsampling closest boxes smallest behaviour piggybacking optimisations compensatable retriable deterministically whitelist deduplicate download internet startup decoder amortised longest recognise subarray counterexample encodings synthesise generalise normalisation metavariable newtype destructures wakeup automation coordinated oldest spinlock empirically earliest intra non anti sub pre post self timestamps checkpoints largest prewrite defunctionalisation"""
WORDS = frozenset((pathlib.Path(__file__).parent / "words.txt").read_text().split()) | frozenset(EXTRA.split())
# dictionary words that are abbreviations or names when written in capitals
KEEP = frozenset("""ACID AIDS ALU AMD ANN ANSI API APIS ARC ARM ART ASCII AST AVL AWS BASE BFS BST CAS CDC CFG CLI CORS CPU CRC CRDT CSE CSV CTE DAG DB DBMS DCE DDL DFS DHT DMA DML DNS DOM DP DSL EOF ETL FFI FIFO FIN FSM FST GC GPU GUI HAMT HTAP HTML HTTP ID IO IOT IP IR ISA JIT JSON JVM LAN LCA LEB LFU LIFO LL LOB LRU LSM LSN LSP LTO MAC MIR ML MPSC MPMC MVCC NAT NFA DFA NLJ NUMA OCC OLAP OLTP ORM OS OT PAX PBFT PC PDF PEG PID PK PNG POSIX PPM QPS RAFT RAII RAM RCU REPL REST RID RLE ROM RPC RTT SAT SCC SEQ SI SIMD SLA SLO SMT SPSC SQL SSA SSD SSI SSL STM SWIM SYN TCB TCP TLB TLS TPC TTL UDP UI URI URL UTF UUID VM WAL WASM XML XOR""".split())
SHORT = frozenset("A AN AT BE BY DO IF IN IS IT NO OF ON OR SO TO UP WE AS".split())
SENTENCE = re.compile(r"(?<=[.!?])\s+(?=[A-Z\"'(`])")
TOKEN = r"[A-Z][A-Z0-9]*(?:['/-][A-Z0-9]+)*"


def is_word(part):
    w = part.lower()
    forms = [w, w[:-1] if w.endswith("s") else w, w[:-2] if w.endswith("es") else w, w[:-3] + "y" if w.endswith("ies") else w,
             w[:-2] if w.endswith("ed") else w, w[:-1] if w.endswith("d") else w, w[:-3] if w.endswith("ing") else w,
             w[:-3] + "e" if w.endswith("ing") else w, w[:-2] if w.endswith("er") else w]
    return any(len(f) > 1 and f in WORDS for f in forms)


def emphatic(token):
    """Whether a token in capitals is a word being stressed rather than an abbreviation."""
    if token in KEEP or any(c.isdigit() for c in token):
        return False
    parts = [p for p in re.split(r"['/-]", token) if p]
    if len(token) <= 2:
        return False
    return all(p in SHORT or (p not in KEEP and len(p) > 2 and is_word(p)) or (len(parts) > 1 and p in ("RE", "UN", "CO", "S", "T")) for p in parts)


def calm(text):
    """Capitals used for emphasis, in ordinary case and marked with ** (returned as \\x02 \\x03 around the words)."""
    # an address is left exactly as written
    addresses = re.findall(r"https?://\S+", text)
    if addresses:
        between = re.split(r"https?://\S+", text)
        return "".join(calm(part) + (addresses[k] if k < len(addresses) else "") for k, part in enumerate(between))
    out = []
    # code in backticks is never touched
    for i, piece in enumerate(text.split("`")):
        if i % 2:
            out.append(piece)
            continue
        tokens = list(re.finditer(rf"(?<![A-Za-z0-9_.:]){TOKEN}(?![A-Za-z0-9_(\[]|::)", piece))
        runs, run = [], []
        for t in tokens:
            joined = run and re.fullmatch(r"[ ]+", piece[run[-1].end():t.start()])
            if run and not joined:
                runs.append(run)
                run = []
            run.append(t)
        if run:
            runs.append(run)
        edits = []
        for run in runs:
            words = [t[0] for t in run]
            strong = [w for w in words if emphatic(w) and len(w) >= 3]
            if not strong:
                continue
            start, end = run[0].start(), run[-1].end()
            lowered = " ".join(w.lower() if emphatic(w) or w in SHORT or (len(words) > 1 and w not in KEEP and is_word(w)) else w for w in words)
            before = piece[:start].rstrip()
            if (not before and i == 0) or before.endswith((".", "!", "?")):
                lowered = lowered[:1].upper() + lowered[1:]
            edits.append((start, end, f"\x02{lowered}\x03"))
        for start, end, new in reversed(edits):
            piece = piece[:start] + new + piece[end:]
        out.append(piece)
    return "`".join(out)


def split_term(item):
    """A bullet's leading term in capitals, and the rest. None when it does not open with one."""
    m = re.match(rf"^(?:(?:The|A|An)\s+)?({TOKEN}(?:\s+(?:{TOKEN}|\([^)]{{1,40}}\)|and|or|of|vs\.?|/|&|\+))*(?:\s+\([^)]{{1,40}}\))?)", item)
    if not m:
        return None
    term = re.sub(r"\s+(and|or|of|vs\.?|/|&|\+)$", "", m[1])
    rest = item[m.end(1) - (len(m[1]) - len(term)):]
    first = re.match(rf"{TOKEN}", term)[0]
    if len(term) < 3 or (term == first and not (emphatic(first) or rest.lstrip().startswith(":"))):
        return None
    name = " ".join(w.lower() if re.fullmatch(TOKEN, w) and (emphatic(w) or w in SHORT or (is_word(w) and w not in KEEP)) else w for w in term.split())
    rest = rest.lstrip()
    rest = rest[1:].lstrip() if rest.startswith((":", "—", "–")) or rest.startswith("- ") else rest
    if len(rest) < 20:
        return None
    name = name[:1].upper() + name[1:]
    # "SNAPSHOT ISOLATION is not serializable": the explanation reads on from the term, so it says the term again
    if re.match(r"(is|are|was|were|can|cannot|does|do|has|have|runs|builds|keeps|means|uses|tracks|checks|makes|gives|never|only|also|alone)\b", rest):
        rest = f"{name} {rest}"
    return name, rest


def paragraphs(text):
    """A long note as shorter paragraphs: split between sentences, a labelled sentence starting a new one."""
    sentences = SENTENCE.split(text.strip())
    out, now = [], []
    for s in sentences:
        labelled = re.match(r"^(Consequences?|Note|Trade-offs?|Example|Caveats?|In practice|Cost|Costs|Why|How|When|Limits?|Warning):\s", s)
        if now and (labelled or len(now) >= 3 or sum(map(len, now)) + len(s) > 360):
            out.append(" ".join(now))
            now = []
        now.append(s)
    if now:
        out.append(" ".join(now))
    short = []
    for chunk in out:
        if len(chunk) <= 480:
            short.append(chunk)
            continue
        # clauses split at semicolons that are not inside brackets or code
        clauses, depth, code, piece = [], 0, False, ""
        for c in chunk:
            code ^= c == "`"
            depth += c in "([{"
            depth -= c in ")]}"
            piece += c
            if c == ";" and depth <= 0 and not code:
                clauses.append(piece.strip())
                piece = ""
        clauses.append(piece.strip())
        now = ""
        for clause in filter(None, clauses):
            if now and len(now) + len(clause) > 340:
                short.append(now)
                now = ""
            now = f"{now} {clause}".strip()
        short.append(now)
    return [s[:1].upper() + s[1:] if s[:1].isalpha() and k and not re.match(r"[a-z]+[_(:<.]", s) else s for k, s in enumerate(short)]


def label(text):
    return re.sub(r"^(Consequences?|Note|Trade-offs?|Example|Caveats?|In practice|Cost|Costs|Limits?|Warning):\s+(.)", lambda m: f"\x02{m[1]}.\x03 {m[2].upper()}", text)


def list_items(text):
    """The parts of a line that lists things: split at commas and semicolons outside brackets and quotes."""
    parts, depth, quoted, now = [], 0, False, ""
    for c in text:
        depth += c in "([{"
        depth -= c in ")]}"
        quoted ^= c == '"'
        if c in ",;" and depth <= 0 and not quoted:
            parts.append(now)
            now = ""
        else:
            now += c
    parts.append(now)
    parts = [re.sub(r"^(and|or)\s+", "", p.strip()).rstrip(".") for p in parts if p.strip(" .")]
    return parts if len(parts) >= 3 and all(len(p) <= 140 for p in parts) else None


def rich(text):
    """A note as HTML, for the places drawn as rows or labels: code, links and bold; everything else is shown as written."""
    parts = text.replace("```", "'''").split("`")
    if len(parts) % 2 == 0:
        parts = ["'".join(parts)]
    for i, piece in enumerate(parts):
        t = html.escape(piece, quote=False).replace("^", "&#94;").replace("\\", "&#92;").replace("@", "&#64;")
        if i % 2:
            parts[i] = f"<code>{t}</code>"
        else:
            t = re.sub(r"https?://[^\s<>\"]+[^\s<>\".,;:)\]]", lambda m: f'<a href="{m[0]}" target="_blank" rel="noreferrer">{m[0]}</a>', t)
            parts[i] = t
    return "".join(parts).replace("\x02", "<strong>").replace("\x03", "</strong>").replace("\n", " ")


def columns(line):
    """Whether a line is laid out by hand: columns held apart by spaces, or drawing characters."""
    t = line.strip()
    return bool(re.search(r"\S\s{3,}\S", t) or re.search(r"[│├└┌┐┘┤┬┴┼─]{2}|\+--|--\+|\|\s.*\s\|$", t))


def split_at_arrow(entry):
    """An "I want to -> use" entry as its two halves. None when no arrow separates them. An arrow inside brackets
    ("(u8 -> u32)") or in "<->" is part of the words. Where one arrow is set apart in a column, it is the separator."""
    found = []
    for m in re.finditer(r"->(?=\s)", entry):
        before = entry[:m.start()]
        if before.endswith("<") or sum(before.count(c) for c in "([") > sum(before.count(c) for c in ")]"):
            continue
        found.append(m)
    if not found:
        return None
    # set apart: two or more spaces before it, and no other such gap earlier in the line (that would be a second entry)
    column = next((m for m in found if re.search(r"\S\s{2,}$", entry[:m.start()]) and not re.search(r"\S\s{2,}\S", entry[:m.start()])), None)
    at = column or found[0]
    want, use = entry[:at.start()].rstrip(), entry[at.end():].lstrip()
    return (want, use) if want and use else None


def cheat_sheet(lines):
    """"I want to ... -> use ..." entries, two of which may share a line, as a table. None if the lines are something else."""
    entries, loose = [], 0
    for line in lines:
        t = line.strip()
        if not t:
            continue
        parts = [p for p in re.split(r"\s{2,}(?=- )", t) if p]
        for part in parts:
            part = re.sub(r"^-\s+", "", part)
            halves = split_at_arrow(part)
            if halves:
                entries.append(halves)
            elif entries and not part.startswith("-"):
                entries[-1] = (entries[-1][0], entries[-1][1] + " " + part)
            else:
                loose += 1
    if len(entries) < 3 or loose > len(entries) // 4:
        return None
    rows = "".join(f"<tr><td>{rich(w[:1].upper() + w[1:])}</td><td><code>{rich(u).replace('<code>', '').replace('</code>', '')}</code></td></tr>" for w, u in entries)
    return f'<table class="cb-g-cheat"><thead><tr><th>I want to</th><th>Use</th></tr></thead><tbody>{rows}</tbody></table>'


def name_chips(text):
    """A list of names (methods, items) as labels."""
    text = re.sub(r"^\s*\((?:everyday first|most used first)\)\s*", "", text)
    parts, depth, now = [], 0, ""
    for c in text:
        depth += c in "([{<"
        depth -= c in ")]}>"
        if c == "," and depth <= 0:
            parts.append(now)
            now = ""
        else:
            now += c
    parts = [p.strip().rstrip(".") for p in parts + [now] if p.strip(" .")]
    if len(parts) < 4 or sum(len(p) <= 70 for p in parts) < len(parts) * 0.9:
        return None
    return '<ul class="cb-g-names">' + "".join(f"<li><code>{html.escape(p.replace(chr(96), ''), quote=False).replace('^', '&#94;').replace(chr(92), '&#92;').replace('@', '&#64;')}</code></li>" for p in parts) + "</ul>"


def sentence_like(line):
    """Whether an indented line is running text rather than something laid out by hand."""
    t = line.strip()
    if columns(line) and len(re.findall(r"\S\s{3,}\S", t)) > 1:
        return False
    plain = re.sub(r"`[^`]*`", "x", t)
    words = re.findall(r"(?<![\w.:])[A-Za-z][a-z'-]*(?=[\s,.;:)]|$)", plain)
    code = len(re.findall(r"[{}=;|<>\[\]]|::|->|=>|\(\)", plain))
    return len(words) >= 6 and len(words) >= 0.55 * len(plain.split()) and code <= 2 and not re.search(r"[│├└┌┐┘┤┬┴┼─]|\+--|[;{}]\s*$|^(fn|let|use|pub|struct|impl|\$|//|#)\b", t)
