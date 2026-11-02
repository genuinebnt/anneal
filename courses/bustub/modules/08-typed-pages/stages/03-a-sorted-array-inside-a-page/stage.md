The last member of BusTub's bucket page and B+ tree pages is `MappingType array_[0]`: an array that continues to the end of the page. Rust has no flexible array member, so this stage builds one: `PageArray<B, T>` is a *view* over a byte container `B` (a `&[u8]` to read, a `&mut [u8]` or `Vec<u8>` to write) that reads and writes entries of type `T: FixedSize` by index. An index page spends most of its time inserting into the middle of this array and removing from it, so those two operations are the heart of the stage, and most of its bugs.

> [!CHECK] `insert_at(index, len, value)` must open a gap at `index` by moving the entries `index..len` one place to the right. Why is a loop that copies entry `i` to `i + 1` from the front of the range wrong, and what do you do instead? Which standard function solves it?
> ||Copying forward overwrites: entry `index` is copied to `index + 1`, which destroys the old entry `index + 1` before it has been moved; every entry ends up equal to the first. The fix is to copy from the back (last entry first), or to use one `memmove`-style copy that handles overlapping ranges: `slice::copy_within(src_range, dest)`.||
>
> - What does "overlapping" mean for a copy within one buffer?
> - Which direction is safe when moving right? When moving left?
> - Why does `copy_from_slice` not work here?

## The task

`PageArray<B, T>` (the struct and `new` are given in `page_array.rs`; `capacity()` is given: `bytes.len() / T::SIZE`). Implement:

- `get(index) -> T` and `set(index, &T)`: panic with a message if `index` is past the capacity; otherwise decode / encode the `T::SIZE` bytes at `index * T::SIZE`.
- `insert_at(index, len, &T)`: the array holds `len` live entries (the page's header says how many; the array does not know). Move entries `index..len` one place to the right and store the value at `index`. Panics if `index > len` or there is no room for `len + 1`.
- `remove_at(index, len)`: move entries `index + 1..len` one place to the left. Panics if `index >= len`.

The property test applies random inserts, removes and sets to a `PageArray` inside a larger buffer and to a `Vec` model, and requires that the array always reads as the model and that the bytes before and after the array's region are never touched. Other tests use `(i64, Rid)` pairs and owned buffers, and check the panics at the edges.

## Your freedom

How you compute offsets, whether you decode through `T::decode` for every access or keep helpers, and whether `insert_at` uses `copy_within` or a loop in the right direction. The behaviour is the `Vec`.

## The Rust toolbox

**A view is a struct with a byte container.** `PageArray<B, T> { bytes: B, _entry: PhantomData<T> }`: `B` can be `&[u8]` (read-only), `&mut [u8]` (writable) or `Vec<u8>` (owned). The bounds `B: AsRef<[u8]>` and `B: AsRef<[u8]> + AsMut<[u8]>` on separate `impl` blocks give read methods to every view and write methods only to writable ones, so writing through a read-only view is a compile error, not a runtime one.

**`PhantomData<T>`** tells the compiler the struct "uses" `T` although it stores no `T`.

**Moving a range inside one slice.** `buf.copy_within(src_start..src_end, dest_start)` copies bytes within the same slice and handles overlap correctly (it is `memmove`). It panics if a range is out of bounds.

**Slice ranges in bytes.** An entry `i` is at `i * T::SIZE .. (i + 1) * T::SIZE`; moving entries `a..b` is moving bytes `a * SIZE .. b * SIZE`.

**`as_ref()` and `as_mut()`.** Inside the impl, `self.bytes.as_ref()` is a `&[u8]` and `self.bytes.as_mut()` a `&mut [u8]` whatever `B` is.

**`assert!` with a message at the boundary.** `assert!(index < self.capacity(), "entry {index} is past the capacity {}", self.capacity())`: format arguments work in panic messages, and a good message names the numbers.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): ranges, `copy_from_slice`, bounds panics.
- [L5 Generics & associated types](/t/l5-generics): bounds on `impl` blocks, `PhantomData`.
- [L2 Borrowing](/t/l2-borrowing): why two `&mut` into one buffer are refused, and why `copy_within` exists.
- Y5 Testing & verification: Understand it: round-trip properties and a `Vec` model.

## Tests

- Random inserts, removes and sets keep the array equal to the `Vec` model, and the bytes around the region are untouched.
- Whole entries (pairs of `i64` and `Rid`) move and overlapping shifts do not smear.
- Capacity counts whole entries only.
- It works over a read-only view and an owned buffer.
- Past-the-capacity, full-array and beyond-`len` operations panic.

## Hints

### Test your off-by-ones on paper

With `len = 4` and `index = 1`: which entries move, to where, and how many bytes? Then `index = len` (append) and `index = 0`.

### Who tracks `len`?

The array does not: callers pass `len` to `insert_at` and `remove_at`. What happens to the bytes past `len`? (They are stale and nobody reads them; the next insert overwrites.)

### A failing property test

The shrunk counterexample is the shortest sequence of operations: read it as a story ("insert 5 at 0; insert 7 at 0; remove at 1") and replay it by hand.

## Performance

`copy_within` is a `memmove` of up to a page: a few hundred nanoseconds for 8 KiB, and linear in the number of entries moved. This is why B+ tree nodes are kept to a modest size: every insert pays for moving half a node on average.

**Measure it.** Insert 500 random keys into a `PageArray<i32>` of 1 000 entries, keeping it sorted, and time it. Then do the same with `Vec<i32>::insert`: should be similar; the page version has no allocation.

## Experiment

Optional. Predict first, then run.

1. **Loop forward.** Replace `copy_within` with a forward loop and run the property test: how long is the shrunk counterexample?
2. **Slot directory.** Instead of shifting entries, keep an array of 2-byte offsets to entries and shift only those. What does it save, and what is the price? (This is the idea of the *slotted page* used by table heaps in module 3b.)

## Other designs

- **Contiguous entries with shifting (ours).** Simple, cache-friendly, `O(n)` insert.
- **Indirection array of offsets** (slotted pages). Cheaper shifts, variable-sized entries, more bookkeeping.
- **Unsorted entries with a bitmap** (what hash buckets use). `O(1)` insert, lookup scans.
- **`zerocopy`/`bytemuck` slices of `T`.** A real `&[T]` view when alignment is guaranteed; no per-access decode.

## In BusTub

```cpp
class ExtendibleHTableBucketPage {
  uint32_t size_; uint32_t max_size_;
  MappingType array_[HTableBucketArraySize(sizeof(MappingType))];
};
```
The array has a compile-time size, but the same shifting happens in B+ tree leaf and internal pages (`array_[0]`).

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `MappingType array_[0]` (flexible array member) | `PageArray<B, T>` over a byte slice |
| `std::memmove(&array_[i + 1], &array_[i], (size - i) * sizeof(T))` | `bytes.copy_within(i * SIZE..len * SIZE, (i + 1) * SIZE)` |
| `array_[i] = value;` | `array.set(i, &value)` |
| undefined behaviour past the end | a panic with the index and capacity |

**Port rule:** a flexible array member becomes a view type; every access is bounds-checked and decoded.

## Learn more

- [`slice::copy_within`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_within) · [`AsRef`](https://doc.rust-lang.org/std/convert/trait.AsRef.html) · [`PhantomData`](https://doc.rust-lang.org/std/marker/struct.PhantomData.html)
