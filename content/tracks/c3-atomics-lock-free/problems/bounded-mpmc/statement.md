Write `Queue<T>`: a bounded FIFO that any number of threads may `push` to and `pop` from at the same time, through
`&self`, with no lock. `push` gives the value back (`Err`) when full and `pop` returns `None` when empty; neither blocks.
Capacity is rounded up to a power of two (at least 2) and reported by `capacity()`.

The trap is the gap between *claiming* a position and *writing* the value: a consumer must never read a slot
whose producer has claimed it but not finished writing. Give every slot a sequence number saying whose turn it is:
for slot `i` and lap `k` it holds `i + k * cap` when a push may fill it and `i + k * cap + 1` when a pop may take it.
Values left in the queue when it is dropped must be dropped exactly once.
