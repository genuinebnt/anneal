A trie stores a string key as a path: one node per character, from the root. A **value** sits on the node where the key ends, and it may be of *any* type: the same trie can hold a `u32` under one key and a `String` under another. `get::<T>` asks for the value at a key **as a `T`**, and gets nothing if the key is missing, leads only to longer keys, or holds a value of another type.

## The task

In `src/primer/trie.rs`:

- `Trie::get::<T>(&self, key) -> Option<&T>`: walk from the root along the characters of `key` (each node's `children` map); at the end the node must have a `value`; downcast it to `T` (`downcast_ref`).
- `Trie::get_shared::<T>(&self, key) -> Option<Arc<T>>`: the same walk, but return an owner of the value (`Arc::downcast`) so that the caller can keep it after the trie is gone (the store of stage 4 needs it).

Stage 1 comes before `put`, so the tests build their tries by hand from the given `TrieNode` fields and `Trie::from_root`.

The tests: exact scenarios (an empty trie has no values; `get` finds the value at the end of the key; a prefix that holds no value is not found; the requested type must match the stored one; the empty key lives in the root; a node can have a value and children; `get_shared` gives an owner of the value), and a property: **a trie built by hand from any set of keys answers `get` for every key of a 31-key universe as a map does**: present keys with their value, everything else (prefixes, extensions, a wrong type) with nothing.

## Your freedom

Almost none: the node type and the trie are given (they are the contract), and a walk down a tree is hard to do in more than one way. The freedom is in how you write the loop (a `for` with `?` on each step, a fold, recursion).

## The Rust toolbox

**`?` on an `Option` inside a loop.** `node = node.children.get(&c)?;` returns `None` from the whole function the moment a character has no child: the shortest possible 'not found'.

**Type-erased values.** A value is `Arc<dyn Any + Send + Sync>`; `value.downcast_ref::<T>()` gives `Some(&T)` only if the stored value really is a `T`, which is how one trie holds a `u32` under one key and a `String` under another.

**`Arc::downcast`.** `value.clone().downcast::<T>().ok()` turns the shared erased pointer into an `Arc<T>` that owns the value independently of the trie.

**`char`s, not bytes.** `key.chars()` walks Unicode scalar values: the children map is keyed by `char`, so a multi-byte letter is one step.

```rust
let mut node = self.root.as_ref()?;              // Option<&Arc<TrieNode>>: no root, no value
for c in key.chars() { node = node.children.get(&c)?; }
node.value.as_ref()?.downcast_ref::<T>()          // Option<&T>
node.value.clone()?.downcast::<T>().ok()          // Option<Arc<T>>
```

## Design notes

**`?` is the algorithm.** Every failure in the walk means "not found": a missing child, no value, wrong type. In a function returning `Option`, `?` turns each into an early `None`.

**Type-erased values.** `Arc<dyn Any + Send + Sync>` forgets the concrete type; `downcast_ref::<T>()` checks a hidden type id and returns `None` on a mismatch. That is the Rust counterpart of BusTub's `dynamic_cast<TrieNodeWithValue<T> *>`.

**Keys are `char`s.** BusTub's children are keyed by `char`; here `chars()` yields Unicode scalar values, which for ASCII is the same thing.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `?` on `Option`, `as_ref`, `and_then`.
- S7 Smart pointers & interior mutability: `Arc` and sharing.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Any` and downcasting.
- The optional *tries* concept.
- L7 Enums & pattern matching: Enums & exhaustiveness: a helper's outcomes as an enum; `dyn Any` downcasting.
- Y5 Testing & verification: Build it: keep every version of a history and check them all.

## Tests

- An empty trie; found values; prefixes; types; the empty key; nodes with value and children; `get_shared`.
- Property: tries built by hand agree with a map on every key of the universe.

## Hints

### The walk and the check are two steps

Reaching the node for the last character does not mean the key is stored: check the node's value separately.

### Do not unwrap the downcast

A value of another type is a normal answer (`None`), not a bug. Keep the `Option` flowing.

## Performance

`get` is `O(len(key) × log fan-out)`: one ordered-map lookup per character, independent of how many keys the trie holds. Nothing is allocated or locked.

**Measure it.** Look up the same 10-character key in tries of 100 and 1 000 000 keys: the time is the same.

## Experiment

Optional. Predict first, then run.

1. **Forget the type check.** Return the value as any type you are asked for (unsafe or a panic). Which test says why that is not an option?
2. **Bytes instead of chars.** Walk `key.bytes()` with a `u8` map. Which keys break?

## Other designs

- **A map of children per node (given, BusTub's):** simple, ordered, memory-hungry.
- **Array of children** for a small alphabet: faster, bigger.
- **A radix (compressed) trie:** a node holds a whole substring; fewer nodes, more cases.
- **A hash map from key to value:** no prefixes, no shared versions.

## In BusTub

`trie.h`: "`// A Trie is a data structure that maps strings to values of type T. All operations on a Trie should not modify the trie itself. It should reuse the existing nodes as much as possible, and create new nodes to represent the new trie.`" and "`You are NOT allowed to remove any `const` in this project, or use `mutable` to bypass the const checks.`"

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `auto *with_value = dynamic_cast<const TrieNodeWithValue<T> *>(node.get()); if (with_value == nullptr) return nullptr;` | `node.value.as_ref()?.downcast_ref::<T>()` |
| `if (it == children.end()) return nullptr;` | `node.children.get(&c)?` |
| returns `const T *` | returns `Option<&T>` |

**Port rule:** a pointer that may be null is an `Option<&T>`; each null check before use becomes `?`.

## Learn more

- [`Any::downcast_ref`](https://doc.rust-lang.org/std/any/trait.Any.html#method.downcast_ref) · [`Arc::downcast`](https://doc.rust-lang.org/std/sync/struct.Arc.html#method.downcast)
