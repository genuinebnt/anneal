A persistent trie never changes: `put` and `remove` return **a new trie** that shares every node it did not have to touch with the old one. The old trie stays valid and costs almost nothing to keep: that is what lets readers and a writer work at the same time in stage 3. `put` creates new nodes only along the path from the root to the key. `remove` does the same and has a second job: a node left with no value and no children is useless and is **pruned**, and so is its parent if that now leads nowhere either. If the whole trie becomes empty it has **no root**.

> [!CHECK] A trie holds the keys "ab" and "ac". You `put` "ad". Which nodes of the new trie are new, which are shared with the old one, and what does the old trie still contain? Then `remove` "ab" from the new trie: which nodes are new, and what happens to the node for "b"? What if you remove "ad" and "ac" as well?
> ||After `put`: the root and the node for "a" are copies (the path to the change), plus a new node for "d"; the nodes for "ab" and "ac" are shared with the old trie, which is unchanged. After removing "ab": the root and "a" are new again, the node for "b" is **gone** (no value, no children), "c" and "d" are shared. Remove "ad" and "ac" too and the "a" node and then the root are pruned: the trie has no root.||
>
> - Which nodes lie on the path from the root to the key?
> - What does cloning a node with `Arc` children copy?
> - What does `remove` do for a key that is not there?

## The task

In `src/primer/trie.rs`:

- **`Trie::put::<T>(&self, key, value: T) -> Trie` is a new trie with `value` under `key`**, replacing what was there whatever its type; the empty key puts the value in the root. Values need not be clonable.
- **`Trie::remove(&self, key) -> Trie` is a new trie without the value under `key`.** A node left with no value and no children is removed, and so on up the path, so an emptied trie has no root. A key that holds no value gives a trie equal to the old one.
- **Both leave the old trie unchanged and copy only the path.** The nodes from the root to the key are new; every other node is shared (the same `Arc`) with the old trie.

The helpers you write for them, and any type they return, are part of the region: their shape is yours.

> [!ASIDE] The steps, if you would rather not work them out
> In `src/primer/trie.rs`: `Trie::put::<T>(&self, key, value: T) -> Trie`. The recursive helper `put_at(node, key, value)` is part of the region: given the old node at this position (if any) and the rest of the key, it returns the new node:
>
> - start from a **clone** of the old node (its children map clones the child `Arc`s, not the subtrees) or from an empty node if there is none;
> - if the key is used up, set `value` on the clone (replacing what was there, whatever its type);
> - otherwise replace the clone's child for the next character by the result of the recursive call on the old child;
> - wrap the result in `Arc::new`.
>
> The empty key puts the value in the root.
>
> In `src/primer/trie.rs`: `Trie::remove(&self, key) -> Trie`, with the helper `remove_at(node, key) -> Removal` in the same region. `Removal` is `Missing` (no value under the key: the answer is the old trie, unchanged) or `Replaced(Option<Arc<TrieNode>>)` (the node that takes this position: `None` if nothing is left of it). For each node on the path: clone it, recurse into the child (or clear the value at the end of the key), then put the replacement child back, drop the child if it became `None`, and return `None` yourself if you now have no value and no children.

The tests: exact scenarios (put builds one node per character; replaces a value even with another type; keys that are prefixes share a path; putting never changes the old trie; only the path is copied and everything else is shared; values need not be clonable; remove deletes only the given key; removing everything leaves a trie with no root; empty nodes are pruned; a node that still leads somewhere stays; removing a missing key changes nothing; removing never changes the old trie), and a property: **random puts and removes, keeping every version**: at each step the new version equals a map, every older version still equals the map it was, the new version shares with the old one every child off the key's path, no empty branch is left behind, and a trie has a root exactly when it holds a key.

## Your freedom

How you copy the path (recursion with a helper, an explicit stack of nodes, rebuilding from the bottom) and how `remove` reports a missing key (an enum, an `Option`, a flag); the contract is the shape of the result, not the helper.

## The Rust toolbox

**`Arc::new` and clone for copy-on-write.** Cloning the struct inside an `Arc` (`Folder::clone(&folder)`, where `folder: Arc<Folder>`) clones its map of children: it copies the child *pointers* (`Arc`s), not the subtrees, so the copy is cheap and shares everything below. Change the copy, then `Arc::new` it.

**Recursion that returns the new node.** A helper such as `fn with_file(old: Option<&Arc<Folder>>, path: &[&str], file: File) -> Arc<Folder>` builds a path bottom-up: each level returns its own new node, and its caller stores that as one child. Taking the *old* node as an `Option` handles a path that does not exist yet with the same code.

**An enum for 'nothing to do'.** A recursive helper with three answers ("nothing changed, keep the old version", "here is the new node", "nothing is left of this node") is an enum with one unit variant and one that carries an `Option`: the caller's `match` must then handle all three, and an early "nothing changed" can skip the copying on the way back up.

**`split_first`.** `match key.split_first() { None => .., Some((c, rest)) => .. }` takes a slice apart without indexing.

**Type erasure on the way in.** `let shared: Arc<dyn Any + Send + Sync> = Arc::new(x);` coerces any `T: Any + Send + Sync` into a shared erased value. Do it once, before the recursion, so the helper need not be generic.

## Design notes

**Why the value is not copied.** The value sits behind its own `Arc`; cloning a node copies the pointer. That is why `put::<T>` works for a `T` that cannot be cloned at all (a `Box` of a type with no `Clone`): the value is moved into the `Arc` once and shared from then on.

**What gets copied.** On the path: one clone per node, each copying that node's child table (pointers). Everything hanging off the path is untouched. For a 10-character key in a trie of a million keys, a `put` allocates 11 nodes.

**Shared means immutable.** `Arc<TrieNode>` gives only `&TrieNode`: there is no way to change a shared node by mistake. This is the property C++ asks you to keep by never removing `const`.

**Why a result type.** Two different "nothing" answers must not be confused: *the key was not there* (keep the whole old trie, all pointers shared) and *the subtree is now empty* (the parent must forget it). Returning `None` for both would make the parent either copy a path for nothing or keep an empty node.

**Pruning is why the tests look at `root()`.** After `remove("te")` of the last key the root must be `None`, not an empty node: BusTub's `RemoveFreeTest` asserts exactly this.

**A value node with children stays.** Removing `te` from `{te, tes, test}` clears the value but the node still has a child; pruning stops there.

## If this is new

- S7 Smart pointers & interior mutability: `Arc`, clone-on-write, sharing.
- L7 Enums & pattern matching: a small enum for a helper's three outcomes.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `Any`, `Send + Sync` bounds.
- Y5 Testing & verification: keeping every version and checking all of them at the end.
- The optional *persistent data structures and path copying* concept.
- [L1 Ownership & moves](/t/l1-ownership-moves): Clones & drops: sharing structure instead of copying: a clone of a node clones pointers.

## Tests

- Put: path, replacement, prefixes, persistence, sharing, non-clonable values.
- Remove: single keys, an emptied trie, pruning, a missing key, persistence.
- Property: every version of a long random history stays what it was; sharing; pruning; the root exists exactly when there is a key.

## Hints

### Recurse on the old child, not on the new node

The clone already has the child pointers; look the child up in the **old** node (`node.children.get(c)`) and replace it with the recursive result.

### An absent node is just an empty node

For a key that extends beyond the existing trie, `put_at(None, rest, value)` builds a fresh chain; there is no separate "create" code path.

### Check sharing in your own test

`Arc::ptr_eq(&old.children[&'x'], &new.children[&'x'])` must hold for every branch off the path.

### Report 'missing' before copying

If the key is not stored, return `Missing` as soon as the walk fails or the last node has no value, so that the caller keeps the old pointer.

### Decide pruning at the end of each level

After updating the clone's children and value, a node with `value.is_none() && children.is_empty()` is gone: return `Replaced(None)`.

### The root is just another node

The same helper handles the root; a trie whose root is pruned is `Trie::from_root(None)`.

## Performance

`put` is `O(len(key) × fan-out)` time and allocates `len(key) + 1` nodes, each the size of its child table. Old versions cost nothing extra: sharing means a million snapshots of one trie are a million root pointers plus the paths that changed.

**Measure it.** Put 23 333 keys one by one and keep every version: it takes about 15 ms in release mode, and memory grows by roughly one path per put, not by the size of the trie.

## Experiment

Optional. Predict first, then run.

1. **Copy the whole trie.** Deep-clone every node in `put`. Which test notices (hint: sharing), and which does not?
2. **No pruning.** Leave empty nodes behind in `remove`. Which property shows it?

## Other designs

- **Path copying with `Arc` (ours, BusTub's):** simple, `O(len)` per write.
- **A mutable trie behind a lock:** fastest single-threaded, no old versions.
- **A persistent trie with structural hashing** (HAMT in Clojure and Scala): fewer levels, wide fan-out, the same sharing idea.
- **Reference counting or epochs** when `Arc` cloning becomes the bottleneck.

## In BusTub

`trie.h`: "`// Create a new trie with the given root.`", `Put`: "`template <class T> auto Put(std::string_view key, T value) const -> Trie;`" and the tests `CopyOnWriteTest1`, `2`, `3` (old versions stay intact), `PointerStability` (the value of an untouched key keeps its address) and `NonCopyableTest`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `node->Clone()` (a virtual method so that nodes with values copy their value) | `(**n).clone()`: derived `Clone`; the value is an `Arc`, so copying it is a pointer copy |
| `std::make_shared<const TrieNode>(...)` | `Arc::new(...)` |
| `std::move(value)` into a `shared_ptr<T>` | `Arc::new(value)`: moved, never copied |

**Port rule:** "clone, then modify the clone" is the whole copy-on-write protocol; shared data is never touched in place.

## Learn more

- [`Arc::ptr_eq`](https://doc.rust-lang.org/std/sync/struct.Arc.html#method.ptr_eq) · [`slice::split_first`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_first)
- [Rust enums](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html) · [Deletion in tries](https://en.wikipedia.org/wiki/Trie#Deletion)
