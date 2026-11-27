"""Y31: preparation for the mini Redis capstone (Y32).

The owner's rule: ideas may overlap, a whole assessment may not. For each problem here, the closest other problem
(judged by statement and tests) and what this one assesses that it does not:

| problem | closest | assessed only here |
|---|---|---|
| y31-a-value-inside-a-value | y6-wasm-binary-decoder, b7-requests-in-pieces | nested values from a prefix: need more against never, at the exact byte, with depth and item limits |
| y31-who-is-in-40th-place | f4-range-queries, s5-indexed-priority-queue | rank and select by order over changing keys, no fixed key range |
| y31-which-names-fit | y8-regex-nfa-matcher | wildcard rules over bytes without compiling an automaton, constant memory |
| y31-throw-out-the-stale | f4-lru-cache-arena, m2-rate-limiter | constant-time random pick under removal, and a sampling stop rule |
| y31-hold-requests-until-something-arrives | c1-fair-semaphore, s5-indexed-priority-queue | waiting on several items at once, cleanup in every queue, deadline order |
| y31-tell-everyone-without-waiting | b7-turn-work-away, c2-bounded-channel | one to many, per-member bound, eviction against a vanished reader |
| y31-write-down-what-happened | y9-kv-store-wal, y18-records-and-frames | logging effects of dice and time, and units that replay whole |
| y31-shrink-a-log-while-it-is-written | y18-compact-safely, y17-save-without-tearing | a rewrite in two halves while writes go on: the tail |
| y31-one-owner-one-sync | c4-drain-with-deadline, c2-thread-pool | batching what already waits, replies after the commit, failure of a batch |
| y31-answer-while-still-listening | c4-fix-cancel-safe-reader, b7-many-requests-one-connection | reading while a reply is pending, bounded input, interruptible writes, push between replies |

The Y32 steps against the closest other problems: step 1 against y31-a-value-inside-a-value and b7-requests-in-pieces
(a real protocol with its own error points and an encoder); steps 2 to 4 against s4 and f4 problems (the command
contract); step 5 against y12-mvcc-snapshot-isolation (watch and queue, not versions); step 6 against
y31-hold-requests (held commands and wake-up order, not the waiting room); step 7 against y31-tell-everyone (no
queues: modes, patterns and frame order); step 8 against y31-write-down-what-happened and y9-kv-store-wal (Redis's
records and replay); step 9 against y18-records-and-frames, y17-save-without-tearing and y31-shrink-a-log (this
format, recovery's order); step 10 against y31-answer-while-still-listening and y31-one-owner (the assembled server).
"""
import importlib

from author import write_track
from y17_parts.common import rustfmt
import y31_parts

NAMES = ["p01_nested", "p03_glob", "p04_stale", "p02_ranked", "p06_fanout", "p05_waiting", "p07_effects", "p08_rewrite", "p09_owner", "p10_connection"]

problems = []
for name in NAMES:
    p = dict(importlib.import_module(f"y31_parts.{name}").PROBLEM, mode="write")
    p.setdefault("use", "use solution::*;\n")
    problems.append(p)

write_track(
    "y31-redis-preparation", "Y31", "Mini Redis: preparation", "Y", "sde3", 31,
    "Preparation for the mini Redis capstone: nested messages, ranked sets and patterns, expiry, waiting and fan-out, logs that replay, and connections served over Tokio.",
    [("messages", "Messages in pieces", "medium"), ("data", "Data a server keeps", "hard"), ("waiting", "Waiting and telling", "hard"), ("disk", "A log you can replay", "hard"), ("network", "Over the network", "hard")],
    problems,
)
rustfmt("y31-redis-preparation")
