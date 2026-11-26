"""Pages for the code in `cookbook/`: one per example file and one per multi-file project. The code is not copied into
the page: the page names the files (`code = [...]`, relative to `cookbook/`) and its text is the file's own notes."""
import pathlib, re

import notes as layout

KINDS = {"RECIPE": "recipe", "CAPSTONE": "capstone", "INTERVIEW": "questions"}
# notes that start with one of these, in capitals, are sections of their own
HEADINGS = {"COMMON USES": "Common uses", "METHODS": "Methods", "WHAT": "What it is", "WHY": "Why it matters", "HOW": "How it works",
            "TRY NEXT": "Try next", "DOCS": "Documentation", "DEFINED IN": "Where it is defined", "CRATES": "Crates",
            "PITFALLS": "Pitfalls", "VERIFICATION": "Verification", "VERIFIED": "Verification", "CHECKS": "What is checked",
            "CHEAT SHEET": "Cheat sheet", "NOT COVERED": "Not covered"}
# the file's suggestions of things to build: exercises, which a reference page leaves out (they stay in the code)
EXERCISES = {"Try next"}
NAMES = {"sql": "SQL", "btree": "B-tree", "lsm": "LSM", "mvcc": "MVCC", "wal": "WAL", "api": "API", "http": "HTTP", "tcp": "TCP", "udp": "UDP",
         "dns": "DNS", "simd": "SIMD", "ffi": "FFI", "std": "std", "qa": "questions and answers", "dp": "DP", "io": "I/O", "crdt": "CRDT",
         "raft": "Raft", "jwt": "JWT", "tls": "TLS", "ast": "AST", "ir": "IR", "vm": "VM", "jit": "JIT", "gc": "GC", "lru": "LRU", "rcu": "RCU",
         "cow": "Cow", "ann": "ANN", "hnsw": "HNSW", "bm25": "BM25", "fst": "FST", "ecs": "ECS", "png": "PNG", "ppm": "PPM", "csv": "CSV",
         "json": "JSON", "cli": "CLI", "oltp": "OLTP", "olap": "OLAP", "cdc": "CDC", "ot": "OT", "swim": "SWIM", "2pc": "2PC", "toowned": "ToOwned", "asref": "AsRef", "tryfrom": "TryFrom", "fromstr": "FromStr", "oncecell": "OnceCell", "hashmap": "HashMap", "btreemap": "BTreeMap", "vecdeque": "VecDeque", "pathbuf": "PathBuf", "osstr": "OsStr", "cstr": "CStr", "refcell": "RefCell", "phantomdata": "PhantomData", "maybeuninit": "MaybeUninit"}


def humanize(stem):
    words = [NAMES.get(w, w) for w in stem.split("_")]
    s = " ".join(words)
    return s[:1].upper() + s[1:]


def inline(text):
    """A note as Markdown that shows what was written: nothing in it may read as a tag, a heading or emphasis."""
    parts = text.replace("```", "'''").split("`")
    if len(parts) % 2 == 0:          # an unmatched backtick would swallow the rest
        parts = ["'".join(parts)]
    for i in range(0, len(parts), 2):
        t = parts[i].replace("&", "&amp;").replace("<", "&lt;")
        t = t.replace("\\", "&#92;").replace("^", "&#94;").replace("*", "&#42;").replace("|", "&#124;").replace("~", "&#126;")
        t = re.sub(r"(?<![A-Za-z0-9])_|_(?![A-Za-z0-9])", "&#95;", t)
        t = t.replace("[!", "&#91;!").replace("$$", "&#36;&#36;")
        t = re.sub(r"(https?://[^\s,)\]]+)", lambda m: "<" + m[1].replace("&#95;", "_").replace("&#126;", "~").replace("&amp;", "&").rstrip(".;:") + ">" + m[1][len(m[1].rstrip(".;:")):], t)
        parts[i] = t
    out = "`".join(parts)
    out = re.sub(r"^(\s*)([#>:+=-])", lambda m: m[1] + f"&#{ord(m[2])};", out)
    return out.replace("\x02", "**").replace("\x03", "**")


def prose(lines):
    """Note lines as Markdown: paragraphs, bullet lists (their continuation lines joined) and indented blocks as text."""
    out, para, items, pre = [], [], [], []

    def flush():
        if para:
            for chunk in layout.paragraphs(" ".join(x.strip() for x in para)):
                out.append(inline(layout.label(layout.calm(chunk))))
        if items:
            terms = [layout.split_term(t) for _, t in items]
            if sum(t is not None for t in terms) >= max(1, (len(items) + 1) // 2):
                # rows: each bullet's term beside what is said about it
                rows = []
                for (_, text), term in zip(items, terms):
                    name, rest = term if term else ("", text)
                    body = "".join(f"<p>{layout.rich(layout.label(layout.calm(c[:1].upper() + c[1:] if k == 0 else c)))}</p>" for k, c in enumerate(layout.paragraphs(rest)))
                    rows.append(f'<div class="cb-g-term"><div class="cb-g-name">{layout.rich(name)}</div><div class="cb-g-says">{body}</div></div>')
                out.append('<div class="cb-g-terms">' + "".join(rows) + "</div>")
            else:
                # a long bullet is split between sentences into paragraphs of the same bullet
                out.append("\n".join(f"{mark} " + "\n\n   ".join(inline(layout.calm(c)) for c in layout.paragraphs(t)) for mark, t in items))
        if pre:
            indent = min((len(x) - len(x.lstrip()) for x in pre if x.strip()), default=0)
            out.append("```text\n" + "\n".join(x[indent:].replace("```", "'''") for x in pre).rstrip() + "\n```")
        para.clear(), items.clear(), pre.clear()

    for line in lines:
        bullet = re.match(r"^\s{0,3}([*•-]|\d{1,2}[.)])\s+(.*)$", line)
        if not line.strip():
            flush()
        elif not pre and not bullet and (para or items) and layout.sentence_like(line):
            if items:
                items[-1] = (items[-1][0], items[-1][1] + " " + line.strip())
            else:
                para.append(line)
        elif layout.columns(line) and not (bullet and "->" not in line and not pre) and not layout.sentence_like(line):
            if para or items:
                flush()
            pre.append(line)
        elif bullet:
            if para or pre:
                flush()
            items.append((bullet[1] if bullet[1][0].isdigit() else "-", bullet[2]))
        elif items and line.startswith("  "):
            items[-1] = (items[-1][0], items[-1][1] + " " + line.strip())
        elif line.startswith("   "):
            if items or para:
                flush()
            pre.append(line)
        else:
            if items or pre:
                flush()
            para.append(line)
    flush()
    return "\n\n".join(out)


def header_of(text):
    lines, started = [], False
    for line in text.split("\n"):
        if line.startswith("//!"):
            started = True
            lines.append(line[4:] if line.startswith("//! ") else line[3:])
        elif started and not (line.startswith("#![") or not line.strip()):
            break
    return lines


def read_notes(text, stem):
    notes = header_of(text)
    kind, title, lede, level, run, rest = "recipe", humanize(stem), "", None, "", []
    i = 0
    first = re.match(r"^(RECIPE|CAPSTONE|INTERVIEW)\b[^:]*:\s*(.*)$", notes[0]) if notes else None
    if first:
        kind, lede, i = KINDS[first[1]], first[2], 1
    while i < len(notes) and notes[i].strip() and not re.match(r"^(LEVEL|Use in):|^[A-Z][A-Z ]{2,}[:(]", notes[i]):
        lede += " " + notes[i].strip()
        i += 1
    for line in notes[i:]:
        if line.startswith("LEVEL:") and level is None:
            level = line.split(":", 1)[1].strip()
        elif line.startswith("Use in:") and not run:
            use, _, run = line[len("Use in:"):].partition("Run:")
            run = run.strip()
            if use.strip():
                rest.append("USE IT IN: " + use.strip())
        else:
            rest.append(line)
    lede = lede.strip() or title
    # a short name from the description when it starts with one, else from the file's name
    head = re.split(r":\s|\s[-–—]\s|\.\s|\s\(", lede)[0].strip().rstrip(".")
    if first and 4 <= len(head) <= 64 and head.count(",") <= 1:
        title = head[:1].upper() + head[1:]
    elif stem.startswith("std_"):
        title = humanize(stem[4:]) + " (std)"
    # a description that begins by repeating the name says only the rest
    rest_of = re.match(re.escape(title) + r"\s*[:.–—-]+\s+(.+)$", lede, re.I)
    if rest_of:
        lede = rest_of[1][:1].upper() + rest_of[1][1:]
    sections, name, block = [], "Design notes", []

    def close():
        if name in EXERCISES:
            block.clear()
            return
        body = prose(block)
        if name == "Cheat sheet":
            body = layout.cheat_sheet(block) or body
        if name == "Methods":
            body = layout.name_chips(" ".join(x.strip() for x in block if x.strip())) or body
        if name == "Documentation":
            urls = re.findall(r"https?://[^\s,)]+[^\s,).;]", " ".join(block))
            if urls:
                body = '<ul class="cb-g-reading">' + "".join(f"<li>{layout.rich(u)}</li>" for u in urls) + "</ul>"
        joined = " ".join(x.strip() for x in block if x.strip())
        plain_lines = not any(re.match(r"^\s{0,3}([*•-]|\d{1,2}[.)])\s", x) or x.startswith("   ") for x in block)
        if plain_lines and name in ("Common uses", "Where you would use it"):
            items = layout.list_items(joined)
            if items:
                body = '<ul class="cb-g-uses">' + "".join(f"<li>{layout.rich(layout.calm(i))}</li>" for i in items) + "</ul>"
        if plain_lines and name == "Further reading":
            works = [w.strip(" ,;.") for w in re.split(r"(?<=\))\s*[,;.]\s+|(?<=\.)\s+(?=[\"A-Z])|\s+,\s+|(?<=/)\s*,\s+|;\s+", joined) if w.strip(" ,;.")]
            if len(works) > 1:
                body = '<ul class="cb-g-reading">' + "".join(f"<li>{layout.rich(w)}</li>" for w in works) + "</ul>"
        if body.strip():
            same = next((s for s in sections if s["heading"] == name), None)
            if same:
                same["body"] += "\n\n" + body
            else:
                sections.append({"heading": name, "body": body})
        block.clear()

    sources = []
    for line in rest:
        key = re.match(r"^([A-Z][A-Z0-9 ]{2,22}?)(?:\s*\([^)]*\))?(?::\s*(.*))?$", line)
        if key and key[2] is None and key[1].strip() not in ("CHEAT SHEET", "TRY NEXT"):
            key = None
        if key and (key[1].strip() in HEADINGS or key[1].strip() == "USE IT IN"):
            close()
            name = HEADINGS.get(key[1].strip(), "Where you would use it")
            if key[2]:
                block.append(key[2])
            elif line.rstrip().endswith(")") and name == "Cheat sheet":
                pass
        elif re.match(r"^(Read|Further reading):", line):
            close()
            name = "Further reading"
            block.append(line.split(":", 1)[1].strip())
        elif name in ("Cheat sheet", "Try next") and not line.strip():
            block.append(line)
        elif name != "Design notes" and not line.strip():
            block.append(line)
            close()
            name = "Design notes"
        else:
            block.append(line)
    close()
    for url in re.findall(r"https?://[^\s,)\]>`\"]+", "\n".join(notes)):
        url = url.rstrip(".;:")
        if len(url) > 12 and not re.search(r"localhost|127\.0\.0\.1|example\.(com|org)|<|\{", url) and url not in [s["url"] for s in sources]:
            short = re.sub(r"^https?://(www\.)?", "", url).rstrip("/")
            sources.append({"title": short if len(short) <= 80 else short[:77] + "…", "url": url})
    if not sections:
        sections = [{"heading": "Design notes", "body": inline(lede)}]
    return dict(kind=kind, title=title, lede=lede, level=level, run=run, sections=sections, sources=sources)


def crate_files(root, name):
    base = root / "crates" / name
    keep = []
    for p in sorted(base.rglob("*")):
        rel = p.relative_to(base)
        if p.is_file() and "target" not in rel.parts and (p.suffix in (".rs", ".toml", ".md", ".sh", ".h", ".c") ) and p.name != "Cargo.lock" and p.stat().st_size <= 256 * 1024:
            keep.append(rel.as_posix())
    # the code first, then what describes it
    first = ["src/lib.rs", "src/main.rs", "README.md", "Cargo.toml"]
    keep.sort(key=lambda f: (first.index(f) if f in first else len(first), f))
    return [f"crates/{name}/{f}" for f in keep]


FENCE_LANGS = {"rust": "rust", "rs": "rust", "sh": "sh", "bash": "sh", "shell": "sh", "console": "sh", "json": "json", "sql": "sql", "python": "python", "http": "http", "text": "text", "": "text"}


def markdown(readme):
    """A README as page Markdown: its own title is dropped, headings move below the page's, fences take a language the page
    knows, links to files in the repository become their text, and what the page would read as its own syntax stays literal."""
    out, fence = [], False
    for line in re.sub(r"\A\s*#\s+.*\n", "", readme).split("\n"):
        mark = re.match(r"^(\s*)(```+|~~~+)\s*([\w+-]*)", line)
        if mark:
            fence = not fence
            out.append("```" + FENCE_LANGS.get(mark[3].lower(), "text") if fence else "```")
            continue
        if fence:
            out.append(line.replace("```", "'''"))
            continue
        head = re.match(r"^(#{1,6})\s+(.*)$", line)
        if head:
            line = "#" * min(5, max(3, len(head[1]) + 1)) + " " + head[2]
        line = re.sub(r"\[([^\]]+)\]\((?!https?://)[^)]*\)", r"\1", line)
        parts = line.split("`")
        for i in range(0, len(parts), 2):
            parts[i] = re.sub(r"<(?!https?://)", "&lt;", parts[i]).replace("^[", "&#94;[").replace("\\(", "&#92;(").replace("$$", "&#36;&#36;").replace("[!", "&#91;!")
        line = "`".join(parts)
        if line.startswith(":::"):
            line = "&#58;" + line[1:]
        out.append(line)
    if fence:
        out.append("```")
    return "\n".join(out).strip()


def read_crate(root, name):
    base = root / "crates" / name
    files = crate_files(root, name)
    notes = []
    for f in ("src/lib.rs", "src/main.rs"):
        if (base / f).exists() and header_of((base / f).read_text()):
            notes = header_of((base / f).read_text())
            break
    readme = (base / "README.md").read_text() if (base / "README.md").exists() else ""
    sections = []
    if notes:
        sections.append({"heading": "Design notes", "body": prose(notes)})
    if readme:
        body = markdown(readme)
        if body.strip():
            sections.append({"heading": "About the project", "body": body})
    layout = "\n".join(f[len(f"crates/{name}/"):] for f in files)
    sections.append({"heading": "What is in it", "body": f"```text title=\"{name}\"\n{layout}\n```"})
    para = next((x.strip() for x in re.split(r"\n\s*\n", re.sub(r"^#.*$", "", readme, flags=re.M)) if x.strip() and not x.strip().startswith(("```", "|", "-"))), "")
    lede = re.sub(r"\s+", " ", para or " ".join(notes[:3]).strip() or f"The {name} project: {len(files)} files.")
    lede = re.sub(r"[*`]", "", lede)
    if len(lede) > 300:
        cut = lede[:300].rsplit(" ", 1)[0]
        lede = cut.rstrip(",;:(") + "…"
    return dict(title=f"{name.removeprefix('cb-').replace('-', ' ').capitalize()} project", lede=lede, sections=sections, files=files)
