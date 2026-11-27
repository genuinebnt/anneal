A parcel desk looks up tracking codes that people type by hand. Typed codes are often off by a slip or two. The desk wants to know whether a stored code is **close** to the typed one, and how close.

**A slip** is one of these, and each counts as 1:

- one item is added;
- one item is left out;
- one item is replaced by another;
- two **neighbouring** items change places.

**No item is touched by two slips.** For example, `ca` to `abc` takes **3** slips. Swapping to `ac` and then adding `b` between the two is not allowed.

`slips(a, b, max)` returns `Some(d)` when the smallest number of slips from `a` to `b` is `d` and `d <= max`. Otherwise it returns `None`. `max` is any `usize`, `usize::MAX` included.

The items are any type that can be compared for equality.

**The budget.** Some tests count how often two items are compared (each call of `eq`).

Let `n` be `a.len()`, `m` be `b.len()` and `w` the smaller of `2 * max + 1` and `m + 1`. One call of `slips` may compare at most `4 * (n + 1) * w + 16` times. The test stops as soon as the budget is passed.

So two codes of 5 000 items with `max = 2` allow about 100 000 comparisons. Memory is not graded.
