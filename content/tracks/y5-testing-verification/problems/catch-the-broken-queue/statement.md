This time **you write the test**. `BoundedQueue` is a FIFO of `u32` with a fixed capacity. Write

```rust
pub fn check<Q: BoundedQueue>(make: impl Fn(usize) -> Q)
```

which must **return normally for a correct queue** and **panic for a broken one**. `make(cap)` returns a new empty
queue holding at most `cap` items; you may call it as often as you like, with any `cap` from 1 up.

The contract of a correct queue:

- `push(v)` appends `v` and returns `Ok(())`; when the queue already holds `cap` items it returns `Err(v)`, giving back
  **the value that was passed in**, and changes nothing.
- `pop()` removes and returns the oldest item, or `None` when empty.
- `peek()` returns the oldest item without removing it, or `None`.
- `len()` is the number of items; `is_empty()` is `len() == 0`.
- Any `u32` is an ordinary value, and equal values are stored as often as they are pushed.

The tests run your `check` on one correct queue (written differently from the obvious one) and on a series of queues
that each break the contract in one way. Some of the bugs only show at a particular capacity, value or history. Your
check must be **deterministic** and finish quickly (each test calls it once, in a debug build).
