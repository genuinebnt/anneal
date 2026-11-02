Implement the doubly linked list that safe Rust famously cannot express cheaply: `push_front`, `push_back`,
`pop_front`, `pop_back`, `front`, `back`, `len`, `append` (O(1), empties `other`), `retain` (unlink in place), and
`iter` / `iter_mut` that are **double-ended** (calling `next` and `next_back` alternately yields every element once and
the two ends meet cleanly).

Nodes are heap allocations referenced by `NonNull`. Dropping the list drops every element once, **without recursion**
(a million-node list must not overflow the stack). Document the invariants in a comment and give each `unsafe` block a
`// SAFETY:` justification.
