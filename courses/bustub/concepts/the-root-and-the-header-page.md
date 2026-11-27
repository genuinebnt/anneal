---
title: The root and the header page: an entry point that moves
summary: Why the root's page id lives in a page of its own, what an empty tree is, when the root changes, and the races a latched header page prevents.
minutes: 8
---
Every operation on a tree starts at the root. In memory that is a field of the tree object. On disk the root is a page like any other, and it *moves*: when the root splits, a brand-new page becomes the root; when the root collapses, its only child does. So "where is the root?" is a piece of **shared, changing state**, and anything shared and changing needs a lock. BusTub puts it in a page of its own, the **header page**, so the lock is the same kind of page latch the rest of the tree uses.

## What is in the header page

One number: the root's page id, or `INVALID` (-1) for an empty tree. That is the whole page; the other 8,188 bytes are unused.

| state | header says | the tree is |
|---|---|---|
| fresh page from `new_page` | `0` (zeros) | **wrong**: reads as "the root is page 0" |
| after `init` | `INVALID` | empty |
| after the first insert | the new leaf's id | a single leaf |
| after the root splits | the new internal page's id | one level taller |
| after the root collapses | the old root's only child | one level shorter |
| after the last remove | `INVALID` | empty again |

> [!WARNING] The zero page trap
> A page that was never written is zeros, and 0 is a legal page id. An index that forgot to format its header will "find" a root that is some other table's page. `init` writes `INVALID` explicitly; the stage's very first test is that a zero header reads as page 0 *until* it is initialised.

## The races a latched header prevents

```svg
caption: Two threads insert into an empty tree. Without the header's write latch both see "no root" and both create one; the second overwrites the first and its key is lost. With the latch held from the check to the store, the second thread sees the first's root.
<svg viewBox="0 0 760 230" role="img" aria-label="Two timelines of two threads inserting into an empty tree, one losing a key without the header latch and one correct with it">
<text class="dim sm" x="20" y="24">without the latch</text>
<rect class="box" x="20" y="34" width="150" height="30" rx="4"/><text class="mid fg sm" x="95" y="53">A: root? none</text>
<rect class="box" x="180" y="34" width="150" height="30" rx="4"/><text class="mid fg sm" x="255" y="53">A: root = leaf A</text>
<rect class="hot" x="100" y="72" width="150" height="30" rx="4"/><text class="mid fg sm" x="175" y="91">B: root? none</text>
<rect class="hot" x="260" y="72" width="170" height="30" rx="4"/><text class="mid fg sm" x="345" y="91">B: root = leaf B</text>
<text class="t-r sm" x="450" y="60">A's key is gone: leaf A is unreachable</text>
<text class="dim sm" x="20" y="142">with the header write latch</text>
<rect class="live" x="20" y="152" width="310" height="30" rx="4"/><text class="mid fg sm" x="175" y="171">A: latch header · root? none · root = leaf A · release</text>
<rect class="live" x="340" y="152" width="300" height="30" rx="4"/><text class="mid fg sm" x="490" y="171">B: latch header · root = leaf A · insert into it</text>
<text class="t-g sm" x="450" y="206">both keys are in the tree</text>
</svg>
```

The same latch protects readers: a reader that read the root id and was then preempted must not follow a stale id into a page that has since been recycled. So readers latch the header, latch the root, and only then release the header ("crabbing" starts at the header).

## Which operations touch the header

- **Search**: read-latch the header, read the root id, latch the root, drop the header.
- **Insert / remove (pessimistic)**: *write*-latch the header, keep it in the operation's context until the root is known to be safe (or the operation is finished), because a root split or collapse writes to it.
- **Optimistic insert / remove**: read-latch it, like a search.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `class BPlusTreeHeaderPage { page_id_t root_page_id_; }` cast from the page | a view with `root_page_id()` and `set_root_page_id()` reading and writing 4 bytes at offset 0 |
| a `root_page_id_` member of the tree guarded by a mutex | the same idea, but the "mutex" is the page's own latch, so the root survives restarts |
| `INVALID_PAGE_ID = -1` | `PageId::INVALID` in the bytes; `Option<PageId>` once it is in memory |

## In real code

### Using it: an entry point behind a lock, and the empty-tree race

```rust test
use std::sync::{Arc, Mutex};
use std::thread;

/// The tree's entry point: None = empty. Behind a Mutex, like the header page behind its write latch.
struct EntryPoint(Mutex<Option<u32>>);

impl EntryPoint {
    /// Returns (the root, whether this call created it). The check and the store are one critical section.
    fn root_or_create(&self, make: impl FnOnce() -> u32) -> (u32, bool) {
        let mut root = self.0.lock().unwrap();
        match *root {
            Some(id) => (id, false),
            None => { let id = make(); *root = Some(id); (id, true) }
        }
    }
}

#[test]
fn exactly_one_thread_creates_the_root() {
    let entry = Arc::new(EntryPoint(Mutex::new(None)));
    let created = Arc::new(Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..8u32).map(|t| {
        let (entry, created) = (Arc::clone(&entry), Arc::clone(&created));
        thread::spawn(move || {
            let (root, was_created) = entry.root_or_create(|| 100 + t);
            if was_created { created.lock().unwrap().push(root); }
            root
        })
    }).collect();
    let roots: Vec<u32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(created.lock().unwrap().len(), 1, "exactly one thread created a root");
    assert!(roots.windows(2).all(|w| w[0] == w[1]), "every thread saw the same root");
}

#[test]
fn a_check_then_store_without_the_lock_held_can_lose_a_root() {
    // The broken shape, run deterministically: both threads checked before either stored.
    let root: Mutex<Option<u32>> = Mutex::new(None);
    let a_saw_none = root.lock().unwrap().is_none();      // lock released here...
    let b_saw_none = root.lock().unwrap().is_none();      // ...so B checks before A stores
    if a_saw_none { *root.lock().unwrap() = Some(1); }
    if b_saw_none { *root.lock().unwrap() = Some(2); }    // B overwrites A's root
    assert_eq!(*root.lock().unwrap(), Some(2), "A's root, and everything inserted under it, is lost");
}
```

```rust test
const INVALID: i32 = -1;

struct Header([u8; 64]);   // a tiny stand-in for the 8 KiB page

impl Header {
    fn zeroed() -> Header { Header([0; 64]) }
    fn init(&mut self) { self.set_root(INVALID); }
    fn root(&self) -> i32 { i32::from_le_bytes(self.0[0..4].try_into().unwrap()) }
    fn set_root(&mut self, id: i32) { self.0[0..4].copy_from_slice(&id.to_le_bytes()); }
    fn root_if_any(&self) -> Option<i32> { Some(self.root()).filter(|&id| id >= 0) }
}

#[test]
fn an_unformatted_header_names_page_zero_and_init_fixes_it() {
    let mut h = Header::zeroed();
    assert_eq!(h.root(), 0, "a zero page says: the root is page 0");
    h.init();
    assert_eq!(h.root_if_any(), None);
    h.set_root(17);                                       // the first leaf, then a new root after a split, then a collapse...
    assert_eq!(h.root_if_any(), Some(17));
    h.set_root(INVALID);                                  // the last key removed
    assert_eq!(h.root_if_any(), None);
}
```

### In the exercises

- **2c-01:** the header page (`root_page_id`, set, init) is formatted by `new`; a lookup read-latches the header, then the root, then drops the header; the first insert write-latches it and stores the new root.
- **2c-02, 2c-03, 2c-05, 2c-06:** a root split or a root collapse updates it.
- **2e-01:** a writer releases it as soon as it reaches a safe page.

### Where it is used

- **PostgreSQL**: the first page of a B-tree index is a *metapage* that holds the root's block number (and a "fast root"); every descent starts by reading it.
- **SQLite**: page 1 holds the database header and the root of `sqlite_schema`, which lists the root page of every table and index.
- **LMDB** and other copy-on-write trees: a commit writes new pages and then atomically switches the **meta page** to the new root; readers that started earlier keep the old root and see a consistent snapshot.
- **Lock-free Rust code**: `ArcSwap` and `RwLock<Arc<T>>` are the in-memory version of "a pointer to the current root that can be swapped".
