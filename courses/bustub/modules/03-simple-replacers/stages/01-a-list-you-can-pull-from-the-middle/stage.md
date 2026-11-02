A buffer pool must answer one question constantly: *which page do I throw out?* The simplest good answer, **least recently used**, needs a list of frames ordered by recency in which you can take any frame out of the middle and put it at the back in constant time. C++ has `std::list` plus an iterator saved in a map. Rust has no such thing in its standard library, and building a linked list with references runs straight into the borrow checker. This stage is where you meet that problem on a small data structure, before it appears in a buffer pool.

> [!CHECK] A doubly linked list has nodes that point at their neighbours, and a neighbour points back. In Rust, who owns each node? What would `Option<Box<Node>>` for `next` and `prev` mean, and why does it stop compiling? Name two ways out and what each costs you.
> ||`Box` means "I own this", so `next: Option<Box<Node>>` and `prev: Option<Box<Node>>` would have two owners for every node, which ownership forbids. Ways out: **indices into a `Vec`** (the nodes are owned by the vector; links are plain numbers, no borrow checker involved; the cost is that an index can go stale and you must detect it); **`Rc<RefCell<Node>>`** with `Weak` back links (shared ownership checked at run time; costs counts, borrow panics and reference cycles); **raw pointers and `unsafe`** (fast and exactly what `std::collections::LinkedList` does, but you carry the proof).||
>
> - Which node owns which, in a list that is only walked forwards?
> - What goes wrong when a handle outlives the element it named?
> - What does a run-time check cost, compared with a compile-time one?

## The task

`IndexList<T>` is an ordered sequence, front to back. Its contract:

- `push_back(value)` adds at the back and returns a `Handle` naming that element.
- `pop_front()` removes and returns the first element.
- `remove(handle)` removes the element a handle names and returns it. `get(handle)` looks at it. `front()` and `iter()` look from the front. `len()` and `is_empty()` count.
- `move_to_back(handle)` moves the element to the back; **its handle stays valid**. It returns whether the handle was valid.
- A handle names its element **until that element is removed. After that it never names anything**, not even a new element that took the old one's place in memory. Using such a *stale* handle returns `None` or `false` and changes nothing.
- Every operation that takes a handle is O(1).

The tests run random sequences of all these operations against a model made of a plain `Vec`, checking the whole list after every step, and a timing test removes 60 000 elements in scrambled order.

## Your freedom

How the nodes are stored and linked, what a `Handle` contains, how freed space is reused, and whether you use `unsafe`. The signatures fix what callers see; the design is yours. A generation counter in each handle is the standard trick for staleness, and not the only one.

## The Rust toolbox

**An arena of nodes.** `Vec<Node<T>>` owns every node; a node's `prev` and `next` are `Option<usize>` indices into that vector. There are no references between nodes, so the borrow checker has nothing to object to. Removing a node leaves a hole that you put on a *free list* (another `Vec<usize>`) and reuse for the next push.

**`Option::take` and `Option::as_ref`.** `node.value.take()` moves the value out of a node and leaves `None` behind; `node.value.as_ref()` gives an `Option<&T>` without moving. They are the daily tools for owned data in a slot that may be empty.

**A handle that detects staleness.** An index alone cannot tell "my element" from "whatever lives in that slot now". Add a *generation* number to the slot, bump it every time the slot is freed, and store the number you saw in the handle: a handle matches only if both agree. `u32` is plenty (and what happens at `u32::MAX`? A good thing to decide and write down).

**Returning an iterator.** `fn iter(&self) -> impl Iterator<Item = &T> + '_` hides the concrete iterator type. `std::iter::from_fn(move || { ... })` builds one from a closure that returns `Some(item)` until it returns `None`; it is the shortest way to walk a linked structure.

**Borrow errors in a linked structure.** Writing `self.nodes[a].next = Some(b)` borrows `self.nodes` mutably for that statement; you cannot hold `&mut self.nodes[a]` and `&mut self.nodes[b]` at once (E0499). Read the indices you need into local `usize` variables first, then do the writes one at a time.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves) and [L2 Borrowing](/t/l2-borrowing): why a value has one owner, and what `&mut` excludes.
- [S3 Vec & slices](/t/s3-vec-slices): indexing, `push`, `pop`, `swap`.
- [S1 Option & Result](/t/s1-option-result): `take`, `as_ref`, `?` on an `Option`.
- [F2 Data layout](/t/f2-data-layout), if you want to see what a node looks like in memory.
- S7 Smart pointers & interior mutability: Use it; Understand it: `Box`, `Rc`, choosing the cheapest correct pointer; why linked structures fight the borrow checker.
- [D5 Linked lists](/t/d5-linked-lists): Linked lists, the Rust way: index arenas, cursors, `take()`.
- [F3 Memory & allocation](/t/f3-memory-allocation): Arenas and pools: a typed-index arena with generations (stale handles).
- [L5 Generics & associated types](/t/l5-generics): Generic code: `IndexList<T>`: a generic container.
- S6 Iterators: Use; Understand: returning `impl Iterator`, `iter::from_fn`.

## Tests

- A new list is empty; pushes keep order, handles find their elements, and values need not be `Clone`.
- A removed handle never names another element, even when its space is reused.
- Moving to the back keeps the handle valid.
- Random sequences of push, pop, remove, move and get agree with a `Vec` model after every step.
- Removing 60 000 elements by handle in scrambled order finishes in time.

## Hints

### How do you find a node from a handle?

Whatever a handle holds must get you to the node without searching, and must tell you if the node is gone. What would you put in the handle for that? What changes in the node when it is freed?

### Unlinking

To take a node out of the middle, its neighbours must point at each other. Which cases need care: the node is the only one; it is the first; it is the last? Draw a three-node list and cross out the middle one. Which fields change?

### Reusing space

If every push appended to the vector, a long-running list would grow forever even when it holds ten elements. Where does a freed slot go, and what must the next push check before it grows the vector?

## Performance

Each operation touches a handful of `Vec` slots: a few nanoseconds, without any allocation once the arena has grown. Compare a design that searches for an element to remove it: with 60 000 elements it does roughly a billion steps, and the timing test fails. The arena also keeps nodes next to each other in memory, which a pointer-per-node list does not.

**Measure it.** Time 1 million `push_back` followed by 1 million `pop_front` on your list and on `VecDeque`. Predict first: which wins, and by how much? Then time `remove` of random elements by handle against `VecDeque::remove(index)` for 50 000 elements.

## Experiment

Optional. Predict first, then run.

1. **No generations.** Remove the generation counter and re-run the stale-handle test. Which assertion fails, and what would a buffer pool built on this list do in production?
2. **`Rc<RefCell<..>>`.** Build the same list from reference-counted nodes with `Weak` back links. How many lines longer is it, and which test is hardest to pass?

## Other designs

- **Arena with generations (ours).** Safe, O(1), cache-friendly; handles are plain `Copy` values.
- **`HashMap<u64, Node>` keyed by a counter.** Handles are ever-increasing numbers, so staleness is free; every access hashes.
- **`Rc<RefCell<Node>>` with `Weak` links.** No indices; pays reference counts and run-time borrow checks.
- **Raw pointers (`unsafe`), like `std::collections::LinkedList`.** The fastest and the one that needs a written safety argument; read the standard library's version when you finish.
- **Slotmap crate.** A packaged arena with generational keys; reading its source is a short lesson.

## In BusTub

BusTub's LRU-K replacer keeps `std::list<frame_id_t>` plus an `unordered_map<frame_id_t, std::list<..>::iterator>`. The C++ list's iterators stay valid when other elements are removed, which is what makes "remove this frame" O(1). Rust has no equivalent, hence this stage.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::list<T>` plus a saved `iterator` per element | `IndexList<T>` plus a `Handle` per element |
| an iterator kept valid by the list's guarantees | a handle kept honest by a generation number |
| `list.splice(list.end(), list, it)` (move to the back) | `list.move_to_back(handle)` |
| a dangling iterator is undefined behaviour | a stale handle is a `None` |

**Port rule:** a pointer or iterator into a container becomes an index plus whatever lets you check it is still current.

## Learn more

- [`Vec`](https://doc.rust-lang.org/std/vec/struct.Vec.html) · [`Option::take`](https://doc.rust-lang.org/std/option/enum.Option.html#method.take) · [`iter::from_fn`](https://doc.rust-lang.org/std/iter/fn.from_fn.html)
- *Learn Rust With Entirely Too Many Linked Lists*, <https://rust-unofficial.github.io/too-many-lists/>
- [Slotmap](https://docs.rs/slotmap) and [`std::collections::LinkedList`'s source](https://github.com/rust-lang/rust/blob/master/library/alloc/src/collections/linked_list.rs)
