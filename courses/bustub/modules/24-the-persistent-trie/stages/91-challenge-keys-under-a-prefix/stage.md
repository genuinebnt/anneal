A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`keys_with_prefix` in `src/primer/trie_extras.rs`: using the **public** node type of your persistent trie (`Trie::root`, `TrieNode::children`, `TrieNode::value`), list every key stored in a trie that starts with `prefix`, in increasing order. It does not change the trie.

## Why

A trie that can only answer "is this key here" is half a trie: autocomplete, prefix scans and `LIKE 'abc%'` all ask for *everything below a node*. The tree you built already has the answer; the exercise is to walk it, and to notice that `BTreeMap` children make the output sorted without sorting.

## The contract

- `keys_with_prefix(trie, prefix) -> Vec<String>`: all keys `k` with `k.starts_with(prefix)` for which a value is stored, ascending.
- An empty prefix lists every key. A prefix that leads nowhere lists none. The prefix itself is included if it holds a value.
- Works on any trie built with your `Trie::put`; reads only.

## Invariants

These must hold after every step, whatever the input:

- The result is sorted and has no duplicates.
- Every returned key has a value in the trie and starts with the prefix.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The result for prefix `p + c` is a subset of the result for `p`.
- Putting a key then listing finds it; removing it then listing does not.
- The result equals filtering the full key list by `starts_with`.

## Examples

Worked cases (the tests include them):

```text
keys {a, ab, abc, b}: prefix "a" -> [a, ab, abc]; prefix "ab" -> [ab, abc]; prefix "c" -> []
```

## What the tests check

- Nested keys, empty prefix, missing prefix.
- Old versions still list what they held.
- A property against a `BTreeSet`.

## Done when

All the `s0a_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A trie and a compressed radix tree](/cookbook/p/trie-radix): ordered prefix scans by walking children kept in byte order.
- [Trie interview questions in Rust](/cookbook/p/tries-core-questions): prefix queries and autocomplete on a trie.
