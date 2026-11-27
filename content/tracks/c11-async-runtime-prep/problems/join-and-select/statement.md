Sometimes one task has to wait for several things at the same moment. You could give each one its own task. Often you don't want to: they borrow the same data, or you want to give up on the others as soon as one is done.

Write two futures that run other futures **inside the current task**. Nothing is spawned and no thread is started.

**`join_all(futures)`** finishes when every child has finished. Its output is the children's outputs **in input order**.

- Each time it is polled, it polls **every** child that has not finished yet, once, in input order. One child being pending never stops the later ones from being polled.
- A child gets the same `Context` that `join_all` was given.
- A finished child is **never polled again**, and it is **dropped during the poll in which it finished**. Its output is kept.
- `join_all` returns `Ready` in the same poll in which the last child finishes. With no children it is ready on the first poll, with an empty vector.

**`select(a, b)`** finishes when the first of two futures finishes. The two may have different output types.

- Each poll polls `a` first. If `a` is ready, `b` is **not polled** in that poll. Otherwise `b` is polled. So when both could finish, `a` wins.
- The result is `Either::Left(output of a)` or `Either::Right(output of b)`.
- **Both children are dropped before `select` returns `Ready`.** The one that lost is dropped then, not later when the `select` itself is dropped.

**Both futures:**

- Children need not be `Unpin`. An `async` block must work as a child.
- Dropping the combinator drops every child it still holds, and every output it has kept.
- Polling the combinator again after it returned `Ready` **panics**. It must not return `Pending` for ever.
