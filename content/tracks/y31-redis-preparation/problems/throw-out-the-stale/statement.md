A web service stores a session for every visitor, each with an end time. A session that has ended is noticed when somebody asks for it. Most visitors never come back, so their sessions stay and fill the memory.

Looking through every session would stall the service. Instead, each sweep **looks at a few picked at random**, and keeps going only while that pays off.

Write `Sessions` with `set(id, ends_at)`, `remove(id)`, `ends_at(id)`, `len()` and `sweep(now, dice, max_rounds)`. A session has **ended** when `ends_at <= now`.

`set` on an id that is already there only moves its end. It adds and removes nothing.

**Picking.** `dice.below(n)` returns a number in `0..n`. A look calls `dice.below(len())`, with the number of sessions at that moment, and looks at the session that number stands for.

While no session is added or removed, each number in `0..len()` stands for one session, and different numbers for different sessions. Which one is up to you.

**A round** is up to `min(20, len())` looks, with `len()` taken when the round starts. Each ended session it looks at is removed at once. A round also stops if no session is left.

**`sweep`** runs rounds and returns the ids it removed. After a round, it runs another only if

- more than a quarter of that round's planned looks removed a session (with 20 looks, 6 or more),
- sessions remain, and
- it has run fewer than `max_rounds` rounds.

With no sessions or `max_rounds == 0`, it does nothing and throws no dice.

The store holds hundreds of thousands of sessions. `set`, `remove`, `ends_at` and **each look** must take constant time on average.
