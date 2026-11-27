---
title: Slices, copy_within and binary search inside a page
summary: How a sorted array lives inside a byte slice, why insertion shifts bytes with a memmove-style copy_within, and the lower-bound binary search that finds where a key goes, with its off-by-one traps.
minutes: 10
---
A bucket or B+ tree node keeps its entries **sorted in an array inside the page**. Inserting in the middle means making a gap; finding a key means searching without scanning. Both are small, both are classic bug magnets.

## A sorted array in raw bytes

Entry `i` of a `PageArray<B, T>` occupies bytes `[i·SIZE, (i+1)·SIZE)` of the array region. Reading and writing go through `FixedSize`:

```rust
pub fn get(&self, index: usize) -> T {
    assert!(index < self.capacity(), "entry {index} is past the capacity {}", self.capacity());
    let at = index * T::SIZE;
    T::decode(&self.bytes.as_ref()[at..at + T::SIZE])
}
```

The `assert!` is the bounds check C++ leaves out; the slice index `[at..at + SIZE]` is a second, automatic one.

## Insert: open a gap with `copy_within`

To put a new entry at position `index` in an array that currently holds `len` entries, everything from `index` to `len - 1` moves up one place:

```rust
pub fn insert_at(&mut self, index: usize, len: usize, value: &T) {
    assert!(index <= len && len < self.capacity(), "no room to insert at {index} (len {len}, capacity {})", self.capacity());
    let size = T::SIZE;
    self.bytes.as_mut().copy_within(index * size..len * size, (index + 1) * size);   // shift right by one entry
    self.set(index, value);
}
```

`slice.copy_within(src_range, dest)` is `memmove`: the source and destination **may overlap** and it copies as if through a temporary. That matters here, because the two ranges overlap by `len - index - 1` entries. C's `memcpy` with overlapping ranges is undefined behaviour; it must be `memmove`. Rust has two different operations to make the distinction visible: `copy_from_slice` (no overlap; a *different* slice, which the borrow checker guarantees) and `copy_within` (the same slice, overlap allowed).

```svg
caption: Inserting 7 at index 2 of the sorted array [2, 4, 8, 9] (len 4). copy_within moves entries 2 and 3 up one place (an overlapping move), and then the new entry is written into the gap. The vacated bytes need not be cleared: len is stored elsewhere.
<svg viewBox="0 0 760 230" role="img" aria-label="Before and after of inserting 7 at index 2 in a sorted array of fixed-size entries">
<defs><marker id="sl-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--fn)"/></marker></defs>
<text class="dim sm" x="20" y="24">before (len = 4)</text>
<rect class="box" x="20" y="34" width="110" height="40" rx="3"/><text class="mid fg" x="75" y="59">2</text>
<rect class="box" x="134" y="34" width="110" height="40" rx="3"/><text class="mid fg" x="189" y="59">4</text>
<rect class="blue" x="248" y="34" width="110" height="40" rx="3"/><text class="mid t-b" x="303" y="59">8</text>
<rect class="blue" x="362" y="34" width="110" height="40" rx="3"/><text class="mid t-b" x="417" y="59">9</text>
<rect class="never" x="476" y="34" width="110" height="40" rx="3"/><text class="mid dim sm" x="531" y="59">unused</text>
<path class="ln-b" d="M300 80 C300 100 360 100 360 112" marker-end="url(#sl-a)"/><path class="ln-b" d="M414 80 C414 100 474 100 474 112" marker-end="url(#sl-a)"/>
<text class="t-b sm" x="560" y="98">copy_within(2*S..4*S, 3*S)</text>
<text class="dim sm" x="20" y="130">after</text>
<rect class="box" x="20" y="140" width="110" height="40" rx="3"/><text class="mid fg" x="75" y="165">2</text>
<rect class="box" x="134" y="140" width="110" height="40" rx="3"/><text class="mid fg" x="189" y="165">4</text>
<rect class="hot" x="248" y="140" width="110" height="40" rx="3"/><text class="mid t-a" x="303" y="165">7</text>
<rect class="blue" x="362" y="140" width="110" height="40" rx="3"/><text class="mid t-b" x="417" y="165">8</text>
<rect class="blue" x="476" y="140" width="110" height="40" rx="3"/><text class="mid t-b" x="531" y="165">9</text>
<text class="t-a sm" x="248" y="200">set(2, 7): the new entry; len is now 5</text>
<text class="dim sm" x="20" y="222">lower_bound(len, |e| e.cmp(&amp;7)) = 2: the position where 7 belongs</text>
</svg>
```

Removal is the mirror image: `copy_within((index + 1) * size..len * size, index * size)` shifts the tail *down*, overwriting the removed entry. Neither needs to clear the vacated slot: `len` is stored elsewhere (the page header), so bytes past `len` are simply unused.

## Where does it go? Lower bound

`lower_bound(len, cmp)` returns the **first index whose entry is not less than the key** (so `len` if the key is bigger than everything). It is the position an insert goes to *and* the position to look at for a lookup:

```rust
pub fn lower_bound(&self, len: usize, mut cmp: impl FnMut(&T) -> Ordering) -> usize {
    let (mut lo, mut hi) = (0, len);          // the answer is in lo..=hi
    while lo < hi {
        let mid = lo + (hi - lo) / 2;         // not (lo + hi) / 2: that can overflow
        if cmp(&self.get(mid)) == Ordering::Less { lo = mid + 1; } else { hi = mid; }
    }
    lo
}
```

The invariant: **everything before `lo` is Less; everything from `hi` on is not Less.** Each step halves `hi - lo`; when they meet, that is the boundary. The closure `cmp` compares an entry with the key being sought, so the same function serves lookup, insert and range scans.

| trap | what goes wrong |
|---|---|
| `hi = len - 1` and `while lo <= hi` | the "closed interval" version: needs signed arithmetic (`hi` can go to `-1`) or a special case for an empty array |
| `mid = (lo + hi) / 2` | can overflow for large indices (not a risk at 511 entries, but a classic bug in the original JDK `binarySearch`) |
| `lo = mid` instead of `mid + 1` | an infinite loop when `hi == lo + 1` |
| using `==` to stop early | finds *a* match, not the *first*; breaks with duplicates and cannot report the insertion point |

`std` has `slice::partition_point` and `binary_search_by`, which do this over a real slice. They cannot be used directly here because the entries are encoded bytes, not a `[T]`: you would have to decode the whole array first.

| C++ | Rust |
|---|---|
| `std::memmove(dst, src, n)` | `slice.copy_within(range, dest)` |
| `std::memcpy` (no overlap) | `dst.copy_from_slice(src)` |
| `std::lower_bound(first, last, key)` | `slice.partition_point(\|x\| x < key)` / this function |
| `std::copy_backward` | `copy_within` (handles direction) |

## In real code

### The API you will use

| call | what it does | when |
|---|---|---|
| `&buf[a..b]` / `&mut buf[a..b]` | a sub-slice (bounds-checked) | views of a page |
| `s.copy_within(src_range, dest)` | `memmove` inside one slice, overlap allowed | open or close a gap |
| `dst.copy_from_slice(src)` | `memcpy` between different slices (same length or it panics) | writing an entry |
| `s.split_at_mut(i)` | two disjoint `&mut` halves | writing two fields at once |
| `s.chunks(n)` / `chunks_exact(n)` / `chunks_mut(n)` | iterate fixed-size pieces | entries of `SIZE` bytes |
| `s.binary_search(&x)` / `binary_search_by(\|e\| ..)` / `partition_point(\|e\| ..)` | search a **sorted** slice: `Result<usize, usize>` / the first index where the predicate turns false | when you have a real `[T]` |
| `s.fill(0)` / `s.iter().all(..)` / `s.rotate_left(k)` | fill, test, rotate | zeroing, checking |

```rust test
#[test]
fn shifting_entries_inside_a_byte_array() {
    const SIZE: usize = 4;                                       // 4-byte entries
    let mut page = [0u8; 8 * SIZE];
    for (i, v) in [2u32, 4, 8, 9].iter().enumerate() {
        page[i * SIZE..(i + 1) * SIZE].copy_from_slice(&v.to_le_bytes());
    }
    let len = 4;
    let at = 2;                                                  // insert 7 before 8
    page.copy_within(at * SIZE..len * SIZE, (at + 1) * SIZE);    // overlapping move: memmove, not memcpy
    page[at * SIZE..(at + 1) * SIZE].copy_from_slice(&7u32.to_le_bytes());
    let got: Vec<u32> = page.chunks_exact(SIZE).take(len + 1).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(got, vec![2, 4, 7, 8, 9]);

    page.copy_within((1 + 1) * SIZE..(len + 1) * SIZE, 1 * SIZE);     // remove entry 1: the tail moves down
    let got: Vec<u32> = page.chunks_exact(SIZE).take(len).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(got, vec![2, 7, 8, 9]);
}
```

```rust test
use std::cmp::Ordering;

/// First index whose entry is not Less than the target: the position an insert goes to, and where a lookup looks.
fn lower_bound(len: usize, mut cmp: impl FnMut(usize) -> Ordering) -> usize {
    let (mut lo, mut hi) = (0, len);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if cmp(mid) == Ordering::Less { lo = mid + 1 } else { hi = mid }
    }
    lo
}

#[test]
fn lower_bound_matches_partition_point() {
    let keys = [2, 4, 4, 8, 9];
    for target in 0..12 {
        let mine = lower_bound(keys.len(), |i| keys[i].cmp(&target));
        let std = keys.partition_point(|&k| k < target);          // the standard library's version of the same search
        assert_eq!(mine, std, "target {target}");
    }
    assert_eq!(lower_bound(5, |i| [2, 4, 4, 8, 9][i].cmp(&4)), 1);       // the FIRST of the equal keys
    assert_eq!(lower_bound(0, |_| unreachable!()), 0);                    // empty array: position 0, no probes
}

#[test]
fn std_binary_search_variants() {
    let v = [1, 3, 5, 7];
    assert_eq!(v.binary_search(&5), Ok(2));
    assert_eq!(v.binary_search(&4), Err(2));                      // Err carries the insertion point
    assert_eq!(v.binary_search_by_key(&7, |&x| x), Ok(3));
}
```

### In the exercises

- **2a-02:** shifting a range inside one buffer with `copy_within`, and a binary search that takes a comparison closure and returns a position (the second example is the algorithm).
- **2b-02:** a bucket page may be sorted (insert at `lower_bound`, reject an equal key) or unsorted; both are valid.

### Where it is used

- **B-tree and B+ tree nodes** keep sorted keys in a contiguous array and binary-search it (SQLite, PostgreSQL's nbtree, RocksDB's blocks).
- **LSM trees**: every SSTable block has a sorted array of keys with a restart-point index searched by binary search.
- **Any ordered lookup in a flat array**: `std`'s `binary_search`, Postgres's `bsearch`, Linux's `bsearch()`.
- **Memmove-based insertion** is how `Vec::insert` works; the cost (O(n) shift) is why a node holds hundreds of entries and not millions.
