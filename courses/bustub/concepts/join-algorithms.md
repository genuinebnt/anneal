---
title: Join algorithms: nested loop, hash and index
summary: The three ways to evaluate a join, their costs, when each wins, how inner and left joins differ, and what the inner side must be able to do (be restarted, be probed).
minutes: 10
---
A join combines two inputs, **left** (outer) and **right** (inner), on a predicate: `from emp join dept on emp.dept_id = dept.id`. The result must contain a row for every *pair* of rows that satisfies the predicate. The question is how to find those pairs without looking at all `|left| × |right|` of them.

| algorithm | idea | cost (pairs examined) | needs |
|---|---|---|---|
| **nested loop** | for every left row, scan the whole right side and test the predicate | `|L| × |R|` | nothing: any predicate |
| **hash join** | build a hash table of the right side on its join key; for every left row, look up its key | `|L| + |R|` | an equality predicate |
| **index nested loop** | for every left row, probe an index on the right table | `|L| × log|R|` | an equality predicate and an index |

```svg
caption: A hash join in two phases. Build: hash every right row on the join key into a table. Probe: for each left row, hash its key and look in the one bucket that could match.
<svg viewBox="0 0 760 180" role="img" aria-label="Build phase fills a hash table from the right input; probe phase looks up each left row">
<defs><marker id="ja-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<text class="dim sm" x="110" y="20" style="text-anchor:middle">1. build (right input)</text>
<rect class="live" x="20" y="30" width="180" height="28" rx="2"/><text class="mid fg sm" x="110" y="49">r1: id 7</text>
<rect class="live" x="20" y="62" width="180" height="28" rx="2"/><text class="mid fg sm" x="110" y="81">r2: id 3</text>
<rect class="live" x="20" y="94" width="180" height="28" rx="2"/><text class="mid fg sm" x="110" y="113">r3: id 7</text>
<rect class="blue" x="300" y="30" width="170" height="92" rx="3"/><text class="mid t-b sm" x="385" y="52">hash table</text><text class="mid dim sm" x="385" y="74">3 → [r2]</text><text class="mid dim sm" x="385" y="94">7 → [r1, r3]</text>
<path class="ln" d="M202 76 L298 76" marker-end="url(#ja-a)"/>
<text class="dim sm" x="610" y="20" style="text-anchor:middle">2. probe (left input)</text>
<rect class="live" x="540" y="30" width="180" height="28" rx="2"/><text class="mid fg sm" x="630" y="49">l1: dept 7  → r1, r3</text>
<rect class="live" x="540" y="62" width="180" height="28" rx="2"/><text class="mid fg sm" x="630" y="81">l2: dept 9  → none</text>
<path class="ln" d="M538 44 C500 44 490 70 472 76" marker-end="url(#ja-a)"/>
<text class="dim sm" x="380" y="160" style="text-anchor:middle">each left row is examined once; only matching rows are paired</text>
</svg>
```

```svg
caption: A hash join in two steps. Build: read the right side once and put each row with a non-NULL key into a table keyed on the join key. Probe: for each left row look its key up; a hit is output, a miss is not, and a NULL key is never looked up, because `NULL = x` is unknown. The cost is one pass over each side. (Animated: build, then probe.)
<svg viewBox="0 0 760 330" role="img" aria-label="Right-side rows being built into a hash table, then left-side rows probing it, with one miss and one NULL key skipped">
<style>
@keyframes hj-b{0%,40%{opacity:1}46%,100%{opacity:.25}}
@keyframes hj-p{0%,46%{opacity:.25}52%,92%{opacity:1}98%,100%{opacity:.25}}
.hj-b{animation:hj-b 12s ease-in-out infinite}.hj-p{animation:hj-p 12s ease-in-out infinite}
</style>
<defs>
<marker id="hj-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker>
<marker id="hj-g" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--grn)"/></marker>
<marker id="hj-r" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--bad)"/></marker>
</defs>
<text class="dim sm" x="30" y="28">left: probe side</text><text class="dim sm" x="300" y="28">hash table on the join key</text><text class="end dim sm" x="730" y="28">right: build side</text>
<g class="hj-p">
<rect class="blue" x="30" y="42" width="130" height="30" rx="4"/><text class="fg sm" x="42" y="62">dept_id 2</text>
<rect class="blue" x="30" y="82" width="130" height="30" rx="4"/><text class="fg sm" x="42" y="102">dept_id 7</text>
<rect class="never" x="30" y="122" width="130" height="30" rx="4"/><text class="dim sm" x="42" y="142">dept_id NULL</text>
<rect class="blue" x="30" y="162" width="130" height="30" rx="4"/><text class="fg sm" x="42" y="182">dept_id 3</text>
<path class="ln-g" d="M160 57 L298 57" marker-end="url(#hj-g)"/>
<path style="stroke:var(--bad);stroke-width:1.6;fill:none;stroke-dasharray:4 3" d="M160 97 L230 97" marker-end="url(#hj-r)"/><text class="t-r sm" x="236" y="101">miss</text>
<text class="dim sm" x="170" y="142">never probes (NULL)</text>
<path class="ln-g" d="M160 177 L298 107" marker-end="url(#hj-g)"/>
</g>
<rect class="live" x="300" y="42" width="160" height="30" rx="4"/><text class="fg sm" x="312" y="62">2 → (2, eng)</text>
<rect class="live" x="300" y="92" width="160" height="30" rx="4"/><text class="fg sm" x="312" y="112">3 → (3, ops)</text>
<rect class="live" x="300" y="142" width="160" height="30" rx="4"/><text class="fg sm" x="312" y="162">5 → (5, hr)</text>
<g class="hj-b">
<rect class="hot" x="600" y="42" width="130" height="30" rx="4"/><text class="fg sm" x="612" y="62">(3, ops)</text>
<rect class="hot" x="600" y="82" width="130" height="30" rx="4"/><text class="fg sm" x="612" y="102">(2, eng)</text>
<rect class="hot" x="600" y="122" width="130" height="30" rx="4"/><text class="fg sm" x="612" y="142">(5, hr)</text>
<path class="ln" d="M598 57 L462 107" marker-end="url(#hj-a)"/><path class="ln" d="M598 97 L462 57" marker-end="url(#hj-a)"/><path class="ln" d="M598 137 L462 157" marker-end="url(#hj-a)"/>
</g>
<text class="t-a sm hj-b" x="30" y="232">1 · build: read the right side once; every row with a non-NULL key goes into the table.</text>
<text class="t-g sm hj-p" x="30" y="252">2 · probe: for each left row, look its key up. A hit is output, a miss is not, a NULL key is never looked up.</text>
<text class="dim sm" x="30" y="292">output: (2, eng) and (3, ops). Work: one pass over each side, not |left| × |right| comparisons.</text>
</svg>
```

## Inner vs left join

An **inner** join outputs only matching pairs. A **left** join outputs every left row: matched ones as above, and a left row with *no* match once, padded with NULLs on the right side. The NULLs have the right side's column types (`integer_null`, `varlen_null`). Right and full joins are the mirror image and need to remember which right rows were matched; BusTub supports inner and left only.

## What the inner side must support

A nested loop join reads the whole inner side once **per outer row**, so the inner executor must be **restartable**: its `init` called again must produce the same rows from the start. (The test runner's `+ensure:nlj_init_check` verifies that the right side is re-initialised for every left tuple, give or take one.) A hash join reads the inner side once (the build). An index join does not read it: it asks the index.

## NULLs and the predicate

The predicate of a join is evaluated with SQL's three-valued logic; a pair is output only if it is **true**. `NULL = x` is unknown, so a NULL key never matches anything, including another NULL. A hash join has to implement that explicitly: rows whose key contains a NULL are not put in the table and do not probe it.

## Choosing

With no equality predicate (`a.x < b.y`) only nested loop works. With an equality, hash join is almost always faster for large inputs; an index join wins when the left side is small and the right is huge and indexed (a few probes beat building a table of millions). Optimizers choose by cost estimates; BusTub's rules choose by pattern (module 3h).

## Invariants: what must always be true

- **Every output row is a pair whose predicate is true.** Unknown is not true: a pair whose predicate evaluates to NULL is not output.
- **Every satisfying pair is output exactly once,** whichever algorithm runs: nested loop, hash and index joins return the same rows, as a bag, in possibly different orders.
- **A NULL key never matches,** not even another NULL. A hash join keeps rows with a NULL key out of its table and never probes with one.
- **A left join outputs every left row at least once:** matched ones as pairs, an unmatched one once, padded with NULLs of the right side's column types.
- **The inner side of a nested loop join is restartable.** Initialising it again gives the same rows from the start, once per outer row.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `expr->EvaluateJoin(&left_tuple, left_schema, &right_tuple, right_schema)` | `expr.evaluate_join(&left, left_schema, &right, right_schema)?` |
| the join builds an output tuple by concatenating `Value`s of both sides | `Tuple::new(&values, &output_schema)` from a `Vec<Value>` |
| `ValueFactory::GetNullValueByType(column.GetType())` for the padding | `Value::null(column.type_id())` |
| `std::unordered_multimap` for the build side | `HashMap<Key, Vec<Tuple>>` |

## In real code

### Using it: three joins over the same inputs

```rust test
use std::collections::HashMap;

type Row = (i32, &'static str);

fn nested_loop(left: &[Row], right: &[Row], left_join: bool) -> (Vec<(Row, Option<Row>)>, usize) {
    let (mut out, mut compared) = (vec![], 0);
    for l in left {
        let mut matched = false;
        for r in right {
            compared += 1;
            if l.0 == r.0 { out.push((*l, Some(*r))); matched = true; }
        }
        if !matched && left_join { out.push((*l, None)); }
    }
    (out, compared)
}

fn hash_join(left: &[Row], right: &[Row], left_join: bool) -> (Vec<(Row, Option<Row>)>, usize) {
    let mut table: HashMap<i32, Vec<Row>> = HashMap::new();
    for r in right { table.entry(r.0).or_default().push(*r); }       // build
    let (mut out, mut compared) = (vec![], 0);
    for l in left {                                                  // probe
        match table.get(&l.0) {
            Some(rs) => for r in rs { compared += 1; out.push((*l, Some(*r))); },
            None if left_join => out.push((*l, None)),
            None => {}
        }
    }
    (out, compared)
}

fn inputs() -> (Vec<Row>, Vec<Row>) {
    ((0..50).map(|i| (i, "l")).collect(), vec![(3, "x"), (7, "y"), (7, "z"), (60, "w")])
}

#[test]
fn both_algorithms_find_the_same_pairs_but_examine_very_different_numbers() {
    let (l, r) = inputs();
    let (nl, nl_work) = nested_loop(&l, &r, false);
    let (hj, hj_work) = hash_join(&l, &r, false);
    let mut a = nl.clone(); a.sort();
    let mut b = hj.clone(); b.sort();
    assert_eq!(a, b);
    assert_eq!(a.len(), 3);
    assert_eq!((nl_work, hj_work), (200, 3), "50 × 4 pairs against 3 probes that found something");
}

#[test]
fn a_left_join_keeps_unmatched_left_rows_once_with_nothing_on_the_right() {
    let (l, r) = inputs();
    let (out, _) = hash_join(&l, &r, true);
    assert_eq!(out.len(), 3 + 48, "3 matched pairs, and the other 48 left rows once each");
    assert_eq!(out.iter().filter(|(_, r)| r.is_none()).count(), 48);
    let (nl, _) = nested_loop(&l, &r, true);
    assert_eq!(nl.len(), out.len());
}

#[test]
fn a_left_row_that_matches_several_right_rows_appears_for_each() {
    let (l, r) = inputs();
    let (out, _) = hash_join(&l, &r, false);
    let sevens: Vec<_> = out.iter().filter(|(l, _)| l.0 == 7).collect();
    assert_eq!(sevens.len(), 2);
}
```

### In the exercises

- **3f-03:** nested loop join: inner, then left with NULL padding; the right child is re-initialised for each left tuple.
- **3f-04:** hash join: build, probe, NULL keys; inner, then left.
- **3f-05:** nested index join: probe the inner table's index for each outer tuple.
- **Module 3h:** the optimizer rule that turns an equality nested loop join into a hash join.

### Where it is used

- **PostgreSQL**: Nested Loop, Hash Join, Merge Join nodes; `EXPLAIN` shows which was chosen.
- **SQLite**: only nested loops (with automatic indexes built on the fly for the inner table).
- **DuckDB**: hash joins (radix-partitioned), with sort-merge and index joins as alternatives.
- **Distributed engines**: broadcast hash joins and shuffle (partitioned) hash joins.
