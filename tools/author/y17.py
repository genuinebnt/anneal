"""Y17: storage engine parts. Preparation for the key-value store capstone (Y18).

The owner's rule: ideas may overlap, a whole assessment may not. For each new problem here and in Y18, the closest
other problem (judged by statement and tests) and what this one assesses that it does not:

| new problem | closest | assessed only here |
|---|---|---|
| y17-append-that-fails | y9-kv-store-wal | failing writes and syncs: undo the partial write, never retry a sync, append-mode file |
| y17-after-the-power-cut | y9-kv-store-wal | every disk state when bytes and names become durable separately |
| y17-read-from-the-back | l3-binary-frame-parser, f2-slotted-page | positional reads from a footer, read budget, short reads, hostile directory |
| y17-save-without-tearing | none writes real files | a publish protocol checked at every crash point and failing call; safe recover |
| y17-one-block-is-enough | s3-keep-sorted | exactly which blocks first keys allow, counted |
| y17-fix-filter-after-restart | f4-bloom-filter | stored bytes and a stable hash, hostile load (not sizing or union) |
| y17-unsaved-changes | y12-mvcc-snapshot-isolation, s4-btreemap-ranges | lazy ordered paging over two layers, call budget, revert against delete |
| y17-which-delete-markers-can-go | s6-merge-sorted-streams, y9-kv-store-wal | which markers may go when only some neighbouring runs are merged |
| y18-records-and-frames | y9-kv-store-wal | a strict codec; a torn end told apart from corruption; zero tail |
| y18-log-on-disk | y17-append-that-fails, y9-kv-store-wal | open, repair and reset a real file; shares the failed-append rollback with y17 |
| y18-table-files | y17-read-from-the-back, y17-one-block-is-enough | the whole format byte for byte, atomic publish, index validation |
| y18-memtable-and-flush | y17-unsaved-changes | tables and log together: flush order, failed flush, read errors |
| y18-reopen-after-a-crash | y17-save-without-tearing | log and tables together after each crash; shares the file-name traps |
| y18-compact-safely | y17-which-delete-markers-can-go | removal order and a consistent open store when a removal fails |
| y18-test-your-store | y5-catch-the-broken-queue | an oracle that knows what a torn write may lose |
"""
from author import write_track
from y17_parts.common import rustfmt
from y17_parts import p1_append, p2_power, p3_save, p4_back, p5_block, p6_filter, p7_unsaved, p8_markers

MODULES = [p1_append, p2_power, p4_back, p3_save, p5_block, p6_filter, p7_unsaved, p8_markers]

problems = [dict(m.PROBLEM, mode=m.PROBLEM.get("mode", "write")) for m in MODULES]

write_track(
    "y17-storage-engine-parts", "Y17", "Storage engine parts", "Y", "sde3", 17,
    "Preparation for the key-value store capstone: files that survive failures and power cuts, sorted blocks, saved filters and layers of changes.",
    [("files-that-survive", "Files that survive", "medium"), ("sorted-data", "Sorted data you can search", "medium"), ("many-layers", "Many layers, one answer", "hard")],
    problems,
)
rustfmt("y17-storage-engine-parts")
