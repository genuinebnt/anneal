The last member of BusTub's bucket page and B+ tree pages is `MappingType array_[0]`: an array that runs to the end of the page. Rust has no flexible array member, so this stage builds one. `PageArray<B, T>` is a *view* over a byte container `B` (a `&[u8]` to read, a `&mut [u8]` or `Vec<u8>` to write) that reads and writes entries of type `T: FixedSize` by index. An index page spends most of its time doing two things to this array: finding where a key is, or would be, and opening or closing a gap there. Those two operations are the stage, and they are where most index bugs live.

The search is `lower_bound`: the first position whose entry is **not less than** the target. One answer does three jobs: *lookup* (is the entry at that position equal to the target?), *insert* (that position is where a new entry goes to keep the page sorted) and *range start* (the first key at or above a lower bound). The shift is a copy of a range onto an overlapping range of the same buffer, which is the oldest bug in C. A page that gets both right gets the whole B+ tree right.

> [!CHECK] A page holds `[10, 20, 30, 40]` (len 4) and you insert 25. Where does `lower_bound` point, which entries move, and what does the page look like if the gap is opened by a loop that copies entry `i` to `i + 1` starting from the front of the range? What does `lower_bound` return for 50, and for an empty page?
> ||`lower_bound(25)` is 2, the first entry not less than 25 (30). Entries 2 and 3 move one place right. A front-to-back loop copies 30 into slot 3, destroying 40 before it has moved, then copies slot 3 (now 30) into slot 4: the page becomes `[10, 20, 25, 30, 30]`. Copy from the back, or use one copy that handles overlap (`copy_within`, which is `memmove`). For 50 it returns 4, which is `len`; for an empty page 0, which is also `len`. In every case the answer is "where it would go", and the caller checks `index < len` before reading.||
>
> - Which direction is safe when moving entries right? When moving left?
> - Why is "the matching index or -1" a worse interface for an index page?
> - Why does `copy_from_slice` not work for this shift?

## The task

`PageArray<B, T>` (the struct, `new` and `capacity` are given in `page_array.rs`: the capacity is the number of whole entries that fit). Implement `get`, `set`, `insert_at`, `remove_at` and `lower_bound`:

- **It behaves like a `Vec` of its live entries.** The array does not know how many entries are live (the page's header says), so `insert_at(index, len, value)` and `remove_at(index, len)` take the length. `get` and `set` read and write entry `index`; `insert_at` makes room at `index` by moving entries `index..len` one place right; `remove_at` closes the gap by moving `index + 1..len` one place left. After any sequence of operations the first `len` entries read exactly as `Vec::insert` and `Vec::remove` would leave them, and no byte outside the container is touched.
- **Caller bugs panic, with the numbers.** `get` or `set` past the capacity, `insert_at` with `index > len` or no room for `len + 1`, and `remove_at` with `index >= len` panic with a message that names the index and the length or capacity.
- **`lower_bound(len, cmp)` is the first index in `0..len` whose entry is not `Less` than the target**, where `cmp` compares an entry with the target (`cmp(&entry)` is `Less` if the entry is smaller); `len` if every entry is less. The first `len` entries are sorted; entries at or past `len` are stale bytes and are never looked at.
- **A search is logarithmic.** A search of 1 024 entries calls `cmp` at most 12 times, whatever the target. Any method that meets that bound is fine; a scan is not.

> [!ASIDE] The steps, if you would rather not work them out
> 1. Entry `i` is at bytes `i * T::SIZE .. (i + 1) * T::SIZE`. `get` asserts `index < capacity` and decodes that range; `set` asserts and encodes into it.
> 2. `insert_at` asserts `index <= len && len < capacity`, moves bytes `index * SIZE .. len * SIZE` to start at `(index + 1) * SIZE` with `copy_within`, then sets entry `index`.
> 3. `remove_at` asserts `index < len` and moves bytes `(index + 1) * SIZE .. len * SIZE` to start at `index * SIZE`.
> 4. `lower_bound`: `lo = 0`, `hi = len`; while `lo < hi`, take `mid = lo + (hi - lo) / 2`; if the entry at `mid` is `Less`, `lo = mid + 1`, otherwise `hi = mid`. Return `lo`.

## Your freedom

How you compute offsets and whether you keep helpers for them; whether the shift is one `copy_within` or a loop that runs in the safe direction; and how you search: the classic halving loop, a branch-free version that halves a fixed number of times, or a recursive one, fetching each probed entry once or not. The tests use only the five public methods, and the behaviour is the `Vec` and `partition_point`.

## The Rust toolbox

**A view is a struct with a byte container.** `View<B> { bytes: B }`, where `B` can be `&[u8]` (read-only), `&mut [u8]` (writable) or `Vec<u8>` (owned). Bounds on separate `impl` blocks, `B: AsRef<[u8]>` for reads and `B: AsRef<[u8]> + AsMut<[u8]>` for writes, give read methods to every view and write methods only to writable ones, so writing through a read-only view is a compile error. `PhantomData<T>` tells the compiler the struct uses `T` although it stores none.

**`as_ref()` and `as_mut()`.** Inside the impl, `self.bytes.as_ref()` is a `&[u8]` and `self.bytes.as_mut()` a `&mut [u8]`, whatever `B` is.

**Moving a range inside one slice.** `buf.copy_within(src_start..src_end, dest_start)` copies within the same slice and handles overlap correctly (it is `memmove`). It panics if a range is out of bounds. You cannot do this with `copy_from_slice`, because that would need a `&` and a `&mut` into one buffer at once.

**`assert!` with a message at the boundary.** `assert!(index < self.capacity(), "entry {index} is past the capacity {}", self.capacity())`: format arguments work in panic messages, and a good message names the numbers.

**A comparison closure instead of a value.** `lower_bound(len, |entry| entry.cmp(&target))` lets the caller decide what "compare" means (by key only, through a comparator, descending) without the array knowing about keys. `impl FnMut(&T) -> Ordering` accepts any closure; `FnMut` rather than `Fn` lets a test count calls.

**`lo + (hi - lo) / 2`, not `(lo + hi) / 2`.** The second can overflow for huge arrays. With page-sized arrays it cannot, but the habit is free.

**`partition_point` as the oracle.** `slice.partition_point(|x| x < &target)` is the standard library's lower bound; the tests compare against it, and you can check your own code against any array with it.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): ranges, `copy_from_slice`, bounds panics, `binary_search`, `partition_point`.
- [L5 Generics & associated types](/t/l5-generics): bounds on `impl` blocks, `PhantomData`, closures as parameters (`impl FnMut`).
- [L2 Borrowing](/t/l2-borrowing): why two `&mut` into one buffer are refused, and why `copy_within` exists.
- [P3 Binary search practice](/t/p3-binary-search-practice) (the practice track): the standard variants, `lower_bound` among them.
- [D4 Binary search](/t/d4-binary-search): Binary search: the lower-bound loop and its invariant.
- Y5 Testing & verification: Understand it: round-trip properties and a `Vec` model.

## Tests

- Random inserts, removes and sets keep the array equal to a `Vec` model, whole `(i64, Rid)` entries included, and the bytes around its region untouched.
- Capacity counts whole entries; read-only and owned views work; going past the capacity, inserting into a full array or past `len`, and removing past `len` panic.
- `lower_bound` equals `partition_point` on random sorted arrays with duplicates, ignores entries past `len`, gives `len` and 0 at the edges, and finds `(GenericKey, Rid)` entries by key through a comparator.
- Inserting at the lower bound keeps an array sorted, and a search of 1 024 entries makes at most 12 comparisons.

## Hints

### Who tracks `len`?

Not the array: callers pass `len` to `insert_at`, `remove_at` and `lower_bound`, because the count lives in the page's header, next to the array. What happens to the bytes past `len`? They are stale, nobody reads them, and the next insert overwrites them. Which of your methods could read them by mistake?

### Test your off-by-ones on paper

With `len = 4` and `index = 1`: which entries move, to where, and how many bytes? Then `index = len` (an append, where nothing moves) and `index = 0`. A property test failure comes with a shrunk counterexample, the shortest failing sequence: read it as a story ("insert 5 at 0; insert 7 at 0; remove at 1") and replay it by hand.

### Duplicates and the comparison count

If three entries equal the target, the first must be returned. Which branch of your loop moves `hi` when the entry equals the target? If the count test fails, you are scanning part of the range or calling the closure twice per probe.

### Draw the invariant

`lo..hi` always contains the answer: every entry before `lo` is less than the target, and every entry from `hi` on is not. Write `lo` and `hi` under a sorted row of eight numbers and run your loop by hand for a target in the middle, at both ends, and present twice. Each step must keep the invariant and shrink the range; when `lo == hi`, the invariant says that is the answer.

## Performance

`copy_within` is a `memmove` of up to a page: typically a few hundred nanoseconds for 8 KiB, and linear in the number of entries moved. Every sorted insert pays for moving half the entries on average, which is one reason B+ tree nodes are kept to a page. The search is `log2(n)` probes, 9 for a leaf of 500 entries, each decoding one entry (a cache miss at worst); a tree of height 3 finds any of a hundred million keys in about 27 probes.

**Measure it.** Insert 500 random keys into a `PageArray<_, i32>` of 1 000 entries, keeping it sorted with `lower_bound` and `insert_at`, and time it against `Vec<i32>::insert` at `partition_point`: they should be close, and the page has no allocation. Then time one million `lower_bound` calls on 500 `(GenericKey<8>, Rid)` entries against a linear scan. At what size does the scan stop winning?

## Experiment

Optional. Predict first, then run.

1. **Loop forward.** Replace the shift in `insert_at` with a forward loop and run the property test. How long is the shrunk counterexample, and does it look like the check-yourself answer?
2. **Branch-free.** Rewrite the search so the `if` becomes arithmetic on the bounds (halve a length, move a base). Does it measure faster on your machine, and does it still meet the comparison bound?
3. **Slot directory.** Instead of shifting entries, keep an array of 2-byte offsets to the entries and shift only those. What does it save, and what does it cost? This is the slotted page used by table heaps in module 3b.

## Other designs

- **Contiguous entries with shifting, binary search on the encoded page (ours).** Simple, cache-friendly, `O(n)` insert, one decode per probe.
- **An indirection array of offsets** (slotted pages). Cheaper shifts and variable-sized entries, more bookkeeping.
- **Unsorted entries with a bitmap** (what hash buckets use). `O(1)` insert, lookups scan.
- **Linear scan.** Faster below roughly 16 entries; the crossover depends on the machine.
- **Eytzinger layout.** Reorders entries for cache-friendly search; makes inserts hard.
- **`zerocopy` or `bytemuck` slices of `T`.** A real `&[T]` view when alignment is guaranteed, with no per-access decode.

## In BusTub

```cpp
class ExtendibleHTableBucketPage {
  uint32_t size_; uint32_t max_size_;
  MappingType array_[HTableBucketArraySize(sizeof(MappingType))];
};
```

The bucket array has a compile-time size; B+ tree leaf and internal pages end in `array_[0]` and shift it the same way. BusTub leaves the search to the student: leaf pages binary search their keys with the comparator, and internal pages do the same to pick the child to descend into.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `MappingType array_[0]` (flexible array member) | `PageArray<B, T>` over a byte slice |
| `std::memmove(&array_[i + 1], &array_[i], (size - i) * sizeof(T))` | `bytes.copy_within(i * SIZE..len * SIZE, (i + 1) * SIZE)` |
| undefined behaviour past the end | a panic with the index and capacity |
| `std::lower_bound(begin, end, key, comp)` with a `bool` "less than" | `array.lower_bound(len, \|e\| comparator.compare(e, &key))` with an `Ordering` |

**Port rule:** a flexible array member becomes a view type whose every access is bounds-checked and decoded, and `std::lower_bound` becomes a position-returning search with a three-way comparison.

## Learn more

- [`slice::copy_within`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_within) · [`AsRef`](https://doc.rust-lang.org/std/convert/trait.AsRef.html) · [`PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html)
- [`slice::partition_point`](https://doc.rust-lang.org/std/primitive.slice.html#method.partition_point) · [`slice::binary_search_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.binary_search_by) · [Binary search on Wikipedia](https://en.wikipedia.org/wiki/Binary_search_algorithm) (the lower-bound and upper-bound variants)
