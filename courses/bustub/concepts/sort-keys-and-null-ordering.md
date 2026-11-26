---
title: Sort keys and NULL ordering
summary: How ORDER BY with several columns, directions and NULLS FIRST/LAST becomes one comparison function, why a key is computed once per row, and how to write a total order in Rust with Ordering and sort_by.
minutes: 8
---
`order by dept asc, salary desc nulls last` compares two rows by `dept` first; only if those are equal does it look at `salary`, backwards, with NULLs placed last. A sort needs this as a single function `compare(row_a, row_b) -> Ordering`. Writing it right once, and using it in the sort, the top-N heap and the window function, is what this concept is about.

## Three decisions per key

| decision | SQL | BusTub |
|---|---|---|
| **direction** | `asc` (default) / `desc` | `OrderByType::{Default, Asc, Desc}`: `Default` means ascending |
| **where NULLs go** | `nulls first` / `nulls last` | `OrderByNullType::{Default, NullsFirst, NullsLast}` |
| **the default for NULLs** | PostgreSQL: NULL is *larger* than every value (last for asc, first for desc) | BusTub's tests: NULL is *smaller* (first for asc, last for desc) |

BusTub's default (NULL as the smallest value) is visible in `p3.16-sort-limit.slt`: `order by colH limit 3` returns the NULL rows first, `order by colH desc` last. An explicit `nulls first` or `nulls last` overrides the default whatever the direction.

## Compute the key once

An `ORDER BY` item is an expression (`order by colA + colD - colA desc`). A sort compares each row many times (`n log n`), and evaluating expressions inside the comparator would repeat the work and could fail in the middle of a sort. So first compute the **sort key** of each row, the vector of values of its order-by expressions, once (`GenerateSortKey`), then sort `(key, row)` pairs comparing keys only.

## The comparator

Walk the keys in order; for each, compare the two values with the direction and NULL rule; the first non-equal result decides; if all are equal the rows are equal. In Rust that is `Ordering` combinators:

```rust,ignore
a.cmp(b).then_with(|| c.cmp(d))     // first decides, ties go to the second
ord.reverse()                       // descending
```

`sort_by` takes a closure returning `Ordering`, and `sort_by` is **stable**: rows that compare equal keep their input order, which matters for reproducible output. `BinaryHeap` and `BTreeMap` need `Ord` on their element, so a comparator that depends on runtime data (the order-by list) is wrapped in a struct that implements `Ord` by calling it.

## Total orders

A comparator must be a **total order**: antisymmetric, transitive, and defined for every pair. `f64` NaN breaks it (NaN is not equal to itself), and Rust refuses to sort floats with `sort()` for that reason: you must choose a rule (`total_cmp`). A comparator that is not a total order can make a sort panic or loop in Rust, and corrupt a heap.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::sort(v.begin(), v.end(), cmp)` with `cmp` returning `bool` ("a before b"), strict weak ordering | `v.sort_by(\|a, b\| ..)` returning `Ordering` |
| `std::stable_sort` for equal rows in input order | `sort_by` is stable (`sort_unstable_by` is the fast, unstable one) |
| `std::priority_queue` with a comparator type | `BinaryHeap<T>` where `T: Ord` (wrap data and comparator in a struct) |
| `TupleComparator::operator()(SortEntry a, SortEntry b) -> bool` | `Ordering` instead of a bool |

## In real code

### Using it: a comparator from a list of keys

```rust test
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Dir { Asc, Desc }
#[derive(Clone, Copy, PartialEq, Debug)]
enum Nulls { Default, First, Last }

#[derive(Clone, Copy)]
struct Key { dir: Dir, nulls: Nulls }

type V = Option<i32>; // None is NULL

fn compare_one(a: V, b: V, k: Key) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) | (Some(_), None) => {
            // where do NULLs go? an explicit rule wins; the default is "NULL is the smallest value"
            let null_first = match k.nulls {
                Nulls::First => true,
                Nulls::Last => false,
                Nulls::Default => k.dir == Dir::Asc,
            };
            let a_is_null = a.is_none();
            if a_is_null == null_first { Ordering::Less } else { Ordering::Greater }
        }
        (Some(x), Some(y)) => match k.dir {
            Dir::Asc => x.cmp(&y),
            Dir::Desc => y.cmp(&x),
        },
    }
}

fn compare_rows(a: &[V], b: &[V], keys: &[Key]) -> Ordering {
    for ((x, y), k) in a.iter().zip(b).zip(keys) {
        let ord = compare_one(*x, *y, *k);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn sorted(rows: &[Vec<V>], keys: &[Key]) -> Vec<Vec<V>> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| compare_rows(a, b, keys)); // stable
    rows
}

const ASC: Key = Key { dir: Dir::Asc, nulls: Nulls::Default };
const DESC: Key = Key { dir: Dir::Desc, nulls: Nulls::Default };

#[test]
fn the_default_puts_null_first_ascending_and_last_descending() {
    let rows = vec![vec![Some(2)], vec![None], vec![Some(1)]];
    assert_eq!(sorted(&rows, &[ASC]), vec![vec![None], vec![Some(1)], vec![Some(2)]]);
    assert_eq!(sorted(&rows, &[DESC]), vec![vec![Some(2)], vec![Some(1)], vec![None]]);
}

#[test]
fn nulls_first_and_last_override_the_direction() {
    let rows = vec![vec![Some(2)], vec![None], vec![Some(1)]];
    let desc_nulls_first = Key { dir: Dir::Desc, nulls: Nulls::First };
    let asc_nulls_last = Key { dir: Dir::Asc, nulls: Nulls::Last };
    assert_eq!(sorted(&rows, &[desc_nulls_first]), vec![vec![None], vec![Some(2)], vec![Some(1)]]);
    assert_eq!(sorted(&rows, &[asc_nulls_last]), vec![vec![Some(1)], vec![Some(2)], vec![None]]);
}

#[test]
fn later_keys_break_ties_and_the_sort_is_stable() {
    let rows = vec![vec![Some(1), Some(9)], vec![Some(1), Some(3)], vec![Some(0), Some(5)], vec![Some(1), Some(3)]];
    assert_eq!(
        sorted(&rows, &[ASC, DESC]),
        vec![vec![Some(0), Some(5)], vec![Some(1), Some(9)], vec![Some(1), Some(3)], vec![Some(1), Some(3)]]
    );
    // the two equal rows keep their input order: marker in a third column
    let tagged = vec![vec![Some(1), Some(1), Some(100)], vec![Some(1), Some(1), Some(200)]];
    assert_eq!(sorted(&tagged, &[ASC, ASC]), tagged);
}
```

### In the exercises

- **3g-01:** `generate_sort_key` and `TupleComparator` are `compare_rows` over `Value`s.
- **3g-02, 3g-03:** the external merge sort compares sort entries with it.
- **3g-04:** the top-N heap orders its entries with it.
- **3g-05:** window functions sort and compare partitions with it.

### Where it is used

- **PostgreSQL**: `SortSupport` comparators per key, with `NULLS FIRST/LAST` and `ASC/DESC` flags on each sort key.
- **DuckDB**: sort keys are encoded into bytes (with the direction and NULL placement baked in) so that a `memcmp` compares rows.
- **Rust**: `Ordering::then_with`, `sort_by_key` with `Reverse`, and `BinaryHeap` with a wrapper that implements `Ord`.
- **Spark / Trino**: `SortOrder(direction, nullOrdering)` per expression.
