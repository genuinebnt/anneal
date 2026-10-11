#!/usr/bin/env python3
"""Checks the doc comments a learner keeps (docs/COURSE_STANDARDS.md, "Code comments").

    python3 tools/check_code_comments.py [--list] [--src courses/bustub/template/src]

Scans `///` and `//!` lines of the template (what a learner sees; regenerate it with `anneal course template` first) and reports:
  * course words: a stage or module id, "this course", "the tests", "yours", "given code" or "(given)", "exercise", "learner", "hint";
  * history: "BusTub does ...", "unlike C++", "port of ...", "this course leaves out ...";
  * a stub (a function with a `todo!`) that has no doc comment, outside a trait impl (the trait documents those).
Exit status 1 when anything is found. `--list` prints every hit.
"""
import re
import sys
from pathlib import Path

SRC = Path(sys.argv[sys.argv.index("--src") + 1] if "--src" in sys.argv else "courses/bustub/template/src")
ID = re.compile(r"\b(?:r|0[a-d]|[1-4][a-k])-(?:\d\d|c\d)\b")
COURSE = re.compile(
    r"\b(?:modules? [0-9]|modules [0-9]|stages?\b|this course|the course|this exercise|the exercise|exercises?\b|learners?\b|the tests?\b|for tests\b|"
    r"is yours|are yours|yours\b|given code|\(given\)|\bGiven\.|hints?\b|\byou(?:r|'re|'ll)?\b)",
    re.I,
)
HISTORY = re.compile(
    r"\b(?:BusTub(?!Instance)\b|bustub's|C\+\+|C/C\+\+|port of|ported|unlike the original|the original|upstream|this course (?:does|leaves|has)|"
    r"the course (?:does|leaves|uses|keeps)|std::nullopt|CMU|15-445)",
    re.I,
)
FN = re.compile(r"\s*(?:pub(?:\([a-z]+\))?\s+)?(?:const\s+)?(?:async\s+)?fn\s+(\w+)")


def in_trait_impl(lines, i):
    ind = len(lines[i]) - len(lines[i].lstrip())
    for j in range(i - 1, -1, -1):
        l = lines[j]
        if not l.strip():
            continue
        lead = len(l) - len(l.lstrip())
        m = re.match(r"\s*impl\b(.*)", l)
        if m and lead < ind:
            return " for " in m.group(1)
        if lead < ind and re.match(r"\s*(?:pub\s+)?(?:fn|struct|enum|trait|mod)\b", l):
            return False
    return False


def main():
    course, history, undocumented = [], [], []
    for f in sorted(SRC.rglob("*.rs")):
        lines = f.read_text().split("\n")
        for i, l in enumerate(lines):
            t = l.strip()
            if t.startswith("///") or t.startswith("//!"):
                body = re.sub(r"`[^`]*`", "``", t[3:])  # identifiers in backticks are not prose
                if ID.search(t) or COURSE.search(body):
                    course.append((f, i + 1, t))
                if HISTORY.search(re.sub(r"`BusTub[A-Za-z]*`", "``", t[3:])):
                    history.append((f, i + 1, t))
            if "todo!(" in l:
                j = i
                while j >= 0 and not FN.match(lines[j]):
                    j -= 1
                if j < 0 or in_trait_impl(lines, j):
                    continue
                k = j - 1
                while k >= 0 and (lines[k].strip().startswith("#[") or lines[k].strip().startswith("//~")):
                    k -= 1
                if not lines[k].strip().startswith("///") and (f, j + 1) not in {(a, b) for a, b, _ in undocumented}:
                    undocumented.append((f, j + 1, FN.match(lines[j]).group(1)))
    show = "--list" in sys.argv
    for title, rows in (("course-specific words or ids", course), ("history (BusTub, C++, port, the original)", history), ("stub without a doc comment", undocumented)):
        print(f"{len(rows):4}  {title}")
        if show:
            for f, n, t in rows:
                print(f"        {f.relative_to(SRC)}:{n}  {t[:150]}")
    return 1 if (course or history or undocumented) else 0


sys.exit(main())
