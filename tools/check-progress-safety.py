#!/usr/bin/env python3
"""Fails when a change would lose someone's progress. CI runs it against the previous commit (or a PR's base):

    python3 tools/check-progress-safety.py <base-rev>

1. Every problem id at <base-rev> must still exist, be listed in some problem's `renamed_from`, or be listed in
   `content/retired.txt` (removed on purpose; its progress is deleted at startup). Progress is stored by id
   (`<track code>-<slug>`, or `lc-<slug>` for the DSA lists), so a renamed, moved or deleted problem would otherwise
   vanish from every dashboard.
2. Migrations are append-only: editing or deleting an applied one makes the API refuse to start.
3. A new migration may not destroy stored data: DROP TABLE, DROP COLUMN, TRUNCATE, DELETE FROM, ALTER COLUMN ... TYPE
   or a bare UPDATE of every row fail unless the file says `-- allow-data-loss: <why>` (the owner's rule: progress is
   never lost; add columns and tables, copy, then stop reading the old ones).

The deploy runs the stronger check too (`anneal-api preflight` against the live database), so this is the early
warning, not the only one.
"""
import json
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout


def ids_at(rev):
    """Problem ids at a revision, from the paths: content/tracks/<code>-<name>/problems/<slug>/problem.toml."""
    ids = set()
    for path in git("ls-tree", "-r", "--name-only", rev, "--", "content/tracks").splitlines():
        parts = path.split("/")
        if len(parts) == 6 and parts[3] == "problems" and parts[5] == "problem.toml":
            ids.add(f"{parts[2].split('-')[0]}-{parts[4]}")
    return ids


def dsa_ids(text):
    """The `lc-<slug>` ids in a content/dsa/problems.json."""
    return {p["id"] for p in json.loads(text)["problems"]} if text else set()


def dsa_ids_at(rev):
    try:
        return dsa_ids(git("show", f"{rev}:content/dsa/problems.json"))
    except subprocess.CalledProcessError:
        return set()


def retired():
    path = ROOT / "content/retired.txt"
    lines = path.read_text().splitlines() if path.is_file() else []
    return {l.split("#")[0].strip() for l in lines if l.split("#")[0].strip()}


def current():
    path = ROOT / "content/dsa/problems.json"
    ids, renamed = dsa_ids(path.read_text() if path.is_file() else ""), {}
    for track in sorted((ROOT / "content/tracks").iterdir()):
        if not (track / "track.toml").is_file():
            continue
        code = tomllib.loads((track / "track.toml").read_text())["code"].lower()
        if track.name.split("-")[0] != code:
            sys.exit(f"{track.name}: directory must start with the track code ({code}-...), ids are derived from it")
        for problem in sorted((track / "problems").glob("*/problem.toml")):
            meta = tomllib.loads(problem.read_text())
            pid = f"{code}-{meta['slug']}"
            ids.add(pid)
            for old in meta.get("renamed_from", []):
                renamed[old] = pid
    return ids, renamed


DESTRUCTIVE = [
    (r"\bDROP\s+TABLE\b", "DROP TABLE"),
    (r"\bDROP\s+COLUMN\b", "DROP COLUMN"),
    (r"\bTRUNCATE\b", "TRUNCATE"),
    (r"\bDELETE\s+FROM\b", "DELETE FROM"),
    (r"\bALTER\s+COLUMN\s+\w+\s+(SET\s+DATA\s+)?TYPE\b", "ALTER COLUMN ... TYPE"),
    (r"\bUPDATE\s+\w+\s+SET\b(?![^;]*\bWHERE\b)", "UPDATE without WHERE"),
]


def destructive(sql):
    """The destructive statements of a migration, ignoring comments."""
    code = re.sub(r"--[^\n]*", "", sql)
    return [name for pattern, name in DESTRUCTIVE if re.search(pattern, code, re.I)]


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    base = sys.argv[1]
    problems = []

    ids, renamed = current()
    gone_on_purpose = retired()
    for current_id in sorted(ids & gone_on_purpose):
        problems.append(f"{current_id} is in content/retired.txt but is a current problem; remove it from the list")
    for gone in sorted((ids_at(base) | dsa_ids_at(base)) - ids - renamed.keys() - gone_on_purpose):
        problems.append(
            f"problem {gone} disappeared. If it was renamed or moved, add renamed_from = [\"{gone}\"] to its "
            "problem.toml; progress stored under the old id then moves to it. To remove a problem on purpose (deleting its "
            "progress), list its id in content/retired.txt."
        )

    for line in git("diff", "--name-status", base, "HEAD", "--", "crates/api/migrations").splitlines():
        status, *paths = line.split("\t")
        if not status.startswith("A"):
            problems.append(f"migration {paths[0]} was {'edited' if status == 'M' else 'removed or renamed'}; "
                            "migrations are append-only, add a new one instead")
        else:
            sql = git("show", f"HEAD:{paths[0]}")
            found = destructive(sql)
            if found and "allow-data-loss:" not in sql:
                problems.append(f"migration {paths[0]} can destroy stored data ({', '.join(found)}). Progress must never be lost: "
                                "add columns or tables and copy instead. Only if it is truly intended, say why in a "
                                "`-- allow-data-loss: <why>` comment")

    for p in problems:
        print(f"::error::{p}")
    if problems:
        sys.exit(1)
    print(f"progress safety: ok ({len(ids)} problems, {len(renamed)} renames, compared with {base[:12]})")


main()
