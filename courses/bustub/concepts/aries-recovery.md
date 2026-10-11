---
title: Crash recovery: analysis, redo, undo
summary: The three passes of ARIES-style recovery (who was running, repeat history, roll back the losers), why redo must be idempotent, why undo is logged too, and what a checkpoint buys.
minutes: 14
---
After a crash the database restarts with two things: the **log**, which holds every record that was flushed, and the **data pages**, each of which was either written before the crash or not (any subset of them, in any state of staleness). Recovery has to turn this into the state of exactly the **committed** transactions, no matter which pages made it. The textbook answer is **ARIES** (Mohan et al., 1992), and its idea fits in one sentence: *repeat history, then undo the losers.*

## Three passes

**1. Analysis.** Read the log and decide who was running. A transaction with a `Commit` record is a **winner**; one with an `Abort` record has finished rolling back; every other one that appears in the log is a **loser**: it was active at the crash and must be undone.

**2. Redo: repeat history.** Walk the log from the start (or from the last checkpoint) and re-apply **every** change, in order, whether its transaction committed or not. Why even the losers'? Because the pages may contain any mixture, and "put every change in place in log order" is the one procedure that is correct from every starting point: after it, the pages are exactly as they were at the instant of the crash. That is a clean starting point for the third pass.

Redo must be **idempotent**: applying a record to a page that already has the change must do no harm. In this course that is arranged by describing each change as "put this slot in this state" (the value after the change) rather than "add 5": setting a slot to a value twice is the same as setting it once. ARIES gets the same effect by storing, in each page, the LSN of the last change applied to it and skipping records at or below it.

**3. Undo the losers.** Now roll back every loser, newest change first, by restoring each change's *before* state. Because redo repeated history, the pages hold every loser change, so undoing them is well defined.

```svg
caption: Recovery after a crash, on a seven-record log (a sharp checkpoint at LSN 4, the crash after LSN 7). Green records belong to winners (T1, T3), red ones to the loser T2. Analysis reads forward from the checkpoint to find the loser; redo repeats history after the checkpoint, the loser's changes included; undo walks T2's changes backwards, newest first, and logs every undo. (Animated: the three passes in turn.)
<svg viewBox="0 0 760 350" role="img" aria-label="A log of seven records and the three recovery passes over it: analysis forward from the checkpoint, redo forward, undo backward over the loser's changes">
<style>
@keyframes ar-1{0%,28%{opacity:1}34%,100%{opacity:.18}}
@keyframes ar-2{0%,30%{opacity:.18}36%,60%{opacity:1}66%,100%{opacity:.18}}
@keyframes ar-3{0%,62%{opacity:.18}68%,94%{opacity:1}100%{opacity:.18}}
.ar-1{animation:ar-1 12s ease-in-out infinite}.ar-2{animation:ar-2 12s ease-in-out infinite}.ar-3{animation:ar-3 12s ease-in-out infinite}
</style>
<defs>
<marker id="ar-ab" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--fn)"/></marker>
<marker id="ar-ag" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--grn)"/></marker>
<marker id="ar-ar" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--bad)"/></marker>
</defs>
<rect class="live" x="30" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="75" y="46">LSN 1</text><text class="mid fg" x="75" y="64">T1 set A</text>
<rect class="bad" x="130" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="175" y="46">LSN 2</text><text class="mid fg" x="175" y="64">T2 set B</text>
<rect class="live" x="230" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="275" y="46">LSN 3</text><text class="mid fg" x="275" y="64">T1 commit</text>
<rect class="blue" x="330" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="375" y="46">LSN 4</text><text class="mid fg" x="375" y="64">checkpoint</text>
<rect class="live" x="430" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="475" y="46">LSN 5</text><text class="mid fg" x="475" y="64">T3 set D</text>
<rect class="bad" x="530" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="575" y="46">LSN 6</text><text class="mid fg" x="575" y="64">T2 set C</text>
<rect class="live" x="630" y="30" width="90" height="44" rx="4"/><text class="mid dim sm" x="675" y="46">LSN 7</text><text class="mid fg" x="675" y="64">T3 commit</text>
<text class="mid t-r big" x="742" y="58">✗</text><text class="mid t-r sm" x="742" y="74">crash</text>
<path class="grid dash" d="M375 80 V338"/>
<g class="ar-1">
<text class="t-b big" x="30" y="108">1 · ANALYSIS</text><text class="dim sm" x="150" y="108">who was running when it stopped?</text>
<path class="ln-b" d="M375 124 H664" marker-end="url(#ar-ab)"/>
<text class="dim sm" x="30" y="142">start at the checkpoint: it lists the active transactions, {T2}. Read to the end of the log.</text>
<text class="t-g sm" x="30" y="158">T3 commits at LSN 7 and T1 committed at LSN 3: winners.  T2 never commits: the loser.</text>
</g>
<g class="ar-2">
<text class="t-g big" x="30" y="192">2 · REDO</text><text class="dim sm" x="120" y="192">repeat history, whoever the change belonged to</text>
<path class="ln-g" d="M375 208 H664" marker-end="url(#ar-ag)"/>
<rect class="live" x="466" y="201" width="18" height="14" rx="2"/><text class="mid fg sm" x="475" y="212">5</text>
<rect class="live" x="566" y="201" width="18" height="14" rx="2"/><text class="mid fg sm" x="575" y="212">6</text>
<text class="dim sm" x="30" y="228">re-apply every change after the checkpoint (5 and 6), T2's too. A change already in the page is harmless:</text>
<text class="dim sm" x="30" y="244">a record says "put the slot in this state", so applying it twice is the same as once.</text>
</g>
<g class="ar-3">
<text class="t-r big" x="30" y="278">3 · UNDO</text><text class="dim sm" x="118" y="278">roll back the losers, newest change first</text>
<path d="M664 294 H186" style="stroke:var(--bad);stroke-width:1.6;fill:none" marker-end="url(#ar-ar)"/>
<circle cx="575" cy="294" r="5" class="bad"/><circle cx="175" cy="294" r="5" class="bad"/>
<text class="mid t-r sm" x="575" y="314">undo C (LSN 6)</text><text class="mid t-r sm" x="175" y="314">undo B (LSN 2)</text>
<text class="dim sm" x="30" y="338">each undo is logged too, and an abort record ends T2; a crash during recovery loses no progress.</text>
</g>
</svg>
```

## Undo is logged too

When a transaction is rolled back (at run time or in recovery), each undo step is itself a change to a page, and it must be **logged** like any other: as a change with `before` and `after` swapped (ARIES calls it a *compensation log record*). Three reasons:

- If the machine dies *during* recovery, the next recovery repeats history, including the undo steps already done, and only the remaining ones have to be undone: recovery makes progress.
- A transaction that was rolled back at run time and whose rids are then written by another transaction must not be rolled back *again* by recovery on top of the newer work. With the undo logged and an `Abort` record at the end, redo replays both the changes and their undos, and the transaction counts as finished.
- The log stays the single truth: the state of the pages is always "the log applied in order".

## Checkpoints

Redo from the start of the log gets slower as the log grows. A **checkpoint** bounds it: write every dirty page to disk (a *sharp* checkpoint, which this course uses), then log a `Checkpoint` record that lists the transactions active at that moment. Everything before it is already in the pages, so redo starts at the checkpoint. Losers that were active at the checkpoint still have changes before it; to undo them, recovery must read back as far as their first record (or, as ARIES does, follow a chain of previous LSNs per transaction). Real systems use *fuzzy* checkpoints that do not stop the world, recording which pages were dirty instead of forcing them.

## Invariants: what must always be true

Recovery is correct when these hold. Each one is something a test can check, and each one fails in a recognisable way when it is broken.

- **Write-ahead.** A data page reaches the disk only after every log record that describes a change to it has been flushed. Break it, and a crash can leave a change on the disk with no record of how to undo it.
- **Commit means flushed.** A transaction is reported committed only after its `Commit` record is on the disk. Break it, and a crash forgets a transaction the client was told had succeeded.
- **After redo, the pages are the state at the crash.** Whatever mixture of written and unwritten pages the crash left, redo makes every logged change present, in log order. Redo is idempotent: applying a record twice leaves what applying it once leaves.
- **Every loser is undone once, newest change first,** and every undo step is logged. Break the logging, and a crash during recovery repeats or skips undo work.
- **Recovery can be run again.** Run it twice, or crash in the middle and run it again, and the result is the same as running it once and finishing.
- **Winner or loser, never both.** Every transaction in the log has a `Commit` (winner), an `Abort` with its undo done (finished) or neither (loser).

The crash-at-every-step test of the boss stage checks the first four together: whatever step the crash falls on, the accounts add up to the same total.

## What you can and cannot prove with a test

Recovery is the hardest code in the course to test by example, because the bugs live in the interleaving of three things: when pages were written, how far the log was flushed, and where the crash fell. A **property test** can generate a random run, crash at the end, recover and compare with a model of the committed transactions; a deterministic test can crash at *every* step of a script (the bank-transfer test of the boss stage). Both find bugs that no hand-written scenario does, and both rely on a test double for the disk that can be copied to simulate the crash.

## C++ comparison

| C / C++ | Rust |
|---|---|
| a `LogRecord` class hierarchy with a type tag | an `enum LogRecord` with data, matched exhaustively |
| `LogRecovery::Redo()` loops over `std::unordered_map<txn_id_t, lsn_t>` tables | `HashSet` / `Vec` of transaction ids from `analyse` |
| `memcpy` of record headers | explicit serialization with a length and a checksum |

## In real code

### Using it: analysis, redo and undo on a tiny log

```rust test
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug)]
enum Rec {
    Begin(u32),
    Set { txn: u32, key: char, before: Option<i32>, after: Option<i32> },
    Commit(u32),
    Abort(u32),
}

fn apply(db: &mut BTreeMap<char, i32>, key: char, state: Option<i32>) {
    match state {
        Some(v) => db.insert(key, v),
        None => db.remove(&key),
    };
}

/// Returns the recovered database from whatever the disk had and the durable log.
fn recover(mut disk: BTreeMap<char, i32>, log: &[Rec]) -> BTreeMap<char, i32> {
    // analysis
    let (mut seen, mut finished) = (vec![], HashSet::new());
    for r in log {
        match r {
            Rec::Begin(t) | Rec::Set { txn: t, .. } => if !seen.contains(t) { seen.push(*t) },
            Rec::Commit(t) | Rec::Abort(t) => { finished.insert(*t); }
        }
    }
    let losers: Vec<u32> = seen.into_iter().filter(|t| !finished.contains(t)).collect();
    // redo: repeat history
    for r in log {
        if let Rec::Set { key, after, .. } = r {
            apply(&mut disk, *key, *after);
        }
    }
    // undo the losers, newest first
    for r in log.iter().rev() {
        if let Rec::Set { txn, key, before, .. } = r {
            if losers.contains(txn) {
                apply(&mut disk, *key, *before);
            }
        }
    }
    disk
}

#[test]
fn committed_work_survives_and_the_unfinished_is_rolled_back_whatever_the_disk_held() {
    let log = vec![
        Rec::Begin(1),
        Rec::Set { txn: 1, key: 'a', before: None, after: Some(10) },
        Rec::Begin(2),
        Rec::Set { txn: 2, key: 'b', before: None, after: Some(20) },
        Rec::Commit(1),
    ];
    // any starting state of the disk gives the same result: nothing written, everything written, a stale value
    for disk in [BTreeMap::new(), BTreeMap::from([('a', 10), ('b', 20)]), BTreeMap::from([('a', 99)])] {
        assert_eq!(recover(disk, &log), BTreeMap::from([('a', 10)]));
    }
}

#[test]
fn recovering_twice_is_the_same_as_recovering_once() {
    let log = vec![Rec::Begin(1), Rec::Set { txn: 1, key: 'x', before: None, after: Some(1) }];
    let once = recover(BTreeMap::new(), &log);
    assert_eq!(recover(once.clone(), &log), once);
}
```

### In the exercises

- **4c-05:** `analyse` (winners and losers) and `redo` over the records.
- **4c-06:** `undo`, with each undo logged as a swapped change and `Abort` at the end.
- **4c-07:** the checkpoint and the shorter redo; **4c-08:** a crash at every step of a script and random crashes with a garbage log tail and a crash in the middle of recovery.

### Where it is used

- **PostgreSQL** replays WAL from the last checkpoint after a crash (redo only: its MVCC makes aborted transactions' versions invisible, so there is no undo pass).
- **InnoDB, SQL Server and DB2** use ARIES descendants with all three passes and physiological logging.
- **SQLite** in rollback-journal mode does undo only (it restores the pages from the journal); in WAL mode it does redo only.
- **Filesystems** (ext4's journal, NTFS) apply the same ideas to metadata.
