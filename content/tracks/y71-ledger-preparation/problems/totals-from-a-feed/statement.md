A chain of shops shows a live board: each shop's takings, and each day's. The board is fed from the sales log. The feed is not tidy. It sends batches again after a reconnect, and sometimes skips ahead.

Positions in the log start at 0 and have no gaps. A `Record` has its position `pos`, the `day` it happened, and a `Sale` or a `Refund` for a shop.

Write `Board`.

**`apply(batch)`** counts the records the board has not counted yet.

- The board always knows the position it needs next, `next()`. A new board needs 0.
- Go through the batch in order, with the position needed next in hand. A record **below** it is old: skip it. A record **at** it counts, and the next one moves up by one. A record **above** it would leave a hole.
- With a hole, return `Gap { expected, got }` and leave the board **exactly as it was**, even if earlier records in the batch were new. `expected` is the position needed at that point, and `got` is the record's.
- Otherwise return how many records were counted.

**Questions:**

- `net(shop)`: all its sales minus all its refunds. It can be negative. An unknown shop has 0.
- `day(day)`: `(sales, refunds)` of that day, over all shops. A refund counts on **the day it is made**. It never changes an earlier day's numbers.
- `top(n)`: up to `n` shops as `(name, net)`, highest net first. Equal nets are ordered by name. Every shop with a record counts, also with a net of 0 or less.

**`rebuild(log)`** is the board for a whole log. **`as_of(log, pos)`** is the board for the records before position `pos`. Here `log[i].pos == i`, and `pos` may be larger than the log.

Two boards that counted the same records are equal. Amounts are `u64`, and the totals must not overflow. A log can be hundreds of thousands of records long and arrive one record at a time.
