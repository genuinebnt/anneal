---
title: Routing with separator keys: lower bounds, upper bounds and the equal-key rule
summary: How an internal page turns a key into a child with one binary search, why the key equal to a separator goes right, and the std tools (partition_point, binary_search) that give you both bounds.
minutes: 9
---
An internal page of a B+ tree is a sorted list of **separator keys** with a child between each pair. Searching it is one question: *which child's range contains my key?* Answer it with a binary search and a lookup in a 681-way page takes ten comparisons. The code is four lines; the part that goes wrong is which side of the boundary an *equal* key falls on.

## The ranges

With children `c0 … c(n-1)` and keys `k1 … k(n-1)` (there is no `k0`), child `ci` covers

```text
c0:    key <  k1
c1:  k1 <= key <  k2
c2:  k2 <= key <  k3
 ...
c(n-1): k(n-1) <= key
```

The lower end is inclusive, the upper end exclusive, so a key equal to a separator belongs to the child on its **right**. This is not arbitrary: a separator is the smallest key of the subtree to its right (a leaf split copies the new leaf's first key up), so searching for exactly that key must go right, where it is.

```svg
caption: The page has separators 10, 20 and 30 and four children. Keys 9, 10, 15, 20, 29 and 30 each go to exactly one child; the boundary values 10, 20 and 30 go right.
<svg viewBox="0 0 760 200" role="img" aria-label="A number line from 0 to 40 split at 10, 20 and 30 into four child ranges with example keys">
<line class="grid" x1="40" y1="90" x2="720" y2="90"/>
<g>
<line class="ln-w" x1="210" y1="60" x2="210" y2="120"/><text class="mid t-w" x="210" y="50">10</text>
<line class="ln-w" x1="380" y1="60" x2="380" y2="120"/><text class="mid t-w" x="380" y="50">20</text>
<line class="ln-w" x1="550" y1="60" x2="550" y2="120"/><text class="mid t-w" x="550" y="50">30</text>
</g>
<text class="mid t-b" x="125" y="150">child 0 · key &lt; 10</text>
<text class="mid t-g" x="295" y="150">child 1 · 10 ≤ key &lt; 20</text>
<text class="mid t-a" x="465" y="150">child 2 · 20 ≤ key &lt; 30</text>
<text class="mid t-v" x="635" y="150">child 3 · 30 ≤ key</text>
<circle class="hot" cx="190" cy="90" r="6"/><text class="mid dim sm" x="190" y="80">9</text>
<circle class="hot" cx="210" cy="90" r="6"/><text class="mid dim sm" x="240" y="80">10 → right</text>
<circle class="hot" cx="295" cy="90" r="6"/><text class="mid dim sm" x="295" y="80">15</text>
<circle class="hot" cx="380" cy="90" r="6"/><text class="mid dim sm" x="410" y="80">20 → right</text>
<circle class="hot" cx="505" cy="90" r="6"/><text class="mid dim sm" x="505" y="80">29</text>
<circle class="hot" cx="550" cy="90" r="6"/><text class="mid dim sm" x="580" y="80">30 → right</text>
</svg>
```

## One search, two bounds

Two functions cover every search over sorted keys:

| name | answers | the "less" test | std |
|---|---|---|---|
| **lower bound** | the first index whose key is **not less** than the target | `key < target` | `partition_point(\|k\| k < target)` |
| **upper bound** | the first index whose key is **greater** than the target | `key <= target` | `partition_point(\|k\| k <= target)` |

`partition_point(pred)` is the primitive: it assumes the slice is split into a prefix where `pred` is true and a suffix where it is false, and returns the length of the prefix, with a binary search. Lower and upper bound differ by one character.

- A **leaf** wants the lower bound: "the first slot whose key is not less than mine"; it is a hit if that slot's key is equal.
- An **internal page** wants the upper bound over the keys `k1 …`: the first key greater than the target, and the child **before** it. If no key is greater, the last child.

Because the first slot of an internal page has no key, search the slice that starts at slot 1 and the number the search returns is already the child index.

> [!WARNING] The bug this prevents
> Use the lower bound on an internal page and a key equal to a separator goes left, into a subtree that does not contain it. The tree looks fine until the first lookup of an exact separator key returns nothing. Write the test with `key == separator` first.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::lower_bound(first, last, value, comp)` | `slice.partition_point(\|x\| comp(x, value))` (the predicate is "is less") |
| `std::upper_bound(first, last, value, comp)` | `slice.partition_point(\|x\| !(value < x))` |
| the starter's `comparator_(a, b) < 0` three-way function object | an `Ordering` from `KeyComparator::compare`, then `.is_lt()`, `.is_le()` |
| searching an array whose slot 0 is invalid by starting at `i = 1` | slicing: `&keys[1..]` |

## In real code

### Using it: partition_point, the std binary searches, and a routing function

```rust test
/// The child to follow for `key` in a page with separators `keys` (the keys of slots 1..): the number of separators that are <= key.
fn route(keys: &[i64], key: i64) -> usize { keys.partition_point(|&k| k <= key) }

/// The same by reading every key: the obviously-correct version to test against.
fn route_linear(keys: &[i64], key: i64) -> usize { keys.iter().take_while(|&&k| k <= key).count() }

#[test]
fn the_key_equal_to_a_separator_goes_right() {
    let keys = [10, 20, 30];
    let cases = [(i64::MIN, 0), (9, 0), (10, 1), (15, 1), (19, 1), (20, 2), (29, 2), (30, 3), (31, 3), (i64::MAX, 3)];
    for (key, child) in cases {
        assert_eq!(route(&keys, key), child, "key {key}");
    }
}

#[test]
fn binary_and_linear_routing_agree_everywhere() {
    let keys: Vec<i64> = (1..=300).map(|i| i * 7).collect();         // 7, 14, ..., 2100
    for key in -3..2200 {
        assert_eq!(route(&keys, key), route_linear(&keys, key), "key {key}");
    }
    assert_eq!(route(&[], 5), 0, "a page with a single child has no separators: always child 0");
}
```

```rust test
use std::cmp::Ordering;

fn lower_bound(keys: &[i32], target: i32) -> usize { keys.partition_point(|&k| k < target) }
fn upper_bound(keys: &[i32], target: i32) -> usize { keys.partition_point(|&k| k <= target) }

#[test]
fn lower_and_upper_bound_differ_only_on_equal_keys() {
    let keys = [10, 20, 20, 30];                                      // duplicates make the difference visible (a unique-key leaf has at most one)
    assert_eq!((lower_bound(&keys, 5), upper_bound(&keys, 5)), (0, 0));
    assert_eq!((lower_bound(&keys, 20), upper_bound(&keys, 20)), (1, 3));
    assert_eq!((lower_bound(&keys, 25), upper_bound(&keys, 25)), (3, 3));
    assert_eq!((lower_bound(&keys, 99), upper_bound(&keys, 99)), (4, 4));
}

#[test]
fn the_standard_binary_search_reports_found_or_the_insertion_point() {
    let keys = [10, 20, 30, 40];
    assert_eq!(keys.binary_search(&30), Ok(2));                       // found: the index
    assert_eq!(keys.binary_search(&25), Err(2), "not found: where it would be inserted to keep the order");
    assert_eq!(keys.binary_search(&5), Err(0));
    assert_eq!(keys.binary_search(&99), Err(4));
    // a leaf's lookup and insert are exactly this: Ok(i) = a hit (or a duplicate), Err(i) = the slot to shift into
    let mut leaf = vec![10, 30];
    for key in [20, 30, 5] {
        match leaf.binary_search(&key) {
            Ok(_) => {}                                               // duplicate: refuse
            Err(at) => leaf.insert(at, key),
        }
    }
    assert_eq!(leaf, vec![5, 10, 20, 30]);
    // with a custom order (a comparator that is not Ord on the type): binary_search_by
    let pairs = [(1, "a"), (3, "c"), (5, "e")];
    assert_eq!(pairs.binary_search_by(|(k, _)| k.cmp(&3)), Ok(1));
    assert_eq!(pairs.binary_search_by(|(k, _)| k.cmp(&4)), Err(2));
    assert_eq!(pairs.binary_search_by_key(&5, |&(k, _)| k), Ok(2));
    assert_eq!(2.cmp(&3), Ordering::Less);
}
```

### In the exercises

- **2c-01:** a leaf's lower bound finds the first slot not less than the key, for a lookup and for the place of a new pair.
- **2c-02, 2c-03:** an internal page routes with a binary search over its separator keys, and the same search finds where a new separator goes after a split.

### Where it is used

- **Every B-tree node search**: PostgreSQL's `_bt_binsrch` binary-searches a page's items; SQLite binary-searches the cells of a page.
- **Rust's `BTreeMap`** searches *linearly* inside a node, because a node holds at most 11 keys and a short scan beats branchy binary search at that size. The best algorithm depends on the node size: at 681 keys binary search wins clearly.
- **`std`'s sorted-slice API**: `binary_search*` and `partition_point` are what ranged queries, `sort`-then-`dedup` pipelines, "insert into a sorted `Vec`" and interval lookups are built from.
