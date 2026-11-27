The B+ tree is the index every database leans on. Its nodes are **pages in the buffer pool**: **leaf pages** hold the `(key, value)` pairs in key order, **internal pages** hold separator keys and the ids of the pages below, and a **header page** says where the root is, so that the root can move when the tree grows or shrinks. This module builds the tree one level at a time. In this stage the whole tree is a header page and a single leaf, the root: you design those two page types and the tree's first operations, and nothing splits yet. The internal page comes in the next stage, when a leaf first overflows and needs a parent.

> [!CHECK] The tree is an empty tree and has a header page. A caller inserts a key. Before the key can be stored, which pages must exist, who creates each, and what is written in the header page? What does `get_root_page_id` return before and after?
> ||Before: only the header page exists, and it records "no root" (`PageId::INVALID`), so `is_empty` is true and `get_root_page_id` returns INVALID. The first insert allocates a leaf page, formats it as an empty leaf, stores the pair in it, and writes the leaf's page id into the header page as the root. Afterwards the root id is valid and `depth` is 1. The header is the only page whose id the tree is given; every other page is found by following ids written in pages, so the tree survives its buffer pool frames being reused.||
>
> - Why does the tree not simply remember the root's page id in a Rust field?
> - Who allocates the first leaf, and when?
> - What does a header page need to contain?

## The task

`BPlusTree<'a, K, V, C, const TOMBS: usize = 0>` is generic over key, value and comparator; `TOMBS` is the size of each leaf's tombstone buffer and stays 0 until module 2d. `new(index_name, header_page_id, bpm, cmp, leaf_max_size, internal_max_size)` creates an **empty** tree on the header page the caller allocated, and must **format** that page: a recycled page may be full of garbage.

- `is_empty()` and `get_root_page_id()`: `PageId::INVALID` for an empty tree.
- `insert(&key, &value) -> bool`: false if the key is already there, and the first value stays. An empty tree gets a root leaf. In this stage the leaf always has room.
- `get_value(&key) -> Vec<V>`: empty or one element; keys are unique.
- Sizes: at most `leaf_max_size` pairs per leaf (at least 2), at most `internal_max_size` children per internal page (at least 3). Sizes that cannot work, or pairs that do not fit in a page, are a bug in the caller: `new` **panics**. `default_leaf_max_size()` is as many pairs as fit in one of your leaf pages (hundreds). The internal size is only remembered for now.
- Two **observers**, read-only and there for the tests: `depth()` (0 for an empty tree, 1 for a lone leaf) and `leaf_sizes()` (how many pairs each leaf holds, left to right).
- Read every page through `self.bpm`, the traced pool in the struct: it counts how many pages the tree latched, and later tests compare those counts with the depth.

## Your freedom

The layout of the leaf page and of the header page, and whether the two share a common header (a kind tag, a size). Keep a leaf's keys sorted: every later stage binary searches them. What the tree struct holds is yours. Constraints: pages are 8 KiB, and `K` and `V` are `FixedSize` (module 2a), so a pair has a fixed width.

## The Rust toolbox

**Views over page bytes.** A small struct generic over the bytes (`B: AsRef<[u8]>` to read, `B: AsRef<[u8]> + AsMut<[u8]>` to write) with accessor methods, as in 2b. `new(&guard[..])` borrows a page for reading; `new(&mut guard[..])` for writing. The view owns nothing, so it costs nothing to build one per call.

**A page-kind tag.** Write a small tag at the start of every tree page (leaf or internal, as a `u32`). There is only one kind of tree page today, but from the next stage code that has nothing but a page id has to know which view to build. An `enum` with `#[repr(u32)]` keeps the numbers stable on disk.

**Const generics.** `const TOMBS: usize = 0` on the tree and on your leaf type: the size of the tombstone buffer changes where the pairs start in a leaf, so it is a *type* parameter, not a field. Write `Leaf::<_, K, V, TOMBS>::new(..)` everywhere; with `TOMBS = 0` it costs no space.

**The traced pool.** `self.bpm.read_page(id)` and `write_page(id)` have the plain pool's signatures and add one to a counter each; `self.bpm.inner()` is the plain pool for calls you do not want counted.

**Guards as scopes.** Take the header's read guard, read the root id, take the root's guard, **then** drop the header guard: holding the child before releasing the parent is the rule every descent in this module follows.

**Panicking on bad parameters.** `assert!(leaf_max_size >= 2 && leaf_max_size as usize <= capacity, "leaf_max_size {leaf_max_size} ...")`, with a message that names the number.

## If this is new

- [L5 Generics & associated types](/t/l5-generics), including const generics, and [L4 Traits & dispatch](/t/l4-traits-dispatch): bounds on an `impl` block.
- [L3 Lifetimes](/t/l3-lifetimes): a view that borrows a guard.
- [S3 Vec & slices](/t/s3-vec-slices): byte ranges.
- Module 2a (`FixedSize`, `PageArray`, `lower_bound`) and 1g (guards) are what you build on.
- L7 Enums & pattern matching: a page-kind tag as an enum, a `match` that must cover every kind.
- [F2 Data layout](/t/f2-data-layout): what a page header is for.

## Tests

- A new tree is empty: no root, depth 0, no leaves, no value for any key. The first key makes a root leaf; a duplicate is refused and keeps its value.
- A tree created on a header page full of `0xFF` is empty; two trees on different header pages are independent.
- Sizes that cannot work panic; the default leaf size is exactly what a page holds.
- With the default leaf size, random inserts and lookups agree with a `BTreeMap`: depth 1, one leaf holding every key, no page left pinned.

## Hints

### Write the pages as tables of bytes

Header: where is the root id? Leaf: the kind tag, how many pairs, the most it may hold, then the pairs. Leave room in the leaf's header for what a later stage will want (a link to the next leaf), or plan to move the pairs when you add it.

### Who writes the header?

Every operation reads the header page first, so its latch decides whether the root may change. In this stage only `insert` into an empty tree writes it. Keep that in mind: in module 2e the header becomes the first latch every writer takes, and the first one it lets go.

### A page that was never written

A page from a fresh pool is all zeros, and zero is a valid page id (page 0). If your "no root" marker is 0 you will confuse the two; use `PageId::INVALID` and write it explicitly when you format the header.

## Performance

A lookup in a one-leaf tree latches the header and the leaf: two pages and one binary search over up to hundreds of pairs. A leaf of 8-byte keys and 8-byte record ids holds about 500 pairs; with internal pages of about 680 children a tree of height 3 typically holds tens of millions of keys, which is why B+ trees stay shallow.

**Measure it.** Insert 400 keys into a default-size tree and time a million random lookups in release mode; check that `bpm.get_reads()` grows by 2 per lookup.

## Experiment

Optional. Write your prediction down first, then run.

1. **Zero for none.** Mark an empty root with page id 0 and create the tree on a recycled page. What breaks first, and which test says so?
2. **The full leaf.** Create a tree with `leaf_max_size = 4` and insert four keys with the code you have. What does your leaf do? That is what the next stage fixes.

## Other designs

- **A header page that names the root (ours, BusTub's).** The root can move; every page is found by id.
- **The root at a fixed page.** When the root splits, its contents move to a new page and the root page is rewritten in place: no header page, one more copy per root split (SQLite keeps a table's root page fixed this way).
- **Slotted leaves.** Variable-size keys and values with a slot directory (module 3b): more flexible, more bookkeeping.

## In BusTub

BusTub gives `BPlusTreePage`, `BPlusTreeHeaderPage` and `BPlusTreeLeafPage` as headers and asks for the members and the tree. Its `BPlusTree::GetRootPageId`, `IsEmpty`, `GetValue` and `Insert` are the surface used here.

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
- Bayer and McCreight, *Organization and maintenance of large ordered indexes*, 1972 · CMU 15-445 lecture on tree indexes
