`LIMIT 10` returns the first ten rows and stops. The point is the *stops*: a limit above a scan reads ten rows, not the table, and a limit above a sort (`ORDER BY price DESC LIMIT 10`) can do better than sorting everything: keep a **bounded heap** of the best ten seen so far, so memory is `N` rows however big the input is, and the work is `O(rows × log N)` with no runs on disk. That executor is called **top-N**; the optimizer (module 3h) turns "sort followed by limit" into it.

> [!CHECK] A `LIMIT 5` executor's child returns batches of 128. How many rows does the limit pass up from the first batch, and what does it do on the next call? For top-N with `N = 3` and the rows 5, 9, 1, 7, 8, 3, 6 sorted descending: what is the heap after each row, which element do you look at to decide whether a new row belongs, and why is that the *worst* of the kept ones? Which of two equal keys is kept?
> ||The limit passes up at most 5 (it asks the child for `min(batch_size, remaining)` and truncates what comes back), and on the next call it answers `false` without calling the child at all. The top-N heap keeps the 3 best; its top is the worst of the kept (the one to evict), so a new row is compared with the top: if it is better, pop and push. After 5, 9, 1: {9,5,1}; then 7 replaces 1 → {9,7,5}; 8 replaces 5 → {9,8,7}; 3 and 6 are worse than 7: no change. Equal keys: the earlier row is kept (a stable sort would put it first), so a new row that only ties the worst is not better.||
>
> - Why must top-N read its whole child, while limit must not?
> - What order do you pop the heap in, to output the best first?
> - What does `N = 0` read?

## The task

In `src/execution/executors/limit_executor.rs` (the struct and `new` are given; `self.limit` is the plan's limit, `self.emitted` the count so far):

- `init`: `emitted = 0` and initialise the child. `next`: if `emitted >= limit` return `false` **without touching the child**; otherwise ask the child for at most `batch_size.min(limit - emitted)` tuples; if it has none return `false`; truncate the batch (and the rids) to that many (a child may return more than it was asked for), add the batch length to `emitted`, and return `true` if the batch is not empty.

In `src/execution/executors/topn_executor.rs` (`HeapEntry`, a wrapper whose `Ord` makes the **worst** kept tuple the greatest, `new`, `get_num_in_heap` and the check executor are given):

- `init`: initialise the child; read all its batches; for each tuple make its sort key (`generate_sort_key`) and a `HeapEntry { entry: (key, tuple), seq, cmp }` (`seq` counts arrivals); push it while the heap holds fewer than `n`; otherwise, if the new entry is smaller than the heap's greatest (`peek`), pop that one and push the new one. After each tuple store `heap.len()` in `self.num_in_heap`. At the end pop the heap into `self.result` in **output order** (best first). An `n` of 0 reads nothing.
- `next`: hand out the next at most `batch_size` tuples of `self.result` (a default rid with each).

The tests: exact scenarios (limit returns the first n rows; larger than the input returns everything; not a multiple of the batch; the rest of the child is never read; after `order by` and in a subquery; `init` starts over; the best n in order; n larger than the input and zero; ties keep the earlier rows; several keys, NULLs and a big input; the heap never holds more than n; re-initialising), and a property: **for random rows and keys, top-N and `order by .. limit n` are exactly the first n rows of the stable sort, and `limit n` returns `min(n, rows)` rows**, for any n including 0 and more than the table has.

## Your freedom

Whether the heap is `BinaryHeap` with the given `Ord` or a sorted `Vec`, and how you stop the limit (counting, or `take`).

## The Rust toolbox

**`BinaryHeap` is a max-heap.** The top is the greatest by `Ord`. `HeapEntry`'s `Ord` is written so that the greatest is the **worst** kept row; `heap.peek()` looks at it and `heap.pop()` removes it.

**`into_sorted_vec`.** `heap.into_sorted_vec()` returns ascending by `Ord`, i.e. best first when the worst is greatest.

**Stable ties with a sequence number.** `HeapEntry { seq, .. }`'s `Ord` breaks ties by arrival order, so the earlier row is better; a heap has no stability of its own.

**`truncate`.** `tuples.truncate(n); rids.truncate(n);` cuts a batch to size.

**`min`.** `batch_size.min(self.limit - self.emitted)` is the most the limit may ask for.

## If this is new

- S5 Queues & heaps: `BinaryHeap`, `peek`, `pop`, `into_sorted_vec`.
- S8 The core traits: implementing `Ord` for a wrapper type.
- [S3 Vec & slices](/t/s3-vec-slices): `truncate`.
- The optional *top-N with a bounded heap* concept.
- Y5 Testing & verification: Build it: a sorted vector as the oracle for sort, limit, top-N and rank.

## Tests

- Limit: first n; larger than input; batch edges; the rest is never read; after `order by`; in a subquery; `init` restarts.
- Top-N: the best n in order; n beyond the input and zero; ties; several keys and NULLs on a big input; the heap bound; re-initialising.
- Property: top-N and sort-then-limit equal the stable sort's prefix.

## Hints

### Why limit asks for `min(batch, remaining)`

A limit of 5 above a child that returns 128 per call would otherwise make the child do the work for 128 rows. Ask for what you need, and truncate defensively.

### The comparison that decides eviction

A new row replaces the top when it is *strictly better* than the worst kept row. If it only ties, the kept one arrived earlier and stays.

## Performance

Limit is `O(limit)`. Top-N is `O(rows × log N)` time and `O(N)` memory, against `O(rows × log rows)` time and `O(rows)` memory (or disk) for sort-then-limit. For `N` of ten and a billion rows that is the difference between a scan and a data centre.

**Measure it.** Compare `order by .. limit 10` through top-N with a full external sort on a million rows.

## Experiment

Optional. Predict first, then run.

1. **Wrong end.** Make the heap's top the *best* row. What does the check executor report, and which test fails first?
2. **Unstable ties.** Drop `seq` from the ordering. Which property fails and with which minimal input?

## Other designs

- **A bounded heap (ours, BusTub's).**
- **Quickselect:** find the N-th element in `O(n)` average, then sort N rows.
- **Sort then limit:** simple, memory-hungry.
- **Index-ordered scan with limit** when an index gives the order for free (module 3e).

## In BusTub

`external_merge_sort_executor.cpp`, `limit_executor.cpp`, `topn_executor.cpp` and `window_function_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`). The 2025 version of the project asks for an external merge sort (`MergeSortRun`, `ExternalMergeSortExecutor<K>`), a top-N executor with a bounded heap, and window functions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::priority_queue<Entry, std::vector<Entry>, Cmp>` | `BinaryHeap<HeapEntry>` with a custom `Ord` |
| `while (!pq.empty()) { out.push_back(pq.top()); pq.pop(); }` then `reverse` | `heap.into_sorted_vec()` |
| `tuple_batch->resize(n)` | `tuples.truncate(n)` |

**Port rule:** a comparator functor for a priority queue becomes an `Ord` impl on a wrapper type.

## Learn more

- [`BinaryHeap`](https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html) · PostgreSQL's "top-N heapsort" in `EXPLAIN`
