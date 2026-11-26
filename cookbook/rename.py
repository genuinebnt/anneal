#!/usr/bin/env python3
"""One-time rename: examples/<chapter>/rNN_name.rs -> examples/<chapter>/name.rs, rewriting references everywhere."""
import glob, os, re, json
from layout import parse, folder_for

files = [f for f in glob.glob("examples/*/*.rs")]
mapping = {}  # old stem -> new stem
for f in files:
    stem = os.path.basename(f)[:-3]
    m = re.match(r"^([rc])(\d+)_(.+)$", stem)
    if m: mapping[stem] = m.group(3)
# collisions: prefix with chapter folder
seen = {}
for old, new in sorted(mapping.items()):
    seen.setdefault(new, []).append(old)
for new, olds in seen.items():
    if len(olds) > 1:
        for old in olds:
            chap = [f for f in files if os.path.basename(f)[:-3] == old][0].split("/")[1]
            mapping[old] = f"{chap}_{new}"
assert len(set(mapping.values())) == len(mapping), "collision"
id_to_new = {}
for old, new in mapping.items():
    m = re.match(r"^([rc])(\d+)_", old); id_to_new[m.group(1) + m.group(2)] = new
json.dump({"names": mapping, "ids": id_to_new}, open("rename_map.json", "w"), indent=1, sort_keys=True)

# 1. move files
for f in files:
    stem = os.path.basename(f)[:-3]
    if stem in mapping:
        os.rename(f, os.path.join(os.path.dirname(f), mapping[stem] + ".rs"))

old_names = sorted(mapping, key=len, reverse=True)
def rewrite_text(text, comment_only_ids):
    for old in old_names:
        text = re.sub(r"(?<![A-Za-z0-9_])%s(?![A-Za-z0-9_])" % re.escape(old), mapping[old], text)
    id_re = re.compile(r"(?<![A-Za-z0-9_])([rc]\d{2,3})(?![A-Za-z0-9_])")
    def sub_ids(s): return id_re.sub(lambda m: id_to_new.get(m.group(1), m.group(1)), s)
    if comment_only_ids:
        return "\n".join(sub_ids(l) if l.lstrip().startswith("//") else l for l in text.split("\n"))
    return sub_ids(text)

# 2. rewrite example sources (comment lines only for bare ids; full names everywhere)
n = 0
for f in glob.glob("examples/*/*.rs"):
    t = open(f).read(); u = rewrite_text(t, True)
    if u != t: open(f, "w").write(u); n += 1
print("rewrote", n, "example files")

# 3. rewrite docs outside examples
docs = glob.glob("../tasks/*.md") + ["../ROADMAP.md", "../CLAUDE.md", "README.md", "TOOLING.md", "SYSTEMS_READING.md",
        os.path.expanduser("~/.claude/projects/-Users-genuinebasilnt-projects-flowforge/memory/project-flowforge-cookbook.md")]
for d in docs:
    if not os.path.exists(d): continue
    t = open(d).read(); u = rewrite_text(t, False)
    if u != t: open(d, "w").write(u); print("updated", d)
print(len(mapping), "renamed")
