A document store keeps **objects** that refer to each other, and **pages** that hold their content. It also keeps two summaries: for each object, how many references it has, and for each page, whether it is taken. After a bad week, the summaries no longer match.

Write `audit`, which says what is wrong, and `repair`, which fixes only what is safe to fix. The store may be damaged in any way, including references that go in circles.

## What to believe

- The root is object 0. An object is **reached** if it is the root, or a child of a reached **readable** object.
- An unreadable object can be reached, but its `children` and `pages` are rubbish: use neither.
- A **reference** is one entry in the `children` of a reached readable object. An object listed twice has two.
- A page is **claimed** if a reached readable object lists it.

`links` is only the store's belief. It never decides whether something is reached.

## Findings

`audit` returns them sorted (in the order the enum derives), each once.

| Finding | When |
|---|---|
| `NoRoot` | there is no object 0 |
| `Unreadable{id}` | a reached object is unreadable |
| `Dangling{from, to}` | a reference names an id with no object |
| `BadPage{id, page}` | a reached readable object lists a page the store does not have |
| `ClaimedTwice{page}` | a page is listed more than once, by one object or by several |
| `WrongLinks{id, stored, found}` | a reached object's `links` differs from its number of references |
| `Lost{id}` | an object exists and is not reached |
| `Unmarked{page}` | a claimed page is not marked in use |
| `Leaked{page}` | a page marked in use is not claimed |

## Repair

`repair` returns how many findings it fixed.

- `Unmarked` is always fixed: marking a page cannot lose anything.
- `WrongLinks` is fixed by storing `found`, `Lost` by removing the object (its slot becomes `None`), `Leaked` by unmarking. **But only if nothing is `NoRoot`, `Unreadable`, `Dangling` or `BadPage`.** Then part of the store could not be followed, and what looks unused may be in use.
- Nothing else changes.

A chain may be 300 000 objects long. The tests run in a debug build.
