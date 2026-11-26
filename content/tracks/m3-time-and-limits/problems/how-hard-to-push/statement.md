You send records to a service in batches. It accepts a batch whole or refuses it whole. It refuses when the batch is larger than what it can take **right now**. It never tells you that number, and the number changes.

Write the part that chooses each batch size. There is no clock: one batch and its answer make one round.

## What to write

`Batcher::new(max)` (`max >= 1`). Then, in a loop, the caller asks `next_size()`, sends that many records and calls `report(accepted)`.

`next_size()` is always from 1 to `max`, and asking twice without a report gives the same answer.

## How it is judged

The tests play the service. It accepts a batch when `size <= capacity`. **Your rule is free**; it only has to meet all of these.

1. **Steady service.** `max` is 1,000 or larger, and capacity stays at some `c` from 20 to 500. After 1,500 rounds, look at the next 2,000. At most **300** are refused, and the accepted records are at least **60%** of `2000 × c`.
2. **Sudden fall.** Capacity has been at some value from 200 to 2,047 for at least 700 rounds (`max` at least that value). It becomes 10. One of the next **16** batches is accepted.
3. **Rise.** After at least 1,500 rounds at capacity 20 (`max = 1000`), capacity becomes 600. Within **700** rounds, a batch of at least **400** is accepted.
4. **Room to spare.** When capacity is at least `max`, then after `max + 100` rounds at least **95%** of `2000 × max` gets through in the next 2,000.
5. **Outage.** At capacity 0 everything is refused and sizes stay valid. When capacity returns, rule 3 and then rule 1 hold again.

Rule 1 also holds after any earlier history of changing capacities. `max` can be as large as `u32::MAX`.
