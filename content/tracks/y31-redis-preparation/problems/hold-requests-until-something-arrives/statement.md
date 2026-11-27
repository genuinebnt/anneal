A lending library lets members ask to borrow books that are all out. A member may name several books, and takes **whichever comes back first**. Some members only wait until a deadline.

Write `Waiting<T>`. Each waiting request has an `id`, the items it wants, an optional deadline, and a ticket of type `T` that the caller gets back.

- **`park(id, wants, deadline, ticket)`** starts waiting and returns `true`. If `id` is already waiting, it returns `false` and changes nothing. An item named twice counts once. `wants` may be empty.
- **`wake(item)`**: one such item has arrived. Of those who want it, the one who has **waited longest** gets it.

  It stops waiting for everything, and is returned with its ticket. `None` if nobody wants the item. One arrival wakes one waiter.
- **`cancel(id)`** stops `id` waiting and returns its ticket.
- **`expire(now)`** removes and returns everyone whose deadline is `now` or earlier: earliest deadline first, and for equal deadlines the longest waiting first. A deadline already past when parked is only acted on here.
- **`next_deadline()`** is the earliest deadline among those waiting.
- `contains(id)`, `len()`, `is_empty()`.

An `id` that parks again after leaving starts at the back of every queue.

**Speed.** There may be 200000 requests, many waiting for the same item. No call may walk through a queue or through everyone who waits. Each should take about `log n` steps for each item or waiter it handles.
