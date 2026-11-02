Merging sorted runs is the heart of an external sort: keep an iterator and a **head** (the next tuple) per run, pick the smallest head, write it out, refill that head. Two merge details decide whether the whole sort is right. It must be **stable**: when heads compare equal, the one from the earlier run wins, so rows with equal keys stay in their input order and `ORDER BY a` followed by another sort works as expected. And it must **release** what it consumes: merged runs' pages go back to the pool (`delete_pages`), or a big sort leaks its whole input. The executor then repeats merging `K` runs at a time, pass after pass, until one run is left, and hands that run out in batches.

> [!CHECK] Three runs have the heads `(5, run 0)`, `(5, run 1)`, `(3, run 2)`. In what order do the next three tuples come out, and which run must supply the second `5`? Why is "pick the smallest head by scanning all heads" fine for `K = 2` and what would you use for `K = 64`? Finally, how many tuples are held in memory by a merge, regardless of the input size?
> ||`3` (run 2), then `5` from run 0, then `5` from run 1: ties go to the earlier run, which is what keeps the sort stable. A linear scan of `K` heads costs `K` comparisons per output tuple, fine for 2; for 64 a binary heap (priority queue) of heads makes it `log K`. A merge holds one head per run plus one output page being built: `K + 1` tuples and pages, independent of input size, which is the point of the algorithm.||
>
> - What happens at the end of a run: does its head become `None`?
> - When do you delete the input runs' pages?
> - What does the executor do with a leftover group of one run?

## The task

In `src/execution/executors/external_merge_sort_executor.rs` (`new`, the factory wiring and `Drop`, which frees the last run's pages, are given):

- `merge_runs(&self, runs: Vec<MergeSortRun>) -> Result<MergeSortRun>`: keep for every run an iterator and its current head `(sort key, tuple)` (`generate_sort_key`; `None` once exhausted); repeatedly choose the head that comes first by `self.cmp.compare(..)`, **the earlier run winning ties**; push its tuple into a `RunBuilder` and refill that head; when no heads are left `finish()` the builder, **free the input runs' pages** (`delete_pages`, after the iterators are dropped) and return the merged run.
- `init`: free the pages of an earlier sort if there was one; `generate_initial_runs()`; while there is more than one run, merge them `K` at a time into the next pass's runs (a leftover group of one run is carried over unchanged); keep the last run in `self.sorted`; reset the reading position.
- `next`: hand out the next at most `batch_size` tuples of the sorted run (page by page, via `read_page` into a buffer), a default rid with each; `true` if the batch is not empty; no run (an empty input) means `false`.

The tests: exact scenarios (two runs merge into one; many runs of different lengths; equal keys come from the earlier run first; descending order and NULLs; a merged run can be merged again; a lot of data through a small pool; `order by` ascending and descending; several keys, expressions and NULLs; a sort bigger than the buffer pool; equal keys keep their input order; an empty table and a single row; strings and running a sort twice; output in batches), and two properties: **merging random sorted runs gives the stable merge** (the sorted whole with ties in run order), and **`order by` on random rows (NULLs, several keys, both directions, `NULLS FIRST/LAST`, up to 1 200 rows through a 24-frame pool) is the model's stable sort**.

## Your freedom

How you pick the smallest head (a scan or a binary heap), how you group runs into passes, and how you buffer output.

## The Rust toolbox

**A struct of heads.** `heads: Vec<Option<SortEntry>>` with one slot per run; `best = heads.iter().enumerate().filter_map(..).min_by(..)`; `min_by` returns the **first** minimum on ties, which is exactly the stable rule.

**`take` and refill.** `let (_, tuple) = heads[i].take().unwrap(); heads[i] = iters[i].next().map(|t| (key(&t), t));` moves the winner out and replaces it.

**`BinaryHeap` with `Reverse`.** A heap of `(Reverse(entry), run_index)` gives `log K` merging; to make ties pick the earlier run, order by `(key, run_index)`.

**`chunks`.** `runs.chunks(K)` groups the runs of one pass; consuming them needs `into_iter` over a `Vec<Vec<_>>` or `drain`.

**Dropping iterators before deleting pages.** `drop(iters);` then `run.delete_pages();` the borrow checker insists the iterators (which borrow the runs) are gone first.

## If this is new

- S5 Queues & heaps: `BinaryHeap`, `Reverse`, `VecDeque`.
- S6 Iterators: `min_by`, `filter_map`, `chunks`.
- [S3 Vec & slices](/t/s3-vec-slices): `drain`, `take`.
- The optional *external merge sort* concept.
- S9 I/O & filesystem: Understand: spilling to disk: bounded-memory reading and writing.
- Y5 Testing & verification: Build it: a sorted vector as the oracle for sort, limit, top-N and rank.

## Tests

- Merging: two runs; many runs; ties from the earlier run; descending and NULLs; a merged run merges again; a lot of data through a small pool.
- The executor: ascending and descending; several keys; bigger than the pool; stability; empty and single-row tables; strings; run twice; output in batches.
- Properties: stable merge; `order by` equals the stable sort.

## Hints

### Stable means "earlier run wins ties"

Within a run the order is already the input order (the initial runs were sorted stably); across runs, earlier runs hold earlier input. Merge them by `(key, run index)` and the whole sort is stable.

### Passes

With `K = 2` and 100 runs the first pass makes 50, then 25, 13, 7, 4, 2, 1. The odd run out is carried to the next pass unmerged.

### Freed pages

If a big sort fails with "out of frames" or a test says the pool is full, you are not deleting the merged runs' pages.

## Performance

Each pass reads and writes every page once; `⌈log_K runs⌉` passes. With `K = 2` a 1 000-page table needs ten passes, with `K = 16` three. CPU per tuple is `K` comparisons with a scan, `log K` with a heap.

**Measure it.** Sort 100 000 rows with `K = 2`, 4 and 16; compare page I/O and time, and find the `K` that stops helping.

## Experiment

Optional. Predict first, then run.

1. **Unstable.** Make ties go to the later run. Which tests fail?
2. **Leak.** Skip `delete_pages` for merged runs. How many sorts of 1 000 rows can a 24-frame pool survive?

## Other designs

- **K-way merge with a linear scan (ours).**
- **K-way merge with a heap** or a loser tree.
- **Polyphase merge:** uneven run counts, fewer passes with few tapes.
- **Sort in memory when it fits** (the executor checks the size first).

## In BusTub

`external_merge_sort_executor.cpp`, `limit_executor.cpp`, `topn_executor.cpp` and `window_function_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`). The 2025 version of the project asks for an external merge sort (`MergeSortRun`, `ExternalMergeSortExecutor<K>`), a top-N executor with a bounded heap, and window functions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::priority_queue<Head, std::vector<Head>, Greater>` | `BinaryHeap<Reverse<(SortKey, usize)>>` |
| `std::inplace_merge` / `std::merge` | your merge over run iterators |
| `bpm_->DeletePage(id)` in a destructor | `run.delete_pages()` explicitly, `Drop` for the last run |

**Port rule:** a destructor that frees pages stays a `Drop` impl; a priority queue of structs becomes a `BinaryHeap` of `Reverse` tuples.

## Learn more

- [`BinaryHeap`](https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html) · *The Art of Computer Programming* vol. 3, section 5.4 (external sorting)
