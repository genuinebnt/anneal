---
title: Access paths: sequential scan or index?
summary: The two ways to read a table, what each costs, when an index wins and when it loses, why an index scan returns rows in key order, and how a rid from an index turns into a row.
minutes: 8
---
To answer `select * from t where a = 5` the engine has an **access path** to choose: read every row and test it (a **sequential scan**), or ask an index which row has `a = 5` and fetch just that (an **index scan**). Both produce the same rows; they differ in how many pages they touch and in what order the rows come out.

| | sequential scan | index scan |
|---|---|---|
| reads | every page of the table, in storage order | index pages along one path, then the table page of each match |
| cost | proportional to the **table size** | proportional to the **number of matches** (and the tree height) |
| output order | storage order (usually insertion order) | **key order** |
| good when | most rows match, the table is small, no usable index | few rows match, the query asks for an order the index has |

## Selectivity decides

If a query matches 1 row in a million, the index reads about 4 pages and wins by five orders of magnitude. If it matches 60% of the rows, the index scan visits most of the table's pages one fetch at a time in *random* order (each rid leads to a different page), which is far worse than reading the table sequentially. Optimizers estimate the fraction of rows that match (the **selectivity**, from statistics) and choose; PostgreSQL's planner switches from an index scan to a *bitmap* scan to a sequential scan as the estimate grows. BusTub has no statistics and uses simple rules.

```svg
caption: Pages read as the share of matching rows grows (not to scale). A sequential scan always reads the whole table, so it is a flat line. An index scan reads a few index pages and then one table page per match, so it starts far below and climbs steeply. Where the lines cross, the optimizer's estimate of selectivity decides. (Animated: more rows match.)
<svg viewBox="0 0 760 316" role="img" aria-label="A chart of pages read against the fraction of rows matched, with a flat sequential scan line and a rising index scan line that cross">
<style>
@keyframes ap-m{0%{transform:translateX(0)}86%,100%{transform:translateX(600px)}}
@keyframes ap-a{0%,22%{opacity:1}28%,100%{opacity:.2}}
@keyframes ap-b{0%,24%{opacity:.2}30%,100%{opacity:1}}
.ap-m{animation:ap-m 12s linear infinite}.ap-a{animation:ap-a 12s linear infinite}.ap-b{animation:ap-b 12s linear infinite}
</style>
<path class="grid" d="M70 240 H730"/><path class="grid" d="M70 40 V240"/>
<text class="dim sm" x="76" y="32">pages read</text><text class="end dim sm" x="730" y="262">share of rows that match, 0 to 100% →</text>
<path style="stroke:var(--fn);stroke-width:2.4;fill:none" d="M70 96 H730"/>
<text class="end t-b sm" x="730" y="86">sequential scan: the whole table</text>
<path style="stroke:var(--acc);stroke-width:2.4;fill:none" d="M70 232 L170 96 L730 44"/>
<text class="t-a sm" x="200" y="190">index scan: a few pages, then one table page per match</text>
<circle class="hot" cx="170" cy="96" r="6"/>
<text class="dim sm" x="184" y="120">they cross</text>
<g class="ap-m"><path style="stroke:var(--dim);stroke-width:1.4;stroke-dasharray:4 4" d="M80 40 V240"/><circle class="live" cx="80" cy="236" r="5"/></g>
<text class="t-a sm ap-a" x="70" y="286">1 row in a million: the index reads about 4 pages and wins by orders of magnitude.</text>
<text class="t-b sm ap-b" x="70" y="304" style="text-anchor:start">60% of the rows: random fetches lose to one sequential pass.</text>
</svg>
```

## From a key to a row

The index stores `key → rid`, not the row. An index scan therefore does, for each match: **look up the key** (get the rid) → **fetch the tuple at the rid** from the table heap → **check it is still there** (the table is the truth; a deleted row may linger in an index) → apply any further filter. Two reads per row, which is why many matches make an index scan slow.

## Ordered output for free

A B+ tree keeps its keys sorted, and its leaves are linked (module 2c): a full scan of the index visits every key in order. `select * from t order by a` on an indexed `a` can therefore skip the sort entirely and produce rows from an index scan of the whole tree. The price is the same random fetch per row; for a `LIMIT 10` it is an excellent trade, for a full table sort it may not be.

## Point lookups and ranges

`where a = 5` is a **point** lookup: one key (or a list: `a = 4 or a = 7` is two lookups). `where a > 5` is a **range** scan: find the first key, follow the leaf chain. BusTub's index scan plan supports points (`pred_keys`) and the full ordered scan; ranges are an exercise in the same spirit.

## Invariants: what must always be true

- **Both paths return the same rows,** as a set. An index scan never invents a row and never drops one that the sequential scan would have returned.
- **The table is the truth.** Every rid an index returns is checked against the table heap: a deleted row may still be in the index, and a tuple that is gone must not appear in the output.
- **Order is a promise only of the index scan:** key order, because the leaves are linked in order. A sequential scan promises none.
- **Cost depends on different things.** A sequential scan on the size of the table, an index scan on the number of matches and the height of the tree; the choice depends on the estimated selectivity, so a wrong estimate is a slow query, never a wrong answer.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `index->ScanKey(key, &rids, txn)` fills an out-vector | `index.scan_key(&key) -> Vec<Rid>` returns it |
| `table_heap->GetTuple(rid)` returns `pair<TupleMeta, Tuple>` | `table.get_tuple(rid)? -> (TupleMeta, Tuple)` |
| iterate the B+ tree with `IndexIterator` (lazy) | `index.scan_all() -> Vec<Rid>` (materialised; simpler, uses memory) |


## Indexes and NULLs

An index answers "which rows have this key?". A NULL is not a key: `NULL = 5` and `NULL = NULL` are never true, so no query can ask for it by equality. The indexes in this course therefore do not store rows whose key has a NULL in it, and a lookup of NULL finds nothing, which is exactly what SQL says `WHERE v1 = NULL` means. One consequence to know: reading a table **in key order through its index** (an index scan without keys) lists only the rows whose key is not NULL, so it is a correct way to answer `ORDER BY v1` only for a column that cannot be NULL (a primary key). Real databases that index NULLs (PostgreSQL's B-trees do) pay for it with a more complicated comparison and sort NULLs first or last.

## In real code

### Using it: two access paths over the same rows

```rust test
use std::collections::BTreeMap;

struct Table { rows: Vec<(i32, &'static str)> } // storage order: arrival order
struct Index { by_key: BTreeMap<i32, usize> }   // key -> position in the table

fn seq_scan(t: &Table, pred: impl Fn(i32) -> bool) -> (Vec<&'static str>, usize) {
    let mut touched = 0;
    let out = t.rows.iter().filter(|(k, _)| { touched += 1; pred(*k) }).map(|(_, v)| *v).collect();
    (out, touched)
}

fn index_point(t: &Table, i: &Index, key: i32) -> (Vec<&'static str>, usize) {
    match i.by_key.get(&key) {
        Some(&rid) => (vec![t.rows[rid].1], 1),
        None => (vec![], 0),
    }
}

fn index_ordered(t: &Table, i: &Index) -> Vec<&'static str> {
    i.by_key.values().map(|&rid| t.rows[rid].1).collect()
}

fn fixture() -> (Table, Index) {
    let rows = vec![(30, "c"), (10, "a"), (20, "b"), (40, "d")];
    let by_key = rows.iter().enumerate().map(|(rid, (k, _))| (*k, rid)).collect();
    (Table { rows }, Index { by_key })
}

#[test]
fn both_paths_find_the_same_row_but_touch_different_amounts() {
    let (t, i) = fixture();
    let (via_scan, touched_scan) = seq_scan(&t, |k| k == 20);
    let (via_index, touched_index) = index_point(&t, &i, 20);
    assert_eq!(via_scan, via_index);
    assert_eq!((touched_scan, touched_index), (4, 1), "the scan looks at every row, the index at one");
    assert_eq!(index_point(&t, &i, 25), (vec![], 0));
}

#[test]
fn the_index_gives_key_order_and_the_scan_gives_storage_order() {
    let (t, i) = fixture();
    assert_eq!(seq_scan(&t, |_| true).0, vec!["c", "a", "b", "d"]);
    assert_eq!(index_ordered(&t, &i), vec!["a", "b", "c", "d"]);
}

#[test]
fn a_rid_in_the_index_can_name_a_row_that_is_gone() {
    let (mut t, i) = fixture();
    t.rows[2].1 = ""; // pretend row 2 was deleted behind the index's back
    let live: Vec<_> = index_ordered(&t, &i).into_iter().filter(|v| !v.is_empty()).collect();
    assert_eq!(live, vec!["a", "c", "d"], "the executor must check the table, which is the truth");
}
```

### In the exercises

- **3e-04:** the ordered index scan: `scan_all`, fetch each tuple, skip deleted ones.
- **3e-04:** point lookups: `pred_keys` → `scan_key`; combining with the plan's filter.
- **Module 3h:** the optimizer rules that *choose* the index scan (`order by` on an indexed column, `where col = constant`).

### Where it is used

- **PostgreSQL**: Seq Scan / Index Scan / Index Only Scan / Bitmap Heap Scan, chosen by cost; `EXPLAIN` shows which.
- **SQLite**: "SCAN t" versus "SEARCH t USING INDEX i (a=?)" in `EXPLAIN QUERY PLAN`.
- **MySQL InnoDB**: a secondary-index lookup then a primary-key lookup (the "double lookup").
- **Covering indexes**: if the index contains every column the query needs, the table is not read at all (index-only scan).
