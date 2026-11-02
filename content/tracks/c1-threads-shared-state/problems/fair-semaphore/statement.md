`Semaphore::new(total)` hands out up to `total` permits. `acquire(n)` waits until `n` permits are free **and every
earlier caller has been served**: a request for 3 permits that arrived first must not be overtaken by later requests
for 1, even when one permit is free. It returns a `Permit` that gives its `n` permits back when dropped.
`try_acquire(n)` never waits and also respects the queue (it fails if anyone is waiting). `acquire` and `try_acquire`
panic unless `1 <= n <= total`. `waiting()` reports how many threads are queued inside `acquire`; `available()` how many
permits are free.
