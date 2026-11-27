---
title: Anatomy of a B+ tree: pages, fan-out and height
summary: The three kinds of page, what lives in each, why a node holds hundreds of keys, the numbers (fan-out, height, minimum fill) and the rules that every operation must leave true.
minutes: 11
---
A hash table answers "is this key here?" in one or two page reads and knows nothing about order. A **B+ tree** answers the same question in three or four page reads and keeps every key in sorted order, so it can also answer "all keys from 40 to 90", "the smallest key above 100" and "the keys in order". It is the default index of nearly every database, and every page of it is a buffer-pool page, so everything you built in modules 1a to 2b is underneath it.

## Why a tree of wide nodes

A binary search tree over a billion keys is 30 levels deep, and each level is a pointer to somewhere else in memory. On disk each pointer is a page read, so 30 reads per lookup. A disk page is 8 KiB and costs the same to read whether you use 16 bytes of it or all of it, so use all of it: make each node a page holding as many keys as fit. With 8-byte keys and 4-byte child ids a page holds 681 children, and the number of levels needed for a billion keys falls from 30 to 4.

| keys | binary tree levels | B+ tree levels (fan-out 340, a typical half-full page) |
|---|---|---|
| 1,000 | 10 | 2 |
| 1,000,000 | 20 | 3 |
| 1,000,000,000 | 30 | 4 |

And the top levels are tiny (1 root, a few hundred pages below it), so the buffer pool keeps them in memory: a lookup in a billion-key index typically costs **one or two disk reads**.

## Three kinds of page

```svg
caption: A B+ tree with leaf pages of up to 2 pairs and internal pages of up to 3 children. Internal pages hold separator keys and child page ids (they only direct the search). Leaves hold the pairs and point at the next leaf (dashed). The header page holds the root's id.
<svg viewBox="0 0 760 290" role="img" aria-label="A three-level B+ tree: a header page naming the root, an internal root, two internal children and five chained leaves">
<defs><marker id="bt-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="never" x="20" y="14" width="120" height="40" rx="4"/><text class="mid dim" x="80" y="31">header page</text><text class="mid dim sm" x="80" y="47">root = page 7</text>
<path class="ln dash" d="M140 34 H318" marker-end="url(#bt-a)"/>
<rect class="blue" x="320" y="14" width="120" height="40" rx="4"/><text class="mid t-b" x="380" y="31">internal 7</text><text class="mid dim sm" x="380" y="47">keys: 5</text>
<path class="ln" d="M350 54 L200 96" marker-end="url(#bt-a)"/><path class="ln" d="M410 54 L560 96" marker-end="url(#bt-a)"/>
<rect class="blue" x="130" y="98" width="140" height="40" rx="4"/><text class="mid t-b" x="200" y="115">internal 3</text><text class="mid dim sm" x="200" y="131">keys: 3</text>
<rect class="blue" x="490" y="98" width="140" height="40" rx="4"/><text class="mid t-b" x="560" y="115">internal 4</text><text class="mid dim sm" x="560" y="131">keys: 7, 9</text>
<path class="ln" d="M160 138 L110 188" marker-end="url(#bt-a)"/><path class="ln" d="M240 138 L260 188" marker-end="url(#bt-a)"/>
<path class="ln" d="M520 138 L450 188" marker-end="url(#bt-a)"/><path class="ln" d="M560 138 L560 188" marker-end="url(#bt-a)"/><path class="ln" d="M600 138 L670 188" marker-end="url(#bt-a)"/>
<g>
<rect class="live" x="50" y="190" width="110" height="44" rx="4"/><text class="mid fg" x="105" y="217">1  2</text>
<rect class="live" x="200" y="190" width="110" height="44" rx="4"/><text class="mid fg" x="255" y="217">3  4</text>
<rect class="live" x="390" y="190" width="110" height="44" rx="4"/><text class="mid fg" x="445" y="217">5  6</text>
<rect class="live" x="520" y="190" width="80" height="44" rx="4"/><text class="mid fg" x="560" y="217">7  8</text>
<rect class="live" x="620" y="190" width="90" height="44" rx="4"/><text class="mid fg" x="665" y="217">9  10</text>
</g>
<path class="ln-g dash" d="M160 212 H198" marker-end="url(#bt-a)"/><path class="ln-g dash" d="M310 212 H388" marker-end="url(#bt-a)"/><path class="ln-g dash" d="M500 212 H518" marker-end="url(#bt-a)"/><path class="ln-g dash" d="M600 212 H618" marker-end="url(#bt-a)"/>
<text class="dim sm" x="50" y="262">leaves: sorted pairs, chained left to right</text>
<text class="t-g sm" x="400" y="262">every leaf at the same depth</text>
</svg>
```

| page | holds | what it is for | BusTub name |
|---|---|---|---|
| **header** | the root's page id | the tree's entry point; changes when the tree grows or shrinks, so it needs a latch of its own | `BPlusTreeHeaderPage` |
| **internal** | `n` child page ids and `n - 1` separator keys | direct a search: child `i` covers keys `key(i) <= K < key(i+1)` | `BPlusTreeInternalPage` |
| **leaf** | sorted `(key, value)` pairs and the next leaf's id | the data; chained so a scan never climbs back up | `BPlusTreeLeafPage` |

The difference between a **B tree** and a **B+ tree** is that a B tree stores values in internal nodes as well; a B+ tree stores them *only* in the leaves. That lets internal pages hold more keys (they carry no values), makes every lookup the same length, and lets the leaf chain serve range scans.

## The numbers

A page's layout is a header followed by an array of fixed-size entries, and the capacity is arithmetic:

```text
leaf      header 16: type 4, size 4, max_size 4, next_page_id 4
          then n * (key 8 + rid 8)     n = (8192 - 16) / 16 = 511
internal  header 12: type 4, size 4, max_size 4
          then n * (key 8 + id 4)      n = (8192 - 12) / 12 = 681
```

Because the header is 12 or 16 bytes, "slots that fit" is `(page_size - header) / entry_size`, and that division is the whole reason a different key type changes the fan-out. The tree can also be told to use *smaller* nodes than fit (`leaf_max_size`, `internal_max_size`), which is how the tests force a deep tree out of a few keys.

## Sizes and the rules that always hold

Let `max` be a node's maximum. In this course (and BusTub):

- a **leaf** splits when an insert brings it to `max` pairs, so at rest it holds `1..max - 1` pairs (the root) or `max/2..max - 1` (any other leaf);
- an **internal** page splits when it would need `max + 1` children, so it holds `ceil(max/2)..max` children (the root: `2..max`);
- **all leaves are at the same depth**, because the tree only grows by splitting the root;
- keys inside every page are strictly increasing, and every key in the subtree of child `i` is `>= key(i)` and `< key(i + 1)`;
- the chain of next-leaf ids visits the leaves left to right, once each.

| operation | touches | changes the height? |
|---|---|---|
| search | one page per level | no |
| insert, leaf has room | one leaf | no |
| insert, leaf full | the leaf, a new leaf, the parent (maybe more) | only if the root splits |
| remove, leaf stays half full | one leaf | no |
| remove, leaf too empty | the leaf, a sibling, the parent (maybe more) | only if the root loses its second child |

> [!WHY] Why the minimum is what it is
> A split of a full page leaves two halves; each must still be at least the minimum, or the page would be "too empty" the moment it exists. And a merge of a too-empty page (`min - 1` entries) with a sibling that has exactly `min` must fit in one page. Those two requirements together force `min = floor(max/2)` for leaves (which hold at most `max - 1`) and `ceil(max/2)` for internal pages. The module's first stage asks you to check it for small `max`.

## B+ trees and C/C++

| C / C++ | Rust |
|---|---|
| `class BPlusTreeInternalPage : public BPlusTreePage` with flexible-array members, cast from a `char *` page | a struct holding the page's bytes (`&[u8]` or `&mut [u8]`) with methods that read and write fields at offsets |
| `page->IsLeafPage()` then `reinterpret_cast<LeafPage *>` | `Page::new(bytes).is_leaf_page()` then `Leaf::new(bytes)`: the same bytes, a different view |
| `INTERNAL_PAGE_SLOT_CNT` macro | an associated `const fn`/function computed from `FixedSize::SIZE` |

## In real code

### Using it: the geometry of a tree, and the split arithmetic, as tested code

Two small tools that answer the questions you will keep asking while building the tree: *how many entries fit*, *how tall will it be*, and *do the minimum sizes work out*.

```rust test
/// How many `(key, value)` entries fit in a page after a header.
fn capacity(page: usize, header: usize, entry: usize) -> usize { (page - header) / entry }

/// The number of levels needed for `n` pairs when leaves hold `leaf` pairs and internal pages `fan` children, all full (the best case).
fn levels(n: u64, leaf: u64, fan: u64) -> u32 {
    let mut pages = n.div_ceil(leaf);          // leaf pages
    let mut levels = 1;
    while pages > 1 {                          // each level up has `fan` times fewer pages
        pages = pages.div_ceil(fan);
        levels += 1;
    }
    levels
}

#[test]
fn the_numbers_of_the_course_pages() {
    let (leaf, internal) = (capacity(8192, 16, 8 + 8), capacity(8192, 12, 8 + 4));
    assert_eq!((leaf, internal), (511, 681));
    assert_eq!(capacity(8192, 16, 4 + 8), 681, "a leaf of 4-byte keys and 8-byte rids holds 681 pairs");
    assert_eq!(levels(1, 511, 681), 1, "one pair: a lone root leaf");
    assert_eq!(levels(511, 511, 681), 1);
    assert_eq!(levels(512, 511, 681), 2, "the first split makes a second level");
    assert_eq!(levels(511 * 681, 511, 681), 2);
    assert_eq!(levels(511 * 681 + 1, 511, 681), 3);
    assert_eq!(levels(511 * 681 * 681, 511, 681), 3, "three levels address up to 237 million pairs in full pages");
    assert_eq!(levels(511 * 681 * 681 + 1, 511, 681), 4);
    assert_eq!(levels(1_000_000_000, 511, 681), 4, "a billion pairs: 4 levels");
    assert_eq!(levels(1_000_000_000, 255, 340), 4, "also in half-full pages (the typical case)");
}

/// The sizes of the two halves when a node holding `n` entries splits (the left half keeps the extra one).
fn split(n: usize) -> (usize, usize) { (n.div_ceil(2), n / 2) }

#[test]
fn the_minimum_sizes_make_every_split_and_merge_work() {
    for max in 2..=64usize {
        let leaf_min = max / 2;
        let internal_min = max.div_ceil(2);
        // a leaf splits when it reaches `max` pairs
        let (a, b) = split(max);
        assert!(a >= leaf_min && b >= leaf_min, "leaf max {max}: halves {a} and {b} must reach min {leaf_min}");
        // an internal page splits when it would have `max + 1` children
        let (a, b) = split(max + 1);
        assert!(a >= internal_min && b >= internal_min, "internal max {max}: halves {a} and {b} must reach min {internal_min}");
        // a page one below the minimum merged with a sibling at the minimum fits in one page
        assert!(leaf_min - 1 + leaf_min < max, "leaf max {max}: the merged leaf must stay below max");
        assert!(internal_min - 1 + internal_min <= max, "internal max {max}: the merged page must fit");
    }
}

#[test]
fn the_layout_is_header_then_entries() {
    // a leaf with two entries, written by hand: header fields little-endian, then the pairs
    let mut page = [0u8; 8192];
    page[0..4].copy_from_slice(&1u32.to_le_bytes());      // page_type: leaf
    page[4..8].copy_from_slice(&2u32.to_le_bytes());      // size
    page[8..12].copy_from_slice(&4u32.to_le_bytes());     // max_size
    page[12..16].copy_from_slice(&(-1i32).to_le_bytes()); // next_page_id: none
    for (i, key) in [10i64, 20].into_iter().enumerate() {
        let at = 16 + i * 16;
        page[at..at + 8].copy_from_slice(&key.to_le_bytes());
        page[at + 8..at + 16].copy_from_slice(&(key * 100).to_le_bytes());   // a stand-in for the rid
    }
    let size = u32::from_le_bytes(page[4..8].try_into().unwrap()) as usize;
    let keys: Vec<i64> = (0..size).map(|i| i64::from_le_bytes(page[16 + i * 16..24 + i * 16].try_into().unwrap())).collect();
    assert_eq!(keys, vec![10, 20]);
    assert_eq!(i32::from_le_bytes(page[12..16].try_into().unwrap()), -1);
}
```

### In the exercises

- **2c-01 to 2c-03:** the pages you design (the leaf and the header in 2c-01, the internal page in 2c-02: capacities, the page header) and the split rule (the left page keeps `ceil`); **2c-05 and 2c-06** add the minimum sizes. The tests check the rules from outside with observers (`depth`, `leaf_sizes`) and a shape checker, not by reading your pages.
- **Everywhere:** `levels` is how you know what height a test tree should have.

### Where it is used

- **PostgreSQL**: every default index is a B+ tree with 8 KiB pages (`nbtree`); its leaf pages are chained left and right, and its first page is a *metapage* holding the root's block number, like the header page here.
- **SQLite** tables and indexes are B+ trees (4,096-byte pages by default); **MySQL InnoDB** stores the table itself as a B+ tree (a clustered index) in 16 KiB pages.
- **File systems**: Btrfs, XFS and NTFS keep directories and extents in B+ trees; **LMDB** and **redb** are key-value stores built on a (copy-on-write) B+ tree.
- **Rust's standard library** has `BTreeMap`/`BTreeSet`: an in-memory B-tree whose nodes hold up to 11 keys, chosen to fit in cache lines the way these nodes are chosen to fit disk pages.
