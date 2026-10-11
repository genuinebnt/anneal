---
title: Outer joins and NULL padding
summary: What LEFT, RIGHT and FULL joins return, why the unmatched side is padded with NULLs, how an executor finds the rows that matched nothing, and why a NULL key never matches anything, not even another NULL.
minutes: 9
---
An inner join answers "which pairs of rows go together?" and silently drops every row that has no partner. For a report that is often the wrong answer. "Every customer with the number of orders they placed" must list the customer who placed none; with an inner join that customer vanishes, and the report looks as if they never existed. An **outer join** keeps the rows that found no partner and fills the columns of the missing side with `NULL`, so the absence is visible in the result.

## Three outer joins, one idea

Take two small tables, `a(x, y)` with the rows `(1, 10)`, `(2, 20)`, `(3, 30)`, and `b(x, z)` with `(2, 200)`, `(3, 300)`, `(4, 400)`. The inner join on `a.x = b.x` is two rows: the pairs for 2 and 3. Now the three outer variants:

- `a LEFT JOIN b` also returns `(1, 10, NULL, NULL)`: the left row with no partner, padded on the right.
- `a RIGHT JOIN b` also returns `(NULL, NULL, 4, 400)`: the right row with no partner, padded on the left.
- `a FULL JOIN b` returns both of those extra rows.

So every outer join is the inner join plus the unmatched rows of the *preserved* side or sides. That sentence is also the test you can write for it: the output of a LEFT join, minus the rows whose right half is padded, is the inner join, and the padded rows are exactly the left rows that appear in no inner pair.

A RIGHT join is a LEFT join written backwards: `a RIGHT JOIN b` returns the same rows as `b LEFT JOIN a` with the columns in a different order. Many databases rewrite one into the other and implement only LEFT. This course's engine keeps all three because the executor for RIGHT and FULL teaches something the LEFT one does not.

```svg
caption: Two small tables, `a.x` = 1, 2, 3 and `b.x` = 2, 3, 4. Every outer join is the inner join (2 and 3 match) plus the unmatched rows of the preserved side or sides, padded with NULLs: LEFT adds `1 · NULL`, RIGHT adds `NULL · 4`, FULL adds both. (Animated: inner, left, right, full.)
<svg viewBox="0 0 760 300" role="img" aria-label="Two tables, their matching pairs, and the extra padded rows that LEFT, RIGHT and FULL joins add to the inner join">
<style>
@keyframes oj-l{0%,24%{opacity:.12}27%,49%{opacity:1}52%,74%{opacity:.12}77%,100%{opacity:1}}
@keyframes oj-r{0%,49%{opacity:.12}52%,100%{opacity:1}}
@keyframes oj-1{0%,24%{opacity:1}27%,100%{opacity:.35}}
@keyframes oj-2{0%,24%{opacity:.35}27%,49%{opacity:1}52%,100%{opacity:.35}}
@keyframes oj-3{0%,49%{opacity:.35}52%,74%{opacity:1}77%,100%{opacity:.35}}
@keyframes oj-4{0%,74%{opacity:.35}77%,100%{opacity:1}}
.oj-l{animation:oj-l 14s ease-in-out infinite}.oj-r{animation:oj-r 14s ease-in-out infinite}
.oj-1{animation:oj-1 14s ease-in-out infinite}.oj-2{animation:oj-2 14s ease-in-out infinite}.oj-3{animation:oj-3 14s ease-in-out infinite}.oj-4{animation:oj-4 14s ease-in-out infinite}
</style>
<text class="dim sm" x="40" y="30">a</text><text class="dim sm" x="300" y="30">b</text><text class="dim sm" x="520" y="30">output</text>
<rect class="hot oj-l" x="40" y="44" width="120" height="34" rx="4"/><text class="mid fg" x="100" y="66">x = 1</text>
<rect class="live" x="40" y="94" width="120" height="34" rx="4"/><text class="mid fg" x="100" y="116">x = 2</text>
<rect class="live" x="40" y="144" width="120" height="34" rx="4"/><text class="mid fg" x="100" y="166">x = 3</text>
<rect class="live" x="300" y="44" width="120" height="34" rx="4"/><text class="mid fg" x="360" y="66">x = 2</text>
<rect class="live" x="300" y="94" width="120" height="34" rx="4"/><text class="mid fg" x="360" y="116">x = 3</text>
<rect class="hot oj-r" x="300" y="144" width="120" height="34" rx="4"/><text class="mid fg" x="360" y="166">x = 4</text>
<path class="ln-g" d="M160 111 L298 61"/><path class="ln-g" d="M160 161 L298 111"/>
<rect class="live" x="520" y="44" width="220" height="30" rx="4"/><text class="fg sm" x="534" y="64">2 · 2     (inner)</text>
<rect class="live" x="520" y="80" width="220" height="30" rx="4"/><text class="fg sm" x="534" y="100">3 · 3     (inner)</text>
<g class="oj-l"><rect class="hot" x="520" y="116" width="220" height="30" rx="4"/><text class="fg sm" x="534" y="136">1 · NULL   (a, unmatched)</text></g>
<g class="oj-r"><rect class="hot" x="520" y="152" width="220" height="30" rx="4"/><text class="fg sm" x="534" y="172">NULL · 4   (b, unmatched)</text></g>
<text class="t-g sm oj-1" x="40" y="218">INNER   the pairs that match: (2, 2) and (3, 3). Unmatched rows vanish.</text>
<text class="t-a sm oj-2" x="40" y="238">LEFT    inner + each unmatched row of a, the columns of b padded with NULL.</text>
<text class="t-a sm oj-3" x="40" y="258">RIGHT   inner + each unmatched row of b, the columns of a padded with NULL.</text>
<text class="t-a sm oj-4" x="40" y="278">FULL    inner + the unmatched rows of both sides.</text>
</svg>
```

## Why a left join is easy and a right join is not

The nested loop join of module 3f takes a left row, scans the whole right side for partners, and emits a pair for each. For a LEFT join one more line is enough: if the scan found no partner, emit the left row padded with NULLs. The decision about a left row is complete the moment its scan ends.

Now ask the same question for a right row. Is `(4, 400)` unmatched? You cannot say until *every* left row has been compared with it, and the executor is at the first left row. The decision is made at the very end. The executor therefore needs memory: the right side is read once into a list, each entry gets a flag "somebody matched me", every comparison sets flags, and when the left side is exhausted the entries whose flag is still off are emitted with NULLs on the left. A FULL join does both things: the LEFT join's padding as it goes, and the flags at the end.

The price is that the whole right side is held in memory, which the plain nested loop join avoids. A hash join solves the same problem in the same way (the build side's table gets a "matched" bit per entry); a sort-merge join finds unmatched rows for free because both sides are walked in order.

## NULL keys

`NULL = NULL` is unknown, not true (see the article on three-valued logic), so a row whose join key is NULL matches nothing, including another NULL key. In an outer join such a row is not lost, though: it has no partner, so it is emitted padded. `a FULL JOIN b ON a.x = b.x` with one NULL key on each side gives two output rows, one per side, each padded; if NULLs matched each other it would give one row with both halves filled in. A test that mixes NULL keys into random tables catches an executor that compares "equal" with `==` on values instead of asking the predicate.

## The condition in ON is not a filter

In an inner join a condition can sit in `ON` or in `WHERE` and the answer is the same. In an outer join it cannot. `a LEFT JOIN b ON a.x = b.x AND b.z > 250` keeps every row of `a`: a row whose partners all have `z <= 250` simply counts as unmatched and is padded. The same condition in `WHERE` removes rows afterwards, including the padded ones (their `b.z` is NULL, and `NULL > 250` is unknown). The next article, *filters above and below outer joins*, builds on exactly this difference.

## Invariants: what must always be true

- **Output = inner join + unmatched rows of the preserved side(s),** each padded with NULLs, and nothing else. Remove the padded rows from a LEFT join and what is left is the inner join.
- **Every row of a preserved side appears at least once.** That is the whole point of an outer join; a result that loses one is wrong, with no error to say so.
- **Padding is NULL,** never zero or an empty string: a client must be able to tell "no partner" from "partner with value 0".
- **A NULL join key matches nothing,** not even another NULL, yet the row is still emitted, padded, on a preserved side.
- **A condition in `ON` decides who matches; it never removes a preserved row.** A condition in `WHERE` is applied afterwards and does remove rows, which is why moving one between them changes the answer.
- **`a RIGHT JOIN b` returns the rows of `b LEFT JOIN a`,** with the columns in the other order.

## Try it

Write the three outer joins for the tables above on paper. Then change `b` to contain two rows with `x = 2` and say how many rows each join returns. Then put a NULL key in both tables and decide, before running anything, how many rows a FULL join has.

## In real code

### Using it: the outer joins over vectors, and the right-side flags

```rust test
type Row = (Option<i32>, i32); // (join key, payload)

/// `left FULL JOIN right ON left.key = right.key`, as (left payload, right payload) with None for the padding.
/// `keep_left` / `keep_right` choose LEFT, RIGHT or FULL.
fn join(left: &[Row], right: &[Row], keep_left: bool, keep_right: bool) -> Vec<(Option<i32>, Option<i32>)> {
    let mut out = vec![];
    let mut matched = vec![false; right.len()]; // the flag per right row
    for l in left {
        let mut found = false;
        for (i, r) in right.iter().enumerate() {
            // NULL = anything is unknown: only two present, equal keys match
            if l.0.is_some() && l.0 == r.0 {
                found = true;
                matched[i] = true;
                out.push((Some(l.1), Some(r.1)));
            }
        }
        if !found && keep_left {
            out.push((Some(l.1), None));
        }
    }
    if keep_right {
        for (i, r) in right.iter().enumerate() {
            if !matched[i] {
                out.push((None, Some(r.1)));
            }
        }
    }
    out
}

#[test]
fn the_three_outer_joins_add_the_unmatched_rows() {
    let a = [(Some(1), 10), (Some(2), 20), (Some(3), 30)];
    let b = [(Some(2), 200), (Some(3), 300), (Some(4), 400)];
    assert_eq!(join(&a, &b, false, false).len(), 2);
    assert!(join(&a, &b, true, false).contains(&(Some(10), None)));
    assert!(join(&a, &b, false, true).contains(&(None, Some(400))));
    assert_eq!(join(&a, &b, true, true).len(), 4);
}

#[test]
fn null_keys_never_match_each_other_and_both_rows_survive_a_full_join() {
    let (n1, n2) = ([(None, 1)], [(None, 2)]);
    assert_eq!(join(&n1, &n2, true, true), vec![(Some(1), None), (None, Some(2))]);
}
```

The padded rows of the left side come out as the loop goes; the padded rows of the right side only after the loop, because only then are the flags final.

### In the exercises

- **3j-01:** the executor for `RIGHT` and `FULL` joins: the flag per right row (`matched` above), the padded rows queued behind the matched ones, and the restart in `init`.
- **3j-07:** the boss states the three identities of this article (LEFT = INNER plus unmatched left rows, RIGHT = swapped LEFT, FULL = LEFT plus unmatched right rows) over random tables with NULLs.

### Where it is used

PostgreSQL implements `FULL JOIN` only for merge and hash joins (never for a bare nested loop) because both can mark unmatched rows cheaply; its hash join keeps a "matched" bit in each build tuple. SQLite supports `RIGHT` and `FULL` joins since version 3.39 and executes them by a nested loop that records the matched rows of the right table. `LEFT JOIN` is the form real queries use most; many ORMs generate nothing else.
