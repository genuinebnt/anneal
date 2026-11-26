"""Y18: the key-value store capstone, seven steps. Each step's starter holds the earlier steps' reference code."""
from author import write_track
from y17_parts.common import rustfmt
from y18_parts import s1_records, s2_log, s3_tables, s4_store, s5_reopen, s6_compact, s7_test

MODULES = [s1_records, s2_log, s3_tables, s4_store, s5_reopen, s6_compact, s7_test]

problems = [dict(m.PROBLEM, mode="write") for m in MODULES]

write_track(
    "y18-kv-store-capstone", "Y18", "Key-value store capstone", "Y", "sde3", 18,
    "Build a key-value store with a write-ahead log, sorted table files, recovery and compaction, in seven steps. Each step's starter carries the earlier steps.",
    [("on-disk", "Bytes on disk", "medium"), ("the-store", "The store", "hard"), ("trust-it", "Trust it", "hard")],
    problems,
)
rustfmt("y18-kv-store-capstone")
