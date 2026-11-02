Rust's checker rejects data that points at itself, and databases are full of it: a node refers to its parent, a page to its frame, a frame to a page. Two shapes cover almost all of it: a structure that **owns** its parts, moving values out of places with `take` and `replace`, and a structure that refers to parts **by handle**, an index into an arena. The buffer pool and the replacers of project 1 use the second all the time. This stage builds one of each in `src/rust_primer/shapes.rs`.

## The task

- `Stack<T>`: a stack as a linked list where each node owns the next: `push`, `pop`, `peek`, `peek_mut`, `len`, `reverse` (in place, in `O(n)`, no clones, no allocation) and `to_vec`. **Dropping a long stack must not overflow the call stack**: the default drop of a chain of `Box`es is recursive, so write `Drop` as a loop.
- `Slab<T>`: an arena. `insert` returns a `Handle` (an index and a **generation**), `get` / `get_mut` / `remove` take one. A removed slot is reused by a later `insert`, and a handle to the removed value must **never** find the value that reuses its slot (that is what the generation is for).

The tests: stack order, `peek_mut`, `reverse`, that values are moved not cloned and dropped exactly once (counting `Rc`s), a stack of 200 000 nodes dropped on a small stack (in a child process, since an overflow kills the process), slab handles, reuse of a slot with a dead handle; and two properties: **a stack is a `Vec` that changes at one end**, and **a slab is a map from handles to values, and dead handles stay dead**.

## Your freedom

Everything private is yours: the node and slot types, whether the free slots are a `Vec` of indexes or a list threaded through the slots, the starting generation. The public methods and `Handle` are the contract.

## The Rust toolbox

**Moving out of a place.** You cannot move out of `&mut self.head`, but you can swap something in: `self.head.take()` leaves `None` and gives you the old `Option<Box<Node>>`; `mem::replace(&mut x, new)` gives the old value and puts the new one; `mem::take(&mut x)` replaces with `Default`.

```rust
pub fn push(&mut self, value: T) {
    let next = self.head.take();                       // the new node owns the old list
    self.head = Some(Box::new(Node { value, next }));
}
```

**`as_ref`, `as_mut`, `as_deref`.** `Option<Box<Node>>` to `Option<&Node>`, to `Option<&mut Node>`, and `as_deref()` straight to `Option<&Node>` in a loop that follows links.

**Drop as a loop.** Dropping the head drops its `next`, which drops its `next`, ... one call frame per node. In `Drop::drop`, take each node's `next` before the node is dropped:

```rust
let mut cur = self.head.take();
while let Some(mut node) = cur { cur = node.next.take(); }   // node is dropped here with no next
```

**Handles.** `Handle { index, generation }` is `Copy`: it borrows nothing, so it can sit in any struct. `slots.get(h.index)?` then compare generations. When a slot is freed, add one to its generation.

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves): moving out of a place; `take`, `replace`.
- S7 Smart pointers & interior mutability: `Box`, `Drop`.
- [S3 Vec & slices](/t/s3-vec-slices): indexing, `get`, `swap_remove`.

## Tests

- Stack order; `peek` and `peek_mut`; `reverse`; `Rc` counts through push, reverse, pop and drop.
- 200 000 nodes dropped on a 256 KiB stack.
- Slab: insert/get/get_mut/remove; remove twice; a dead handle after the slot is reused.
- Properties: a stack against a `Vec`; a slab against live and dead handles.

## Hints

### Why `reverse` needs `take`

You cannot move `node.next` out of a node you only borrow. `mem::replace(&mut node.next, reversed)` puts the new tail in and gives you the old one.

### The drop test runs in a child process

A stack overflow aborts the whole program, so the test starts itself again. If it fails, the message says the program crashed: that is a recursive drop.

### The generation is the whole trick

Without it, `get` on an old handle finds whatever lives in the slot now. Compare generations on every access; bump the generation on `remove`, not on `insert`.

## Performance

`push` and `pop` are `O(1)` but allocate a node each (a `Vec` would amortise that). The slab allocates nothing per `insert` once it has free slots, and a lookup is one indexed read and one comparison.

**Measure it.** Push and pop 1 000 000 numbers on `Stack` and on `Vec`: the list is several times slower, and not for the reason you might expect: allocation and cache misses.

## Experiment

Optional. Predict first, then run.

1. **Skip the custom `Drop`** and run the long-stack test. What does the failure say?
2. **Never bump the generation.** Which test fails, and which line of that test is the bug a real program would have?

## Other designs

- **`Option<Rc<RefCell<Node>>>`:** nodes shared and mutable, at the price of runtime borrow checks and reference-count traffic.
- **`Vec` as a stack** is simply better here; the list is for learning the shape, because trees and graphs need it.
- **A slab without generations** (`slab` crate): indexes are reused at once; a stale index is a bug you have to avoid yourself.
- **A raw-pointer list with `unsafe`:** what the standard library's `LinkedList` does; not needed here.

## In BusTub

BusTub's buffer pool and replacers hold `frame_id_t` and `page_id_t` integers, never pointers to each other: an index into an array of frames is a handle. The replacers of project 1 use exactly the index-instead-of-pointer and `Option::take` moves practised here.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `node->next = head; head = node;` | `let next = self.head.take(); self.head = Some(Box::new(Node { value, next }));` |
| `delete node;` after saving `next` | `cur = node.next.take();` (the node drops itself) |
| `std::list<T>::iterator` kept after `erase` (undefined) | a `Handle` whose generation no longer matches: `None` |
| `T *` into a vector that may reallocate | an index, or a `Handle`, that stays valid |

**Port rule:** replace a pointer to a neighbour with an index into the structure that owns both; replace `delete` with `take`.

## Learn more

- [Learn Rust With Entirely Too Many Linked Lists](https://rust-unofficial.github.io/too-many-lists/) · [`Option::take`](https://doc.rust-lang.org/std/option/enum.Option.html#method.take) · [`mem::replace`](https://doc.rust-lang.org/std/mem/fn.replace.html)
