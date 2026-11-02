What if the table does not fit in memory? **External sorting** works in two phases. **Pass 0** reads the input a memory-sized piece at a time, sorts each piece in memory and writes it out as a **run**: a sorted sequence of tuples stored on pages. Later passes **merge** runs (stage 3) until one run remains. This stage builds the storage for runs and pass 0. A run is a list of page ids; each page holds a 4-byte tuple count followed by tuples, each stored as a 4-byte length and its bytes. The pages live in the buffer pool, so a big run is evicted and read back by the pool you built in module 1, not by code of the sort.

> [!CHECK] A run builder is given tuples one at a time and the page is 8192 bytes. When does it start a new page, what does it do with a tuple larger than a page, and what is the largest tuple that still fits (with a 4-byte count and a 4-byte length)? If pass 0 fills memory with one page of tuples at a time, how many runs does it make for a table of 100 pages, and how does that choice affect the number of merge passes?
> ||A new page starts when the next tuple (4 + length bytes) does not fit in what is left; a tuple that cannot fit even an empty page is an error (a sort cannot split a tuple); the largest is 8192 − 4 − 4 = 8184 bytes. With one-page runs, 100 pages of input give about 100 runs; merging K-way takes ⌈log_K 100⌉ passes, each reading and writing the whole data. Larger initial runs mean fewer runs and fewer passes, which is why real engines fill all available memory in pass 0.||
>
> - Who writes the count: the builder when it flushes?
> - Where is a partly filled page kept before it is flushed?
> - What does reading a page back cost with the pool full?

## The task

In `src/execution/executors/external_merge_sort_executor.rs` (`finish`, `from_tuples`, `delete_pages` and the constructor shapes are given):

- `RunBuilder::flush_page(&mut self)`: if the buffer holds tuples, allocate a page with `bpm.new_page()`, write the count (4 bytes, little-endian) and then the buffered bytes into it through a write guard, remember the page id and empty the buffer.
- `RunBuilder::push(&mut self, tuple)`: a tuple that cannot fit even an empty page is an `Execution` error; if it does not fit in what is left of the current page, flush the page first; then append the tuple's serialized bytes to the buffer and count it.
- `MergeSortRun::read_page(&self, index) -> Vec<Tuple>`: read the page's count and that many tuples (`Tuple::deserialize_from`; each occupies `4 + length` bytes).
- `RunIterator::next`: the next tuple of the current page; when the page is used up, load the next with `read_page`; `None` after the last page.
- `generate_initial_runs(&mut self) -> Result<Vec<MergeSortRun>>` (`write_sorted_run(entries)`, which sorts entries stably and writes them as a run, is given): initialise the child and read all its batches; keep `(sort key, tuple)` entries in a buffer for as long as their serialized sizes plus the 4-byte count fit in **one page**; when the next tuple would not fit (and the buffer is not empty) write the buffer as a run and start a new one with that tuple; write what is left; return the runs in order.

The tests: exact scenarios (a few tuples fit one page and read back in order; a full page starts the next; an empty run has no pages; a tuple that cannot fit is an error and one that exactly fits is fine; a builder filled tuple by tuple; page-by-page reading matches the iterator; an empty input makes no runs; a small input is one sorted run of one page; a big input is cut into runs of one page each, sorted inside; a page is filled before a new run starts; descending order and stability inside a run; the sort can start again), and two properties: **for random tuple sizes a run reads back its tuples in order and uses exactly the pages the greedy-fill rule predicts**; and **pass 0 on random tables gives one-page runs, each sorted, which together hold exactly the input rows**.

## Your freedom

How the builder buffers (a `Vec<u8>` and a count, or writing into a pinned page directly), and how pass 0 decides a buffer is full (adding sizes, or trying to push).

## The Rust toolbox

**Little-endian bytes.** `(count as u32).to_le_bytes()` writes the count; `u32::from_le_bytes(page[0..4].try_into().unwrap())` reads it (module 2a).

**A write guard for a page.** `let mut guard = bpm.write_page(id); guard.get_data_mut()[..n].copy_from_slice(&bytes);` the latch and the pin are released when `guard` goes out of scope.

**`Vec::resize` and slices.** `buf.resize(at + size, 0); tuple.serialize_to(&mut buf[at..]);` grows the buffer then fills the new part.

**A structure that borrows the pool.** `MergeSortRun<'e>` holds `&'e BufferPoolManager`: the run cannot outlive the pool, and the compiler checks it.

**An iterator over pages.** `RunIterator` holds the run, the page index and the current page's tuples (a `VecDeque<Tuple>` or `vec::IntoIter`); `next` refills when empty.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `copy_from_slice`, `resize`, `to_le_bytes`.
- S6 Iterators: implementing `Iterator` for a struct with a buffer.
- [L3 Lifetimes](/t/l3-lifetimes): a struct that borrows a pool.
- [F7 I/O & serialization](/t/f7-io-serialization): length-prefixed records.
- S9 I/O & filesystem: Understand: spilling to disk: bounded-memory reading and writing.

## Tests

- Runs: a few tuples one page; a full page starts the next; empty runs; a too-large tuple is an error and an exact fit is fine; tuple by tuple; page by page.
- Pass 0: empty input; a small input is one sorted run; a big input is cut into one-page runs; descending and stable; restartable.
- Properties: greedy page fill and exact read-back; runs sorted and complete.

## Hints

### Count the bytes

A page holds `8192 − 4` bytes of tuples. A tuple occupies `4 + data.len()`. Write the arithmetic as a small function and use it in `push` and in pass 0.

### Where is the partly filled page?

In the builder's buffer, not in the pool: it is written once, by `flush_page`, when it is full or at `finish`. Reading is the opposite: `read_page` pins the page, copies the tuples out and lets go.

### Sort the whole buffer, not each batch

Pass 0 sorts what it holds when the buffer is full, then writes it. Sorting each child batch separately and writing them gives sorted but tiny runs.

## Performance

Writing a run is sequential page writes through the pool; reading it back is sequential reads. With runs of one page each, a table of `N` pages makes `N` runs; every merge pass reads and writes `N` pages. The total I/O is `2N × passes`.

**Measure it.** Count page writes (`DiskManager` counters from module 1a) for sorting tables of 100, 1 000 and 10 000 pages; plot against `N log N`.

## Experiment

Optional. Predict first, then run.

1. **Bigger initial runs.** Let pass 0 collect 8 pages of tuples per run. How many runs and merge passes does a 1 000-page table need now?
2. **Replacement selection.** Keep a heap and emit the smallest tuple that is not smaller than the last one written: runs become about twice memory. Sketch what changes in `generate_initial_runs`.

## Other designs

- **Page-sized initial runs (ours, BusTub's).**
- **Memory-sized runs** (any real engine).
- **Replacement selection:** runs of about twice memory on random input.
- **Runs in temporary files** instead of the buffer pool.

## In BusTub

`external_merge_sort_executor.cpp`, `limit_executor.cpp`, `topn_executor.cpp` and `window_function_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`). The 2025 version of the project asks for an external merge sort (`MergeSortRun`, `ExternalMergeSortExecutor<K>`), a top-N executor with a bounded heap, and window functions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class MergeSortRun { std::vector<page_id_t> pages_; }` | `struct MergeSortRun<'e> { pages: Vec<PageId>, bpm: &'e BufferPoolManager }` |
| `WritePageGuard guard = bpm->WritePage(id); auto *data = guard.GetDataMut();` | `let mut guard = bpm.write_page(id); guard.get_data_mut()` |
| `memcpy(data + 4, buf.data(), buf.size())` | `data[4..4 + n].copy_from_slice(&buf)` |
| `class Iterator` with `operator++` | `impl Iterator for RunIterator` |

**Port rule:** a vector of page ids and a pool pointer become a `Vec<PageId>` and a borrowed reference; `memcpy` becomes `copy_from_slice`.

## Learn more

- *Database Management Systems* (Ramakrishnan, Gehrke), external sorting · [`slice::copy_from_slice`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_from_slice)
