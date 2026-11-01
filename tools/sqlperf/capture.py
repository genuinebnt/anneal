#!/usr/bin/env python3
"""Captures every query plan the SQL performance course shows, from a real Postgres, so no plan is ever typed by hand.

Pages declare a plan as a fenced block in their Markdown:

    ```plan id=m3-wrong-order setup=orders-created-customer
    SELECT id, total_cents FROM orders WHERE customer_id = 4242 ORDER BY created_at DESC LIMIT 20;
    ```

`setup` names entries of content/sql/setups.toml (comma-separated, applied in order and undone in reverse). The tool
runs each query once to warm the caches, then takes the JSON and the text plan in the same session (so both show the
steady-state work, not a cold first run), in UTC, and writes content/sql/plans/<id>.json with the plans, the query, the
setups and the server version.

    python3 tools/sqlperf/capture.py --db postgres://anneal@127.0.0.1:5418/sqlperf            # capture everything
    python3 tools/sqlperf/capture.py --db ... --only m3-wrong-order                           # one plan
    python3 tools/sqlperf/capture.py --db ... --check                                         # compare shapes, write nothing

--check re-runs every plan and fails when a plan's shape (node types, relations, indexes) differs from the committed
file: timings and buffer counts vary between runs, the shape must not. The database must hold tools/sqlperf/schema.sql.
"""

import argparse
import json
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTENT = ROOT / "content" / "sql"
FENCE = re.compile(r"^```plan\s+(.*?)\n(.*?)^```\s*$", re.S | re.M)


def psql(db: str, sql: str) -> str:
    out = subprocess.run(["psql", db, "-X", "-q", "-At", "-v", "ON_ERROR_STOP=1", "-c", sql], capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit(f"psql failed: {out.stderr.strip()}\n--- while running:\n{sql}")
    return out.stdout


def plans_in_pages() -> list[dict]:
    found = []
    for page in sorted((CONTENT / "pages").glob("*.toml")):
        data = tomllib.loads(page.read_text())
        texts = [s["body"] for s in data.get("sections", [])] + [c["answer"] for c in data.get("checks", [])]
        for text in texts:
            for m in FENCE.finditer(text):
                opts = dict(re.findall(r"(\w+)=(\S+)", m.group(1)))
                if "id" not in opts:
                    raise SystemExit(f"{page.name}: a plan block has no id")
                setups = [s for s in opts.get("setup", "").split(",") if s]
                found.append({"id": opts["id"], "page": page.stem, "setup": setups, "query": m.group(2).strip()})
    ids = [p["id"] for p in found]
    dupes = {i for i in ids if ids.count(i) > 1}
    if dupes:
        raise SystemExit(f"plan ids used twice: {sorted(dupes)}")
    return found


def shape(node: dict) -> list:
    """What must not change between runs: the operations, what they read, and how they nest."""
    keys = ("Node Type", "Relation Name", "Index Name", "Join Type", "Strategy", "Parent Relationship")
    return [{k: node[k] for k in keys if k in node}, [shape(c) for c in node.get("Plans", [])]]


def capture(db: str, plan: dict, setups: dict) -> dict:
    for name in plan["setup"]:
        if name not in setups:
            raise SystemExit(f"plan {plan['id']}: unknown setup {name!r}")
    apply = "\n".join(setups[n]["sql"] for n in plan["setup"])
    undo = "\n".join(setups[n]["undo"] for n in reversed(plan["setup"]))
    q = plan["query"].rstrip(";")
    # one session: a first run warms the buffer cache and the session's own caches (a cold session also reads the
    # index's metapage and catalog pages), then both plans are taken warm, so they show the steady-state work
    script = (
        "SET TimeZone = 'UTC';\n\\o /dev/null\n"
        f"{q};\n\\o\n\\echo @@JSON\n"
        f"EXPLAIN (ANALYZE, BUFFERS, SETTINGS, FORMAT JSON) {q};\n\\echo @@TEXT\n"
        f"EXPLAIN (ANALYZE, BUFFERS) {q};\n"
    )
    try:
        if apply:
            psql(db, apply)
        out = subprocess.run(["psql", db, "-X", "-q", "-At", "-v", "ON_ERROR_STOP=1"], input=script, capture_output=True, text=True)
        if out.returncode != 0:
            raise SystemExit(f"psql failed on plan {plan['id']}: {out.stderr.strip()}")
    finally:
        if undo:
            psql(db, undo)
    raw = out.stdout.split("@@JSON\n", 1)[1].split("@@TEXT\n", 1)[0]
    text = out.stdout.split("@@TEXT\n", 1)[1]
    tree = json.loads(raw)[0]
    return {
        "id": plan["id"],
        "engine": "postgres",
        "version": psql(db, "SHOW server_version").strip(),
        "query": plan["query"],
        "setup": [{"id": n, "sql": setups[n]["sql"]} for n in plan["setup"]],
        "json": tree,
        "text": text.rstrip("\n"),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--db", required=True, help="a throwaway database holding tools/sqlperf/schema.sql")
    ap.add_argument("--only", help="capture just this plan id")
    ap.add_argument("--check", action="store_true", help="compare shapes with the committed plans, write nothing")
    args = ap.parse_args()
    setups = {s["id"]: s for s in tomllib.loads((CONTENT / "setups.toml").read_text()).get("setup", [])}
    out_dir = CONTENT / "plans"
    out_dir.mkdir(exist_ok=True)
    wanted = [p for p in plans_in_pages() if not args.only or p["id"] == args.only]
    if args.only and not wanted:
        raise SystemExit(f"no plan called {args.only}")
    bad = 0
    for plan in wanted:
        got = capture(args.db, plan, setups)
        path = out_dir / f"{plan['id']}.json"
        if args.check:
            if not path.exists():
                print(f"MISSING {plan['id']}: run capture")
                bad += 1
                continue
            old = json.loads(path.read_text())
            if shape(old["json"]["Plan"]) != shape(got["json"]["Plan"]) or old["query"] != got["query"]:
                print(f"CHANGED {plan['id']}: the plan's shape or query differs from the committed one")
                bad += 1
            else:
                print(f"ok      {plan['id']}")
        else:
            path.write_text(json.dumps(got, indent=1, ensure_ascii=False) + "\n")
            top = got["json"]["Plan"]
            print(f"wrote   {plan['id']}: {top['Node Type']}, {got['json'].get('Execution Time', 0):.3f} ms")
    if args.check:
        known = {p["id"] for p in plans_in_pages()}
        for f in out_dir.glob("*.json"):
            if f.stem not in known:
                print(f"UNUSED  {f.stem}: no page shows it")
                bad += 1
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
