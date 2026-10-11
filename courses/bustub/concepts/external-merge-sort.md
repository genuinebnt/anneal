---
title: External merge sort: sorting more than fits in memory
summary: Why sorts on big tables work in passes of runs, how many I/Os that costs, the k-way merge with a heap, and how a database stores a run on pages.
minutes: 11
---
A table with a billion rows does not fit in memory, but `order by` must still work. **External merge sort** sorts data on disk with a bounded amount of memory, in two ideas: *sort small pieces in memory*, then *merge sorted pieces* repeatedly until one remains.

```svg
caption: Pass 0 turns the input into sorted runs (here one page each); every later pass merges K runs into one, until a single run, the sorted output, remains. With K = 2 and 8 pages that is 3 merge passes; each pass reads and writes every page once.
<svg viewBox="0 0 760 210" role="img" aria-label="Four passes of runs: eight one-page runs, four two-page runs, two four-page runs, one eight-page run">
<defs><marker id="em-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<text class="dim sm" x="16" y="30">pass 0</text>
<g class="live"><rect x="90" y="14" width="60" height="24" rx="2"/><rect x="156" y="14" width="60" height="24" rx="2"/><rect x="222" y="14" width="60" height="24" rx="2"/><rect x="288" y="14" width="60" height="24" rx="2"/><rect x="354" y="14" width="60" height="24" rx="2"/><rect x="420" y="14" width="60" height="24" rx="2"/><rect x="486" y="14" width="60" height="24" rx="2"/><rect x="552" y="14" width="60" height="24" rx="2"/></g>
<text class="dim sm" x="16" y="78">pass 1</text>
<g class="blue"><rect x="90" y="62" width="126" height="24" rx="2"/><rect x="222" y="62" width="126" height="24" rx="2"/><rect x="354" y="62" width="126" height="24" rx="2"/><rect x="486" y="62" width="126" height="24" rx="2"/></g>
<text class="dim sm" x="16" y="126">pass 2</text>
<g class="blue"><rect x="90" y="110" width="258" height="24" rx="2"/><rect x="354" y="110" width="258" height="24" rx="2"/></g>
<text class="dim sm" x="16" y="174">pass 3</text>
<g class="blue"><rect x="90" y="158" width="522" height="24" rx="2"/></g>
<path class="ln" d="M153 40 L153 60" marker-end="url(#em-a)"/><path class="ln" d="M285 88 L285 108" marker-end="url(#em-a)"/><path class="ln" d="M351 136 L351 156" marker-end="url(#em-a)"/>
<text class="dim sm" x="640" y="30">8 runs</text><text class="dim sm" x="640" y="78">4 runs</text><text class="dim sm" x="640" y="126">2 runs</text><text class="dim sm" x="640" y="174">1 run: sorted</text>
</svg>
```

## The cost

With `N` pages of data and merging `K` runs at a time, there are `1 + ⌈log_K N⌉` passes (pass 0 makes the runs), and every pass reads and writes all `N` pages: about `2N · (1 + ⌈log_K N⌉)` page I/Os. A larger `K` (merging more runs at once, which needs `K` input buffers plus one output) means fewer passes; with `K = 2` as in BusTub a 1,000-page sort takes 11 passes, with `K = 100` two. If everything fits in memory, pass 0 alone produces one run and the sort is a plain in-memory sort.

## A run on pages

A **run** is a sorted sequence of tuples stored on a list of pages. Each page holds a count and tuples one after another (each with its length); a run is the list of its page ids. To read a run, an iterator holds the current page and position; to write one, you fill a page, allocate the next when it is full, and remember the ids.

## The merge

To merge `K` sorted runs, keep a cursor on each and repeatedly output the smallest of the `K` current tuples, advancing that cursor. A **min-heap** of `(current tuple, which run)` finds the smallest in `O(log K)`; for `K = 2` a simple comparison does. When a run is exhausted it leaves the heap; the merge ends when all are.

## Why bother when memory is big?

Because databases cannot assume it. A query can sort a result larger than its share of memory; running out is not an option, so the sort spills to disk in a controlled way (and, as a bonus, tests can force it by using a small buffer pool).

## Invariants: what must always be true

- **Every run is sorted,** and merging sorted runs gives a sorted run that holds exactly the tuples of its inputs: nothing lost, nothing duplicated.
- **Passes are `1 + ⌈log_K N⌉`,** each reading and writing every page once, and the last pass leaves exactly one run: the sorted output.
- **Memory stays bounded.** A merge holds one input buffer per run being merged plus one output buffer, never the whole input; that is the point of sorting on disk.
- **A merge ends when every run is exhausted,** and an exhausted run leaves the heap; a run is never read past its end.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `class MergeSortRun { std::vector<page_id_t> pages_; BufferPoolManager *bpm_; class Iterator {...}; }` | `struct MergeSortRun<'e> { pages: Vec<PageId>, bpm: &'e BufferPoolManager }` and `impl Iterator` for the reader |
| `std::priority_queue` of pairs with a comparator | `BinaryHeap` of a wrapper implementing `Ord` (reversed for a min-heap) |
| `bpm->WritePage(pid)` then `memcpy` into `GetDataMut()` | `bpm.write_page(pid)` guard and a slice copy |
| a `delete`d page leaks unless you remember | drop the guard; `bpm.delete_page(pid)` for runs you are done with |

## In real code

### Using it: runs, a k-way merge and passes

```rust test
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Pass 0: cut the input into runs of `run_len` values and sort each.
fn make_runs(input: &[i32], run_len: usize) -> Vec<Vec<i32>> {
    input.chunks(run_len).map(|c| { let mut r = c.to_vec(); r.sort(); r }).collect()
}

/// Merge sorted runs into one with a min-heap of (value, run index, position).
fn merge(runs: &[Vec<i32>]) -> Vec<i32> {
    let mut heap = BinaryHeap::new();
    for (i, r) in runs.iter().enumerate() {
        if let Some(v) = r.first() { heap.push(Reverse((*v, i, 0usize))); }
    }
    let mut out = vec![];
    while let Some(Reverse((v, i, pos))) = heap.pop() {
        out.push(v);
        if let Some(next) = runs[i].get(pos + 1) { heap.push(Reverse((*next, i, pos + 1))); }
    }
    out
}

/// Repeatedly merge `k` runs at a time; returns the sorted data and the number of merge passes.
fn external_sort(input: &[i32], run_len: usize, k: usize) -> (Vec<i32>, usize) {
    let mut runs = make_runs(input, run_len);
    let mut passes = 0;
    while runs.len() > 1 {
        runs = runs.chunks(k).map(|group| merge(group)).collect();
        passes += 1;
    }
    (runs.pop().unwrap_or_default(), passes)
}

#[test]
fn runs_are_sorted_pieces_and_a_merge_joins_them() {
    let runs = make_runs(&[5, 3, 9, 1, 8, 2], 3);
    assert_eq!(runs, vec![vec![3, 5, 9], vec![1, 2, 8]]);
    assert_eq!(merge(&runs), vec![1, 2, 3, 5, 8, 9]);
}

#[test]
fn the_number_of_merge_passes_is_a_logarithm_of_the_number_of_runs() {
    let input: Vec<i32> = (0..64).rev().collect();
    let (sorted, passes) = external_sort(&input, 8, 2); // 8 runs, merged 2 at a time: 3 passes
    assert_eq!(sorted, (0..64).collect::<Vec<_>>());
    assert_eq!(passes, 3);
    let (_, wide) = external_sort(&input, 8, 4); // 8 runs, 4 at a time: 8 -> 2 -> 1
    assert_eq!(wide, 2);
    let (_, one_run) = external_sort(&input, 100, 2); // everything fits in one run: no merging at all
    assert_eq!(one_run, 0);
}

#[test]
fn duplicates_and_odd_counts_are_fine() {
    let (sorted, _) = external_sort(&[2, 2, 1, 3, 3, 3, 0], 2, 3);
    assert_eq!(sorted, vec![0, 1, 2, 2, 3, 3, 3]);
    assert_eq!(external_sort(&[], 4, 2), (vec![], 0));
}
```

### In the exercises

- **3g-02:** `MergeSortRun`, tuples on pages with an iterator over them, and pass 0 (sorted runs of one page each).
- **3g-03:** merging `K` runs into one, and the executor: all passes in `init`, the final run read out in batches.

### Where it is used

- **PostgreSQL**: `tuplesort.c` (quicksort in memory, then "external merge" with polyphase/tape merging when `work_mem` is exceeded; `EXPLAIN ANALYZE` shows `Sort Method: external merge  Disk: ...kB`).
- **SQLite**: the `vdbesort.c` merge sorter with PMAs (sorted runs) merged in passes.
- **Hadoop / Spark**: shuffle sorts spill sorted runs and merge them.
- **LSM-tree stores (RocksDB)**: compaction merges sorted runs (SSTables).
