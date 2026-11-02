The B+ tree is the index every database leans on. Its nodes are **pages in the buffer pool**: **internal pages** hold separator keys and the ids of the pages below, **leaf pages** hold the `(key, value)` pairs and a link to the next leaf, and a **header page** says where the root is, so the root can change when the tree grows or shrinks. This is the module's design stage, as hash-table pages were in 2b: you decide what the three page types store and how, and the tests see only the tree's behaviour through a few read-only observers. In this stage the tree is a single root leaf: everything fits and nothing splits.

> [!CHECK] The tree is an empty tree and has a header page. A caller inserts a key. Before the key can be stored, which pages must exist, who creates each, and what is written in the header page? What does `get_root_page_id` return before and after?
> ||Before: only the header page exists, and it records "no root" (`PageId::INVALID`), so `is_empty` is true and `get_root_page_id` returns INVALID. The first insert allocates a leaf page, formats it as an empty leaf, stores the pair in it, and writes the leaf's page id into the header page as the root. Afterwards the root id is valid and `depth` is 1. The header is the only page whose id the tree is given; every other page is found by following ids written in pages, so the tree survives its buffer pool frames being reused.||
>
> - Why does the tree not simply remember the root's page id in a Rust field?
> - Who allocates the first leaf, and when?
> - What does a header page need to contain?

## The task

`BPlusTree<'a, K, V, C, const TOMBS: usize = 0>` is generic over key, value and comparator; `TOMBS` is the size of each leaf's tombstone buffer and is 0 until module 2d. `new(index_name, header_page_id, bpm, cmp, leaf_max_size, internal_max_size)` creates an **empty** tree whose header page is the page the caller allocated: it must **format** that page (a recycled page may be full of garbage). In this stage:

- `is_empty()` and `get_root_page_id()` (`PageId::INVALID` for an empty tree).
- `insert(&key, &value) -> bool`: false if the key is already there (the first value stays). An empty tree gets a root leaf. For now the leaf always has room.
- `get_value(&key) -> Vec<V>`: empty or one element; keys are unique.
- Sizes: `leaf_max_size` pairs per leaf (at least 2), `internal_max_size` children per internal page (at least 3); sizes that cannot work, or that do not fit in a page, are a bug in the caller: `new` **panics**. `default_leaf_max_size()` and `default_internal_max_size()` are as many as fit in one of your pages (hundreds).
- Three **observers**, read-only and for the tests: `depth()` (0 for an empty tree, 1 for a lone leaf, one more per level) and `leaf_sizes()` (how many pairs each leaf holds, left to right). `leaf_keys()` and `leaf_tombstones()` come with module 2d.
- Read every page through `self.bpm`, the traced pool given in the struct: it counts how many pages the tree latched, and the tests compare those counts with the depth.

The tests check, on random sequences with the default sizes (so nothing splits), that the tree behaves like a map, that the depth is 1 and the single leaf holds every key, that duplicates are refused, that a tree on a dirty header page starts empty, that bad sizes panic, and that no page stays pinned.

## Your freedom

The layout of all three page types and of the page-type tag they share; whether keys are sorted in a leaf (they should be: the next stages binary search them); whether an internal page keeps its first key; what the tree struct holds. Add observers to your own pages freely. Constraints: pages are 8 KiB, and `K` and `V` are `FixedSize` (module 2a).

## The Rust toolbox

**Views over page bytes.** A small struct generic over the bytes (`B: AsRef<[u8]>` to read, `B: AsRef<[u8]> + AsMut<[u8]>` to write) with accessor methods, as in 2b. `BPlusTreeLeafPage::new(&guard[..])` borrows a page for reading; `new(&mut guard[..])` for writing.

**A page-type tag.** Every page starts with a small tag (leaf or internal, say as a `u32`) so that code that has only a page id can tell which view to build. An `enum` with `#[repr(u32)]` and a `TryFrom<u32>` keeps the tag honest: an unknown number is a corrupt page, and a `match` on the enum must handle every kind.

**Const generics.** `const TOMBS: usize = 0` on the tree and on your leaf type: the number of tombstone slots changes where the pairs start in a leaf, so it is a *type* parameter, not a field. Write `Leaf::<_, K, V, TOMBS>::new(..)` everywhere; `TOMBS = 0` costs no space.

**The traced pool.** `self.bpm.read_page(id)` and `write_page(id)` have the same signatures as the plain pool's and add one to a counter each; `self.bpm.inner()` is the plain pool for calls you do not want counted.

**Guards as scopes.** Take the header's read guard, read the root id, take the root's guard, **then** drop the header guard: holding the child before releasing the parent is the rule every descent follows.

**Panicking on bad parameters.** `assert!(leaf_max_size >= 2 && leaf_max_size as usize <= capacity, "...")` with a message naming the number.

## If this is new

- [L5 Generics & associated types](/t/l5-generics), including const generics, and [L4 Traits & dispatch](/t/l4-traits-dispatch): bounds on an `impl` block.
- [L3 Lifetimes](/t/l3-lifetimes): a view that borrows a guard.
- [S3 Vec & slices](/t/s3-vec-slices): byte ranges.
- Module 2a (`FixedSize`, `PageArray`, `lower_bound`) and 1g (guards) are what you build on.
- L7 Enums & pattern matching: Enums & exhaustiveness: a page-type tag as an enum, `TryFrom<u32>`, `match` that must cover every kind.
- S7 Smart pointers & interior mutability: Understand it: page ids instead of pointers; why a tree of references fights the borrow checker.
- [F2 Data layout](/t/f2-data-layout): Locality: a slotted page for a B-tree node: header, slot array, cell heap.

## Tests

- A new tree is empty: no root, depth 0, no leaves, no value for any key.
- The first key makes a root leaf; a duplicate is refused and keeps its first value.
- A tree created on a header page full of `0xFF` is empty; two trees on different header pages are independent.
- Sizes that cannot work panic; the default sizes are what a page holds.
- For random inserts and lookups with default sizes the tree equals a `BTreeMap`; the depth is 1, the leaf holds every key, no page is left pinned.

## Hints

### What does each page type need?

Write the three pages as tables of bytes. Header: where is the root id? Leaf: how many pairs, how many fit, the next leaf's id, then the pairs. Internal: how many children, the separator keys, the child ids. Decide whether an internal page with `n` children stores `n` keys (the first unused) or `n - 1`.

### Who writes the header?

The header page is shared by every operation, so it is the page whose latch decides whether the root may change. In this stage only `insert` into an empty tree writes it; keep that in mind for module 2c-05, where the header becomes the first latch of every write.

### A page that was never written

A page from a fresh pool is all zeros, and zero is a valid page id (page 0). If your "no root" marker is 0 you will confuse the two; use `PageId::INVALID` (-1) and write it explicitly when you format the header.

## Performance

A lookup in a one-leaf tree latches the header and the leaf: two pages, one binary search over up to hundreds of pairs. With `leaf_max_size` = 511 a leaf holds 511 pairs, so a tree of height 3 holds `511 * 680 * 680 / 4` or about 60 million pairs in the typical half-full case: why B+ trees are shallow.

**Measure it.** Insert 400 sorted keys into a default-size tree and time a million random lookups in release mode; count `bpm.get_reads()` per lookup (it should be 2).

## Experiment

Optional. Predict first, then run.

1. **Zero for none.** Mark an empty root with page id 0 and create the tree on a recycled page. What breaks first?
2. **A wide leaf.** Change `leaf_max_size` to 2 and insert three keys with the code you have. What does your leaf do when it overflows (nothing is split yet)? That is what the next stage fixes.

## Other designs

- **Header page + three layers (ours).** The root can move; every page is found by id.
- **Root page id fixed.** The tree always keeps its root in the same page and copies the old root's contents into a new page when it splits: no header page needed, one more copy per root split.
- **Slotted leaves.** Variable-size keys and values with a slot directory (module 3b); more flexible, more bookkeeping.
- **Prefix compression** in internal pages: smaller separators, deeper trees' worth of fan-out.

## In BusTub

BusTub gives you `BPlusTreePage`, `BPlusTreeHeaderPage`, `BPlusTreeInternalPage` and `BPlusTreeLeafPage` as headers and asks you to fill in the members and the tree. Its `BPlusTree::GetRootPageId`, `IsEmpty`, `GetValue` and `Insert` are the surface used here.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `template <typename KeyType, typename ValueType, typename KeyComparator, size_t NumTombs = 0> class BPlusTree` | `struct BPlusTree<'a, K, V, C, const TOMBS: usize = 0>` |
| `reinterpret_cast<LeafPage *>(guard.GetDataMut())` | `Leaf::<_, K, V, TOMBS>::new(&mut guard[..])` |
| `INVALID_PAGE_ID` for "no root" | `PageId::INVALID`, `is_valid()` |
| `BUSTUB_ASSERT` on bad max sizes | `assert!` with a message |

**Port rule:** a C++ class overlaid on page bytes becomes a view struct over a slice; a template parameter that changes a layout becomes a const generic.

## Learn more

- [Const generics](https://doc.rust-lang.org/reference/items/generics.html#const-generics) · [`#[repr(u32)]` enums](https://doc.rust-lang.org/reference/type-layout.html#primitive-representations)
- Bayer and McCreight, *Organization and maintenance of large ordered indices*, 1970 · CMU 15-445 lecture on tree indexes
