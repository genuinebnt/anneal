This is the module's design stage. The table has **three levels**, and you design all of them: a **header page** that maps the top bits of a hash to a directory page; a **directory page** that maps the low bits to a bucket page; and **bucket pages** that hold the key-value pairs. All three live in buffer pool pages (module 1f), so everything the table knows must be in those 8 KiB, and none of it may live in the Rust struct that represents the table. In this stage nothing splits: the buckets are big enough that every key fits, so the work is the structure and the paths through it.

> [!CHECK] An empty table has a header page and nothing else. The first key arrives. List, in order, every page that must exist before the key can be stored, who creates each, and what each page records about the next. Which of those pages would still exist if the program were stopped after the first insert and started again with the same disk?
> ||The key's hash picks a header slot; that slot is empty, so a directory page must be created and its page id written into the header slot. The directory has one slot (depth 0) for now; it points to a bucket page, which must be created, and its page id is written into the directory. Then the pair goes into the bucket. All three pages are in the buffer pool and become dirty; after a flush they are on disk, and the table object holds only the header page's id, so a restart that remembers that one id finds everything else by following page ids. That is the test of a correct design: the Rust struct holds nothing a restart would lose.||
>
> - Where is the directory's page id stored?
> - What does the table struct keep, and why only that?
> - In what order are the pages latched on the way down?

## The task

`DiskExtendibleHashTable<K, V, C>` over a buffer pool. `new(name, bpm, cmp, hash_fn, header_max_depth, directory_max_depth, bucket_max_size)` creates an empty table. The 32-bit hash of a key is `hash_fn.get_hash(&key) as u32`. The structure, which the tests verify only through behaviour:

- **Header:** `2^header_max_depth` slots, each empty or the id of a directory page. A key goes to slot `hash >> (32 - header_max_depth)` (the **top** bits; with depth 0 there is one slot).
- **Directory:** a global depth `g` (starts 0, at most `directory_max_depth`) and `2^g` slots, each holding a bucket page id and a local depth. A key goes to slot `hash & (2^g - 1)` (the **low** bits).
- **Bucket:** at most `bucket_max_size` pairs; keys are unique.

Operations in this stage:

- `get_value(&key) -> Vec<V>`: empty or one element. Read-latch downward: header, then directory, then bucket, letting go of each parent once its child is held.
- `insert(&key, &value) -> bool`: false if the key is already present; creates the directory and the first bucket on demand; true if it went into a bucket that had room. (A full bucket is the next stage: until then it returns false.)
- `remove(&key) -> bool` need not work yet.
- `verify_integrity()`: **you write the checker**. It must panic if your own structure's invariants are broken. The tests call it after every operation.
- `global_depth(&key) -> Option<u32>`: the global depth of the directory that serves the key's header slot (`None` if it has none yet). A read-only observer so that growth and shrinking can be tested without looking inside your pages.
- `get_header_page_id()`, `index_name()`, `default_header_max_depth()` (9), `default_directory_max_depth()` (9) and `default_bucket_max_size()` (as many pairs as fit in your bucket page, hundreds for `(i32, i32)`).

The tests run random insert and lookup sequences against a `HashMap` model for several table shapes, check that keys in different header slots do not disturb each other, that a page worth of keys fits without splitting, and that **no page stays pinned** between operations (a leaked guard fails a test with the page number).

## Your freedom

The layout of all three pages: field order, offsets, sizes, byte order, whether you use the `PageArray` and `FixedSize` of module 2a, a `#[repr(C)]` struct, or plain offsets; whether buckets are sorted or not; what `verify_integrity` checks; and how you name your page types. The tests see none of it.

## The Rust toolbox

**A page is a byte array with a typed view.** A small struct that wraps `&[u8]` (read) or `&mut [u8]` (write) and has methods like `bucket_page_id(slot)` reads and writes by offset. Two impls (one with `AsRef<[u8]>`, one with `AsRef<[u8]> + AsMut<[u8]>`) give read-only methods to both and write methods only to the mutable view, so a read guard cannot change a page.

**Guards are scopes.** `let guard = bpm.read_page(id);` pins and latches; the guard lives to the end of its block or until `drop(guard)`. To hand over from parent to child: take the child's guard first, then `drop(parent)`. The compiler error "cannot borrow ... as mutable because it is also borrowed as immutable" usually means you are still using a view of a page you meant to let go.

**`Option` or a sentinel for "no page".** `PageId::INVALID` (-1) is stored in an unused slot; `is_valid()` tests it. A freshly formatted page must set every slot to invalid, since a new page is zeros, and zero is page 0, a real page.

**Generic over the key and value.** `K: FixedSize + Clone, V: FixedSize + Clone, C: KeyComparator<K>`: the `FixedSize` trait of 2a gives `K::SIZE`, so a bucket's capacity is `(PAGE_SIZE - header) / (K::SIZE + V::SIZE)`.

**Early return with guards.** `return false;` in the middle of a function drops every guard on the way out; that is why guards are better than lock/unlock calls.

## If this is new

- [L3 Lifetimes](/t/l3-lifetimes), first problems: a view struct that borrows a page.
- [L5 Generics & associated types](/t/l5-generics): bounds on an `impl` block.
- [S3 Vec & slices](/t/s3-vec-slices): byte ranges and sub-slices.
- Module 1g's page guards, and module 2a's `FixedSize` and `PageArray`, are what you build on. The optional *extendible hashing* concept explains the structure; *latching a hash table* the order of guards.
- S8 The core traits: Implement by hand: `Eq` + `Hash` consistency.
- [F2 Data layout](/t/f2-data-layout): Size it: `repr(C)` pages and `offset_of!`.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: latch crabbing: take the child, then let go of the parent; lock ordering.
- Y5 Testing & verification: Understand it: an invariant checker run after every operation; scoped threads for a stress test.

## Tests

- An empty table has no values; an inserted key is found with its value; a duplicate is refused and keeps its first value.
- Keys in different header slots do not disturb each other.
- A page worth of keys fits without any splitting (`default_bucket_max_size`).
- The default sizes describe what a page holds.
- For random sequences on tables with roomy buckets, the table behaves like a `HashMap`, `verify_integrity` passes after every step and no page stays pinned.

## Hints

### Start from the empty table

Write the table of "what is in page X" for an empty table and for a table with one key, as bytes. Then write the three views, each with the few accessors the paths need: which accessors does `get_value` use, in order?

### Where is the table's memory?

If your table struct holds a `Vec` of directories, a restart (or a pool eviction!) loses it. Check: could you drop the struct, create a new one from the header page id alone, and still find every key? If not, move the state into the pages.

### The latch order

Always header, then directory, then bucket, and let go of a parent once its child is latched. `get_value` read-latches; `insert` write-latches because it may create or change pages below. What would go wrong if two inserts took the directory and the header in opposite orders?

## Performance

A lookup latches three pages and does a hash and two array reads: roughly the cost of three pool lookups (hundreds of nanoseconds when the pages are resident). The bucket search is linear over up to hundreds of entries if the bucket is unsorted; a sorted bucket with `lower_bound` (module 2a) trades slower inserts for faster lookups.

**Measure it.** Time 100 000 `get_value` calls on a table of 100 000 keys with unsorted and sorted buckets of 200 entries. Predict the ratio from the number of comparisons.

## Experiment

Optional. Predict first, then run.

1. **Lose the pin.** Forget to drop a guard on one early-return path. Which test fails and what does its message say?
2. **A different split of the hash.** Use the low bits for the header and the top bits for the directory. Does any test notice? Which later stage will?

## Other designs

- **BusTub's layout.** A header of `2^h` directory page ids plus its max depth; a directory page with `max_depth`, `global_depth`, an array of local depths and an array of bucket page ids; a bucket page with `size`, `max_size` and an unsorted array of `(key, value)`. Each is a view over a page.
- **Sorted buckets.** `lower_bound` lookups; `O(n)` inserts that shift entries.
- **Bitmap buckets.** A bit per slot says which entries are live; `O(1)` insert and delete, no shifting.
- **No header.** One directory page for the whole table: simpler, but bounded by one page of directory slots.

## In BusTub

`ExtendibleHTableHeaderPage`, `ExtendibleHTableDirectoryPage` and `ExtendibleHTableBucketPage` are classes laid over the page's bytes; the table's `GetValue`, `Insert` and `Remove` latch header, directory and bucket in that order. BusTub's own page tests check the layout; this course checks behaviour instead, because a different layout that behaves the same is as correct.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `reinterpret_cast<ExtendibleHTableDirectoryPage *>(guard.GetDataMut())` | a view struct constructed over `&mut guard[..]` |
| `static_assert(sizeof(Page) <= BUSTUB_PAGE_SIZE)` | `const _: () = assert!(...)` |
| `INVALID_PAGE_ID` | `PageId::INVALID` and `is_valid()` |
| `WritePageGuard header_guard = bpm_->WritePage(header_page_id_)` | `let header_guard = bpm.write_page(id);` |
| `header_guard.Drop()` | `drop(header_guard)` |

**Port rule:** a class overlaid on page bytes becomes a view over a slice with explicit offsets.

## Learn more

- Fagin et al., *Extendible hashing: a fast access method for dynamic files*, TODS 1979
- CMU 15-445 lecture on hash tables · [`PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html)
