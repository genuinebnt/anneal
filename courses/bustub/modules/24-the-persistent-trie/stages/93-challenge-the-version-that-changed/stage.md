A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/primer/shared_list.rs` is a small persistent list: `push` returns a **new** list and the old one must stay exactly as it was. It looks right, and after a push the old version has changed. Find the bug and fix it.

## Why

Sharing and mutation do not mix: once two values point at the same cell, a write through one is visible through the other. A persistent structure is correct only if no shared node is ever written.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`push` rewrote the shared head cell in place (through the `RefCell`) to make room at the front, so every version sharing that cell changed. A persistent push allocates a new node pointing at the old head and writes nothing shared.||

## The contract

- `PList::new()`, `push(&self, v) -> PList` (a new version with `v` at the front), `to_vec(&self)` (front first), `len`.
- Every version keeps its contents for ever, whatever is pushed onto it or onto its descendants.

## Invariants

These must hold after every step, whatever the input:

- `to_vec` of a version never changes after the version is created.
- `push` adds exactly one element at the front.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `a.push(x).to_vec() == [x] + a.to_vec()`.
- Two pushes onto the same version do not see each other.
- The length of a version is fixed at creation.

## Examples

Worked cases (the tests include them):

```text
a = [] ; b = a.push(1) ; c = b.push(2) ; d = b.push(3): b = [1], c = [2,1], d = [3,1]
```

## What the tests check

- Old versions after a push.
- Branching from one version.
- A property against a vector model.

## Done when

All the `s0a_c4` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Linked-list interview questions in Rust](/cookbook/p/linked-lists-in-rust): a persistent shared-tail list with Rc.
- [More primers](/cookbook/p/more-primers): a persistent trie where no shared node is ever written.
