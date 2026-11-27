A game server sends millions of scores through one function. The screen shows only the best `k`.

**An entry** is an id and a score. `Entry::new(id, score)` returns `None` when the score is NaN, which is not a score. `parts()` gives the id and the score back.

**Which entry is better.** The one with the higher score. With equal scores, the one with the **smaller** id.

For scores, `-0.0` is lower than `0.0`, and both infinities are ordinary scores. This is the order of `f64::total_cmp`. Implement `Ord` and the traits beside it so that **greater means better**. Two entries are equal when neither is better.

**The list** works for any ordered item `T`, not only for entries. `Best::new(k)` starts an empty list that keeps at most `k` items. Greater is better.

- `offer(item)` considers one item. While the list holds fewer than `k` items, the item is kept. When it holds `k`, the item replaces the worst one only if it is **greater** than that one. `offer` returns `true` when the new item is kept.
- `cutoff()` returns the worst item kept, but only when the list holds exactly `k` items and `k` is above 0. Otherwise it returns `None`.
- `into_sorted()` returns the kept items, greatest first.

Equal items are each kept as an item of their own. `k` is any `usize`, `usize::MAX` included.

**The budget.** The tests offer items that count their comparisons. Every call of `cmp`, `partial_cmp` or `eq` on them counts once. Let `n` be the number of offers, `m` the smaller of `k` and `n`, and `bits(m)` the number of binary digits of `m`. All the calls of one test, `into_sorted` included, may compare at most `(n + 1) * (6 * bits(m) + 12)` times. Each offer is followed by a call to `cutoff`. The test stops as soon as the budget is passed.

Only comparisons are graded. Moving items about is not, so measure that yourself with `k` of 100 000 and random scores.
