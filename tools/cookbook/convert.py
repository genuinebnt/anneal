#!/usr/bin/env python3
"""Makes the cookbook pages (content/cookbook/<subject>/<slug>.toml) from the owner's two HTML guides and from the code
in `cookbook/` (tools/cookbook/code_pages.py).

    python3 tools/cookbook/convert.py ~/dbms.html ~/compiler.html

The page body is the Markdown anneal already renders (web/src/components/SdMd.tsx): code figures become fences,
folds become `:::deeper` blocks, side notes become `<aside data-call="…">`, and everything else stays plain HTML.
The guides' exercises and "Done when" checklists are left out (the cookbook is reference only); their reading
links become the page's sources. Which section goes in which chapter is tools/cookbook/outline.json.

Hand-written lessons (content/cookbook/lessons/<subject>/<id>.toml) are never written or deleted here. A lesson replaces
the generated page of the same id when the content is loaded, and takes that page's place from it, so the generated page
is still made. If a run would no longer make a lesson's page in its subject, nothing is changed and the run fails.
"""
import html, json, pathlib, re, sys
from html.parser import HTMLParser

import code_pages

ROOT = pathlib.Path(__file__).resolve().parents[2]
VOID = {"br", "hr", "img", "input", "meta", "link", "col", "wbr", "source", "area", "base"}


class Node:
    def __init__(self, tag, attrs=(), parent=None):
        self.tag, self.attrs, self.kids, self.parent = tag, dict(attrs), [], parent

    @property
    def classes(self):
        return (self.attrs.get("class") or "").split()

    def walk(self):
        yield self
        for k in self.kids:
            if isinstance(k, Node):
                yield from k.walk()

    def text(self):
        return "".join(k if isinstance(k, str) else k.text() for k in self.kids)

    def find(self, tag=None, cls=None):
        return next((n for n in self.walk() if n is not self and (tag is None or n.tag == tag) and (cls is None or cls in n.classes)), None)


class Tree(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = self.cur = Node("root")

    def handle_starttag(self, tag, attrs):
        # an unclosed <p> or <li> ends when the next one starts
        if tag in ("p", "li") and self.cur.tag == tag:
            self.cur = self.cur.parent
        n = Node(tag, attrs, self.cur)
        self.cur.kids.append(n)
        if tag not in VOID:
            self.cur = n

    def handle_startendtag(self, tag, attrs):
        self.cur.kids.append(Node(tag, attrs, self.cur))

    def handle_endtag(self, tag):
        n = self.cur
        while n.parent and n.tag != tag:
            n = n.parent
        if n.parent:
            self.cur = n.parent

    def handle_data(self, data):
        self.cur.kids.append(data)


def plain(s):
    return re.sub(r"\s+", " ", s).strip()


def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")[:60].strip("-")


def esc_text(s):
    # `^[`, `\(` and `@@` mean something to the page renderer: written as references they stay literal text
    return html.escape(s, quote=False).replace("^", "&#94;").replace("\\", "&#92;").replace("@", "&#64;")


CALLS = {"warn": "warning", "rust": "rust", "test": "testing", "prod": "production", "note": "note", "perf": "performance", "why": "why", "idea": "idea", "cairn": "note"}
SVG_NAMES = {"viewbox": "viewBox", "preserveaspectratio": "preserveAspectRatio", "markerwidth": "markerWidth", "markerheight": "markerHeight", "refx": "refX", "refy": "refY"}


class Page:
    def __init__(self, links):
        self.links = links  # guide anchor -> cookbook page id
        self.sources = []

    def dropped(self, n):
        c = n.classes
        return (n.tag in ("input", "script", "style", "button", "label") and n.tag != "label") or n.tag == "input" or \
            (n.tag == "div" and ("ex" in c or "refs" in c or "slice-no" in c)) or (n.tag == "ul" and "done" in c) or (n.tag == "header" and "slice-head" in c) or \
            (n.tag == "h4" and re.match(r"\s*(Exercises|Done when)", n.text()) is not None)

    def sources_from(self, n):
        for a in n.walk():
            if a.tag == "a" and (a.attrs.get("href") or "").startswith("https://"):
                url = a.attrs["href"]
                if url not in [s["url"] for s in self.sources]:
                    self.sources.append({"title": plain(a.text()) or url, "url": url})

    # ---- inline/raw HTML (never contains a fence) ----
    def raw(self, n, in_pre=False):
        if isinstance(n, str):
            t = esc_text(n)
            return t.replace("\n", "&#10;") if in_pre else re.sub(r"\s*\n\s*", " ", t)
        if n.tag == "div" and "refs" in n.classes:
            self.sources_from(n)
        if self.dropped(n):
            return ""
        tag = n.tag
        assert tag not in ("iframe", "object", "embed", "img", "form", "link", "meta", "base"), f"unexpected <{tag}>"
        inner = "".join(self.raw(k, in_pre or tag == "pre") for k in n.kids)
        attrs = {}
        for k, v in n.attrs.items():
            assert not k.startswith("on") and "javascript:" not in (v or "").lower(), f"active attribute {k}"
            if k in ("id", "for", "style") and not self.in_svg(n):
                continue
            attrs[SVG_NAMES.get(k, k)] = v or ""
        if tag == "a":
            href = attrs.get("href", "")
            if href.startswith("#"):
                target = self.links.get(href[1:])
                if not target:
                    return inner
                attrs = {"href": f"/cookbook/p/{target}"}
            elif href.startswith("https://") or href.startswith("http://"):
                attrs = {"href": href, "target": "_blank", "rel": "noreferrer"}
            else:
                return inner
        if "class" in attrs:
            attrs["class"] = " ".join("cb-g-" + c for c in attrs["class"].split())
        if tag in ("details",):
            attrs.pop("open", None)
        a = "".join(f' {k}="{html.escape(v, quote=True).replace(chr(10), " ")}"' for k, v in attrs.items())
        if tag in VOID:
            return f"<{tag}{a}>"
        return f"<{tag}{a}>{inner}</{tag}>"

    def in_svg(self, n):
        while n:
            if n.tag == "svg":
                return True
            n = n.parent
        return False

    # ---- blocks (may emit fences) ----
    def fence(self, pre, title=""):
        code = pre.text().strip("\n")
        out = "out" in pre.classes
        spans = pre.find("span") is not None or any("language-rust" in x.classes for x in pre.walk())
        name = title.lower()
        if out or "grammar" in pre.classes:
            lang = "text"
        elif name.endswith(".rs") or re.search(r"\b(fn|impl|struct|enum|use|let mut|pub)\b", code):
            lang = "rust"
        elif re.match(r"\s*(SELECT|CREATE|INSERT|EXPLAIN|UPDATE|WITH|BEGIN)\b", code):
            lang = "sql"
        elif re.match(r"\s*(\$ |cargo |git |rustup )", code):
            lang = "sh"
        else:
            lang = "rust" if spans else "text"
        title = title or ("Output" if out else "")
        ticks = "```"
        if "\n```" in "\n" + code:
            # a sample of Markdown cannot sit inside a fence: it stays a plain block
            return "<pre>" + esc_text(code).replace("\n", "&#10;") + "</pre>"
        opt = f' title="{title.replace(chr(34), chr(39))}"' if title else ""
        return f"{ticks}{lang}{opt}\n{code}\n{ticks}"

    def has_pre(self, n):
        return n.find("pre") is not None

    def broken_fence(self, para, after):
        """The guide has a few places where Markdown fences were never converted: a paragraph starts with ``` and
        the text that follows the code sits in the next <pre>. This puts both back together as Markdown."""
        text = para.text().strip() + "\n```\n" + (after.text().strip("\n") if after is not None else "")
        out, code, lang = [], None, ""
        for line in text.split("\n"):
            if line.startswith("```"):
                if code is None:
                    first, _, title = line[3:].partition(" ")
                    lang = "rust" if first == "rust" else "text"
                    title = title.strip() or ("Output" if first == "out" else "")
                    out.append(f'```{lang}' + (f' title="{title.replace(chr(34), chr(39))}"' if title else ""))
                    code = []
                else:
                    out += (reindent(code) if lang == "rust" else code) + ["```", ""]
                    code = None
            elif code is not None:
                code.append(line)
            else:
                # prose: a "<" outside a code span would be read as a tag
                parts = line.split("`")
                parts[::2] = [x.replace("<", "&lt;").replace("^[", "&#94;[") for x in parts[::2]]
                out.append("`".join(parts))
        if code is not None:
            out += (reindent(code) if lang == "rust" else code) + ["```"]
        return "\n".join(out).strip()

    def blocks(self, kids, deep=0):
        """Markdown for a list of sibling nodes."""
        out, run = [], []
        used = set()

        def flush():
            s = "".join(run).strip()
            if s:
                out.append(s)
            run.clear()

        for i, n in enumerate(kids):
            if id(n) in used:
                continue
            if not isinstance(n, str) and n.tag == "p" and n.text().lstrip().startswith("```"):
                flush()
                after = next((k for k in kids[i + 1:] if not (isinstance(k, str) and not k.strip())), None)
                after = after if isinstance(after, Node) and after.tag == "pre" else None
                if after is not None:
                    used.add(id(after))
                out.append(self.broken_fence(n, after))
                continue
            if isinstance(n, str):
                if n.strip():
                    run.append(self.raw(n))
                elif run:
                    run.append(" ")
                continue
            if n.tag == "div" and "refs" in n.classes:
                self.sources_from(n)
            if self.dropped(n):
                continue
            c = n.classes
            if n.tag == "figure" and "code" in c:
                flush()
                cap = n.find("figcaption")
                for pre in [x for x in n.walk() if x.tag == "pre"]:
                    out.append(self.fence(pre, plain(cap.text()) if cap else ""))
            elif n.tag == "pre":
                flush()
                out.append(self.fence(n))
            elif n.tag == "aside" and "call" in c:
                flush()
                kind = next((CALLS[x] for x in c if x in CALLS), "note")
                if self.has_pre(n):
                    # a side note with code: the note's words stay in the panel, the code follows as full blocks
                    inner = [k for k in n.kids if isinstance(k, str) or not (k.tag == "pre" or (k.tag == "figure" and "code" in k.classes))]
                    rest = [k for k in n.kids if not isinstance(k, str) and (k.tag == "pre" or (k.tag == "figure" and "code" in k.classes))]
                    out.append(f'<aside data-call="{kind}">' + "".join(self.raw(k) for k in inner).strip() + "</aside>")
                    if rest:
                        out.append(self.blocks(rest, deep))
                else:
                    out.append(f'<aside data-call="{kind}">' + "".join(self.raw(k) for k in n.kids).strip() + "</aside>")
            elif n.tag == "details":
                flush()
                sm = n.find("summary")
                label = plain("".join(k if isinstance(k, str) else ("" if "tier" in k.classes else k.text()) for k in sm.kids)) if sm else "More"
                body = self.blocks([k for k in n.kids if k is not sm], deep + 1)
                if deep == 0 and "notes" in c:
                    out.append(f"### {esc_text(label)}\n\n{body}")
                elif deep == 0:
                    out.append(f":::deeper {label.replace('|', '/')}\n{body}\n:::")
                else:
                    out.append(f"<h4>{esc_text(label)}</h4>\n\n{body}")
            elif n.tag == "div" and (self.has_pre(n) or set(c) & {"body", "prose", "solbody", "notesbody", "sec-head", "tips"}):
                flush()
                out.append(self.blocks(n.kids, deep))
            elif self.has_pre(n):
                flush()
                # a list or paragraph that holds code: split around the code so it renders as a code block
                if n.tag in ("ul", "ol"):
                    for li in [k for k in n.kids if not isinstance(k, str)]:
                        out.append(self.blocks(li.kids, deep))
                else:
                    out.append(self.blocks(n.kids, deep))
            elif n.tag in ("p", "ul", "ol", "table", "h3", "h4", "h5", "div", "svg", "figure", "blockquote", "dl", "section", "header"):
                flush()
                out.append(self.raw(n))
            else:
                run.append(self.raw(n))
        flush()
        return "\n\n".join(x for x in out if x)


def reindent(lines):
    """Indentation was lost in the broken blocks: it is rebuilt from the braces."""
    depth, out = 0, []
    for line in lines:
        t = line.strip()
        lead = len(t) - len(t.lstrip("})]"))
        out.append("    " * max(0, depth - lead) + t)
        bare = re.sub(r'"(?:\\.|[^"\\])*"', '""', t)
        depth = max(0, depth + sum(bare.count(c) for c in "{([") - sum(bare.count(c) for c in "})]"))
    return out


def sections_of(file):
    src = pathlib.Path(file).expanduser().read_text()
    # HTML tag names in the guides are lower case, so `Res<String>` or `Vec<u8>` left unescaped is code, not a tag
    src = re.sub(r"<(/?)((?:[A-Z]\w*|[ui](?:8|16|32|64|128|size)|f32|f64|str|bool|char)(?:<[^<>\n]*>)?)>", lambda m: f"&lt;{m[1]}{html.escape(m[2])}&gt;", src)
    tree = Tree()
    tree.feed(src)
    found = {}
    for sec in tree.root.walk():
        if sec.tag != "section":
            continue
        h2 = sec.find("h2")
        if h2 is None:
            continue
        found.setdefault(plain(h2.text()), (sec, h2))
    return found


def convert(sec, h2, links):
    page = Page(links)
    anc = h2
    while anc.parent is not sec:
        anc = anc.parent
    meta, summary, lede = "", [], ""
    head = anc if anc.tag in ("header", "div") else None
    m = sec.find("p", "meta")
    if m:
        meta = plain(m.text())
    if head is not None and "sec-head" in head.classes:
        p = head.find("p")
        lede = plain(p.text()) if p else ""
    body = []

    def flatten(kids):
        for k in kids:
            if not isinstance(k, str) and k.tag == "div" and "body" in k.classes:
                flatten(k.kids)
            elif k is h2 or k is anc or (not isinstance(k, str) and k is m):
                continue
            else:
                body.append(k)

    flatten(sec.kids)
    groups = [("", [])]
    for k in body:
        if not isinstance(k, str) and k.tag == "h3":
            groups.append((plain("".join(x if isinstance(x, str) else ("" if "src" in x.classes else x.text()) for x in k.kids)), []))
        else:
            groups[-1][1].append(k)
    out, seen = [], set()
    for heading, kids in groups:
        md = fix_drawings(page.blocks(kids))
        if not md.strip():
            continue
        heading = heading or "Overview"
        n = 2
        base = heading
        while heading in seen:
            heading = f"{base} ({n})"
            n += 1
        seen.add(heading)
        out.append({"heading": heading, "body": md})
    if not lede:
        lede = re.sub(r"^Lecture #\d+\s*·\s*(?:Textbook[^·]*·\s*)?", "", meta)
    return dict(meta=meta, lede=lede, sections=out, sources=page.sources)


def toml_str(s):
    return json.dumps(s, ensure_ascii=False).replace("\x7f", "\\u007f")


def toml_ml(s):
    return '"""\n' + s.replace("\\", "\\\\").replace('"""', '""\\"') + '\n"""'


def write_page(out, subject, pid, fields, sections, sources, code=()):
    lines = [f"{k} = {v if isinstance(v, int) else toml_str(v)}" for k, v in fields.items() if v is not None and k != "subject"]
    if code:
        lines.append("code = [" + ", ".join(toml_str(c) for c in code) + "]")
    for x in sections:
        lines += ["", "[[sections]]", f"heading = {toml_str(x['heading'])}", f"body = {toml_ml(x['body'])}"]
    for x in sources:
        lines += ["", "[[sources]]", f"title = {toml_str(x['title'])}", f"url = {toml_str(x['url'])}"]
    d = out / subject
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{pid}.toml").write_text("\n".join(lines) + "\n")


# drawings whose words run out of their box or off the drawing at the app's fonts: no stylesheet can move them
DRAWING_FIXES = (
    ('<text x="520" y="62" class="cb-g-mut">inner: k keys, k+1 children; child i holds keys in [key_i, key_{i+1})</text>',
     '<text x="592" y="26" class="cb-g-mut">inner: k keys, k+1 children;</text><text x="592" y="42" class="cb-g-mut">child i holds keys in [key_i, key_{i+1})</text>'),
    *((f'<rect x="{x}" y="164" width="170" height="22" rx="3" class="cb-g-box"></rect>',
       f'<rect x="{x}" y="164" width="205" height="22" rx="3" class="cb-g-box"></rect>') for x in (60, 320, 580)),
)


def fix_drawings(body):
    for old, new in DRAWING_FIXES:
        body = body.replace(old, new)
    return body


# folders of content/cookbook that are written by hand: nothing in them is ever deleted or written
BY_HAND = ("samples", "lessons")


def stranded_lessons(out, planned):
    """The lessons whose page this run would not make. `planned` holds (subject, id) pairs."""
    lessons = out / "lessons"
    return [f for f in sorted(lessons.rglob("*.toml")) if (f.parent.name, f.stem) not in planned or f.parent.parent != lessons]


def main(db, co):
    outline = json.loads((ROOT / "tools/cookbook/outline.json").read_text())
    files = {"db": sections_of(db), "co": sections_of(co)}
    book = ROOT / "cookbook"
    samples = ROOT / "content/cookbook/samples"
    plan, ids, links = [], {p.name for p in samples.iterdir()} if samples.is_dir() else set(), {}

    def fresh(base, hint):
        pid, n = base, 2
        if pid in ids:
            pid = f"{base}-{hint}"
        while pid in ids:
            pid, n = f"{base}-{hint}-{n}", n + 1
        ids.add(pid)
        return pid

    items = [(mod, ci, ch, kind, x) for mod in outline for ci, ch in enumerate(mod["chapters"])
             for kind, key in (("guide", "sections"), ("code", "recipes"), ("crate", "crates")) for x in ch.get(key, [])]
    # the guides name their pages first, so adding code never moves a guide page's address
    for kind_first in ("guide", "code", "crate"):
        for order, (mod, ci, ch, kind, x) in enumerate(items):
            if kind != kind_first:
                continue
            if kind == "guide":
                key, title = x
                sec, h2 = files[key][title]
                pid = fresh(slug(title), {"db": "db", "co": "compilers"}[key])
                for n in sec.walk():
                    if "id" in n.attrs:
                        links.setdefault((key, n.attrs["id"]), pid)
                plan.append((order, mod, ci, ch, kind, (key, title, sec, h2), pid))
            else:
                stem = pathlib.PurePosixPath(x).stem
                pid = fresh(slug(stem), "code" if kind == "code" else "project")
                plan.append((order, mod, ci, ch, kind, x, pid))
    out = ROOT / "content/cookbook"
    stranded = stranded_lessons(out, {(t[1]["subject"], t[-1]) for t in plan})
    if stranded:
        sys.exit("nothing was changed: these lessons would have no page to replace (was a guide title or a file renamed, or a page moved to another subject?)\n"
                 + "\n".join(f"  {f.relative_to(ROOT)}" for f in stranded))
    for old in out.glob("*/*.toml"):
        if old.parent.name not in BY_HAND:
            old.unlink()
    made = {"guide": 0, "code": 0, "crate": 0}
    for order, mod, ci, ch, kind, x, pid in sorted(plan, key=lambda t: t[0]):
        fields = dict(subject=mod["subject"], module=mod["module"], module_order=mod["order"], chapter=ch["title"], chapter_order=ci + 1, order=order + 1)
        if kind == "guide":
            key, title, sec, h2 = x
            page = convert(sec, h2, {a: p for (k, a), p in links.items() if k == key})
            assert page["sections"], f"{title}: nothing left after conversion"
            lede = page["lede"] or plain(re.sub(r"<[^>]+>", "", html.unescape(page["sections"][0]["body"])))[:220]
            fields = dict(title=title, **fields, kind="guide", lede=lede, origin={"db": "Database systems guide", "co": "Compiler guide"}[key])
            write_page(out, mod["subject"], pid, fields, page["sections"], page["sources"])
        elif kind == "code":
            n = code_pages.read_notes((book / x).read_text(), pathlib.PurePosixPath(x).stem)
            fields = dict(title=n["title"], **fields, kind=n["kind"], lede=n["lede"], level=n["level"])
            write_page(out, mod["subject"], pid, fields, n["sections"], n["sources"], [x])
        else:
            n = code_pages.read_crate(book, x)
            fields = dict(title=n["title"], **fields, kind="project", lede=n["lede"])
            write_page(out, mod["subject"], pid, fields, n["sections"], [], n["files"])
        made[kind] += 1
    print(f"pages written to {out}: {made['guide']} from the guides, {made['code']} examples, {made['crate']} projects")


if __name__ == "__main__":
    main(*sys.argv[1:3])
