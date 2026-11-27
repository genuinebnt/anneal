A program needs a list with four thousand million places, of which a few thousand are ever used. The places that hold 0 should cost nothing.

`List` holds `fan` to the power `levels` numbers, all 0 at first. It keeps them in **nodes** that it gets from a `Pool`. Every node has `fan` slots of 32 bits.

## The shape

With `levels` = 1 there is one node, and its slots are the values. With more levels, a node's slots name the nodes below it. Each slot covers an equal, consecutive share of the node's indices. A slot of 0 means that everything it covers is 0.

So an index written in base `fan`, with `levels` digits, names the slots to follow from the root. The first digit is the root's. `root()` is the root node, or 0 while every value is 0.

## The rule that is always true

**The pool holds exactly the nodes that lie above some non-zero value.** No empty node survives any call, and no node is left over.

## Calls

- `get(pool, i)` is the value at `i`. Beyond the end it is 0.
- `set(pool, i, v)` stores `v`; storing 0 erases. An index at or past the end is `OutOfRange`.
- If the pool runs out during `set`, return `Full` and change **nothing**: same values, same nodes.
- `cut(pool, len)` makes every value at index `len` and after it 0.

`fan` to the power `levels` may be larger than a `u64` can hold. Then every `u64` is in range.

## Limits on reads

The tests count calls of `pool.get`, and stop a call that makes too many. The list may be huge while almost empty, so:

| Call | Most reads |
|---|---|
| `get` | `levels` |
| `set` | `levels * (fan + 1)` |
| `cut` | `2 * fan * (nodes in use before the call)`, plus 1 |

The pool in the tests panics if you free a node twice or touch a free one.
