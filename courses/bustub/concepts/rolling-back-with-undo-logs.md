---
title: Rolling back with undo logs: how abort restores the database
summary: Why a transaction that updates tuples in place can be undone from the same undo logs readers use, what to restore for each kind of change, and why nobody else can be disturbed.
minutes: 7
---
A transaction that updates tuples **in place** has already overwritten the table's bytes when it decides (or is forced) to abort. Without a copy of what was there, the old values would be lost. The copy exists: the undo log the first change left behind, which is exactly what readers with older snapshots use. **Abort applies it for good.**

## What each kind of change needs

For every tuple in the transaction's write set:

| what the transaction did to the tuple | what the table holds now | what abort writes |
|---|---|---|
| inserted a **new** tuple (new rid) | the tuple, `ts = my temp ts`, no log | a tombstone (`is_deleted = true`) with `ts = 0`: nobody could see it, so any old timestamp is right |
| updated or deleted a committed tuple | the new bytes (or the old bytes with `is_deleted`), `ts = my temp ts`, head link = **my** log | the old version: apply my log to the table's tuple, `meta.ts = log.ts`, and move the head link to `log.prev_version` |
| inserted over a **tombstone** | the new tuple, head link = my log with `is_deleted = true` | the tombstone again: applying the log gives "deleted", `meta.ts = log.ts` |

One log per tuple per transaction (module 4a) is what makes this a single step: the log already covers every column the transaction changed, so applying it once undoes all of the changes.

```svg
caption: Abort of transaction T7, which had updated one tuple in place. Before: the table holds T7's new bytes with T7's temporary timestamp, and the head link leads to T7's undo log, then to the older one. Abort applies T7's log to the table's tuple (its value and timestamp come back) and moves the head link to the log's `prev_version`. Readers who needed the old version already read it through the log, so nothing changes for them. (Animated: before, then after.)
<svg viewBox="0 0 760 300" role="img" aria-label="A table tuple and a chain of undo logs before and after the abort of a transaction">
<style>
@keyframes ul-b{0%,42%{opacity:1}52%,94%{opacity:0}100%{opacity:1}}
@keyframes ul-a{0%,42%{opacity:0}52%,94%{opacity:1}100%{opacity:0}}
.ul-b{opacity:0;animation:ul-b 10s ease-in-out infinite}.ul-a{opacity:1;animation:ul-a 10s ease-in-out infinite}
</style>
<defs><marker id="ul-ad" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker>
<marker id="ul-ag" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--grn)"/></marker></defs>
<text class="dim sm" x="40" y="22">the table (updated in place)</text><text class="dim sm" x="300" y="22">undo logs, newest first</text>
<g class="ul-b">
<text class="t-a big" x="40" y="258">before the abort</text>
<rect class="hot" x="40" y="50" width="190" height="84" rx="4"/><text class="t-a" x="54" y="74">tuple</text><text class="fg sm" x="54" y="96">value 30</text><text class="fg sm" x="54" y="114">ts T7 (temporary)</text>
<rect class="blue" x="300" y="50" width="190" height="84" rx="4"/><text class="t-b" x="314" y="74">T7's undo log</text><text class="fg sm" x="314" y="96">value 10</text><text class="fg sm" x="314" y="114">ts 4</text>
<rect class="blue" x="550" y="50" width="190" height="84" rx="4"/><text class="t-b" x="564" y="74">older undo log</text><text class="fg sm" x="564" y="96">value 5</text><text class="fg sm" x="564" y="114">ts 2</text>
<path class="ln" d="M230 92 H298" marker-end="url(#ul-ad)"/><path class="ln" d="M490 92 H548" marker-end="url(#ul-ad)"/>
<text class="dim sm" x="40" y="160">head link → T7's log; the log's prev_version → the older log</text>
</g>
<g class="ul-a">
<text class="t-g big" x="40" y="258">after the abort</text>
<rect class="live" x="40" y="50" width="190" height="84" rx="4"/><text class="t-g" x="54" y="74">tuple</text><text class="fg sm" x="54" y="96">value 10</text><text class="fg sm" x="54" y="114">ts 4</text>
<rect class="never" x="300" y="50" width="190" height="84" rx="4"/><text class="dim" x="314" y="74">T7's undo log</text><text class="dim sm" x="314" y="96">unreachable; stays in the</text><text class="dim sm" x="314" y="114">transaction map until GC</text>
<rect class="blue" x="550" y="50" width="190" height="84" rx="4"/><text class="t-b" x="564" y="74">older undo log</text><text class="fg sm" x="564" y="96">value 5</text><text class="fg sm" x="564" y="114">ts 2</text>
<path class="ln-g" d="M230 72 C 330 40 450 40 548 72" marker-end="url(#ul-ag)"/>
<text class="t-g sm" x="40" y="160">head link → the older log (T7's prev_version)</text>
</g>
<text class="dim sm" x="40" y="190">abort = apply T7's log to the tuple: value := log.value, ts := log.ts, head link := log.prev_version,</text>
<text class="dim sm" x="40" y="206">all under the page's write latch, so the tuple and the link change together.</text>
</svg>
```

## Why nobody is disturbed

While the transaction ran, its tuples carried its temporary timestamp, which every other transaction sees as "uncommitted, invisible". Readers that needed an older version already walked past the table's tuple into the log. Restoring the table's tuple to the old version leaves their view unchanged, and a new reader sees exactly what the old readers saw. The only thing that must happen *atomically* is the tuple + link pair: do it under the page's write latch, like any other write.

## What is not undone

Index entries. A tombstone keeps its entry in a primary-key index (that is what lets the next insert of the key find and reuse the rid), so abort leaves the entry and the restored tombstone or old tuple stays reachable through it. The transaction's undo logs also stay in the transaction map until garbage collection, but nothing links to them any more.

## Invariants: what must always be true

- **One log per tuple per transaction.** The log covers every column the transaction changed, so applying it once undoes all of the changes. Abort is one step per tuple, not one per update.
- **After an abort the tuple is as it was.** Its value, its timestamp and its head link equal what they were before the transaction touched it. No tuple still carries the aborted transaction's temporary timestamp.
- **The tuple and its link change together,** under the page's write latch. A reader never sees the restored bytes with the old link, or the reverse.
- **Readers with older snapshots see no change.** They already read the old version through the log chain; restoring the table's tuple gives the same answer.
- **Index entries stay.** An aborted insert leaves a tombstone, and the primary-key entry that finds it is what lets the next insert of the key reuse the rid.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `table_heap->UpdateTupleInPlace(meta, tuple, rid, check)` after `ReconstructTuple` | `table.with_page_mut(rid, \|page\| ...)`: read the tuple, reconstruct, write meta and bytes, all in one closure |
| `txn_mgr->UpdateUndoLink(rid, prev)` | the same, called inside the closure so tuple and link change together |

**Port rule:** the "read-modify-write under a latch" is a closure; the closure returns a `Result` and the code after it handles the error.

## In real code

### Using it: undo by applying the log

```rust test
#[derive(Clone, Debug, PartialEq)]
struct Meta {
    ts: i64,
    deleted: bool,
}

#[derive(Clone, Debug)]
struct Log {
    ts: i64,
    deleted: bool,
    old: Vec<Option<i32>>, // Some(v): this log restores that column
}

/// What abort writes for one tuple: the old version, or a tombstone.
fn rollback(table: &(Meta, Vec<i32>), my_log: Option<&Log>) -> (Meta, Vec<i32>) {
    match my_log {
        None => (Meta { ts: 0, deleted: true }, table.1.clone()), // a tuple I created: bury it
        Some(log) if log.deleted => (Meta { ts: log.ts, deleted: true }, table.1.clone()),
        Some(log) => {
            let mut row = table.1.clone();
            for (i, v) in log.old.iter().enumerate() {
                if let Some(v) = v {
                    row[i] = *v;
                }
            }
            (Meta { ts: log.ts, deleted: false }, row)
        }
    }
}

#[test]
fn undoing_an_update_restores_the_logged_columns() {
    let table = (Meta { ts: 1 << 62, deleted: false }, vec![9, 5, 7]);
    let log = Log { ts: 3, deleted: false, old: vec![Some(1), None, Some(2)] };
    assert_eq!(rollback(&table, Some(&log)), (Meta { ts: 3, deleted: false }, vec![1, 5, 2]));
}

#[test]
fn undoing_a_delete_brings_the_tuple_back_and_undoing_an_insert_buries_it() {
    let deleted = (Meta { ts: 1 << 62, deleted: true }, vec![1, 1]);
    let full = Log { ts: 4, deleted: false, old: vec![Some(1), Some(1)] };
    assert_eq!(rollback(&deleted, Some(&full)).0, Meta { ts: 4, deleted: false });
    let fresh = (Meta { ts: 1 << 62, deleted: false }, vec![5, 5]);
    assert_eq!(rollback(&fresh, None).0, Meta { ts: 0, deleted: true });
    let over_tombstone = Log { ts: 6, deleted: true, old: vec![] };
    assert_eq!(rollback(&fresh, Some(&over_tombstone)).0, Meta { ts: 6, deleted: true });
}
```

### Using it: relinking the chain

```rust test
use std::collections::HashMap;

type Link = Option<(u32, usize)>; // (txn, log index)

struct Log {
    prev: Link,
}

/// After applying the head log, the tuple's head link becomes the log's previous version.
fn unlink_head(heads: &mut HashMap<u32, Link>, rid: u32, logs: &HashMap<(u32, usize), Log>) {
    let head = heads[&rid].expect("the tuple has a chain");
    heads.insert(rid, logs[&head].prev);
}

#[test]
fn abort_exposes_the_previous_version() {
    // rid 1: my log (txn 9, 0) -> older log (txn 5, 0)
    let mut logs = HashMap::new();
    logs.insert((9, 0), Log { prev: Some((5, 0)) });
    logs.insert((5, 0), Log { prev: None });
    let mut heads = HashMap::new();
    heads.insert(1, Some((9, 0)));
    unlink_head(&mut heads, 1, &logs);
    assert_eq!(heads[&1], Some((5, 0)));
    heads.insert(2, Some((5, 0)));
    unlink_head(&mut heads, 2, &logs);
    assert_eq!(heads[&2], None, "a one-log chain ends");
}
```

### In the exercises

- **4b-04:** `TransactionManager::abort` restores every tuple in the write set, with the three cases of the table above.
- **4b-06:** why the primary-key index entry of an aborted insert stays.

### Where it is used

- **MySQL InnoDB**: rollback applies undo records to the clustered-index row in reverse order.
- **PostgreSQL** does not need this step for tuples (an aborted transaction's row versions are simply never visible, because its transaction id is recorded as aborted); it pays by leaving them for `VACUUM`.
- **HyPer-style engines** keep the undo buffer exactly for this: on abort the buffer is applied backwards, on commit it is kept for readers.
