Most of this course's tests have one shape: run a long random sequence of operations on **your** structure and on a **model**, a simple structure nobody doubts, and compare after every step. When they differ, the framework shrinks the sequence to the shortest one that still differs, and that is your bug report. This last stage lets you meet the technique on a small structure: a ring buffer in `src/rust_primer/ring.rs`.

## The task

`RingBuffer<T>`: a queue of fixed capacity in one array that wraps around.

- `new(capacity)` (0 is allowed: every push is refused), `capacity`, `len`, `is_full`.
- `push(value) -> Result<(), T>`: at the back; a full buffer **gives the value back**.
- `push_overwriting(value) -> Option<T>`: when full, first drops the oldest value and returns it (capacity 0 returns the value itself).
- `pop`, `front`, `back`, `iter` (oldest to newest, a boxed iterator: no `Vec` of the values) and `clear` (drops everything now).

The tests: first-in first-out, a refused push, wrapping around the end of the array many times, overwriting, clear drops the values at once; and the property that matters: **for any capacity and any sequence of `push`, `push_overwriting`, `pop` and `clear`, the ring buffer answers exactly like a `VecDeque` with a limit**, comparing `len`, `is_full`, `front`, `back` and the whole `iter` after every step.

## Your freedom

How the slots are stored (`Vec<Option<T>>`, or `MaybeUninit` if you are brave), whether you keep `head` and `len` or `head` and `tail`; whether `push` is a call of `push_overwriting` or the other way round.

## The Rust toolbox

**Wrap-around indexes.** The newest slot is `(head + len) % capacity`; popping moves `head` to `(head + 1) % capacity`. A capacity of 0 divides by zero: handle it first.

**Moving a value out of a slot.** `self.slots[i].take()` leaves `None` and gives you the value, so a `T` that is not `Copy` can leave the array.

**An iterator without a `Vec`.** `Box::new((0..self.len).filter_map(move |i| self.slots[(self.head + i) % cap].as_ref()))` returns a `Box<dyn Iterator<Item = &T> + '_>`: the box lets the return type be named, and `+ '_` says it borrows the buffer.

**The test, in miniature.** The test in this stage is the pattern to copy:

```rust
proptest! {
    #[test]
    fn matches_the_model(capacity in 0usize..9, ops in ops()) {
        let mut real = RingBuffer::new(capacity);
        let mut model: VecDeque<u8> = VecDeque::new();
        for op in ops {
            match op {                                   // do the same on both
                Op::Pop => prop_assert_eq!(real.pop(), model.pop_front()),
                /* ... */
            }
            prop_assert_eq!(real.len(), model.len());   // and compare the state after every step
        }
    }
}
```

An `enum Op` of the operations; a strategy that makes a `Vec<Op>`; `prop_assert_eq!` after every step. Anything that differs, anywhere in the sequence, is found, and shrunk.

## If this is new

- Y5 Testing & verification: properties, models, shrinking.
- S5 Queues & heaps: `VecDeque`.
- L7 Enums & pattern matching: an operation as an enum.

## Tests

- First-in first-out; full buffers; wrapping through many rounds; overwriting and capacity 0; `clear` drops at once.
- Property (256 cases): the ring buffer against a limited `VecDeque`, over all four operations, with every observer compared after every step.

## Hints

### Keep `len` instead of `tail`

With head and tail alone, empty and full look the same (head == tail). A separate `len` makes both unambiguous.

### Read a shrunk failure from the end

The last operation of the shrunk sequence is the one that went wrong, and the ones before it set up the state. Try it by hand on paper with a capacity of 2 or 3.

### Fix one thing at a time

If three observers differ, the first one that differs after the first bad step is the nearest to the cause.

## Performance

Every operation is `O(1)`, and nothing is allocated after `new`. The `%` is a division; a power-of-two capacity could use a mask (`& (cap - 1)`) instead.

**Measure it.** Push and pop 100 million numbers through a ring of 1024 and through a `VecDeque`: the ring is about as fast; the point of a ring is not speed but the **bound** on memory.

## Experiment

Optional. Predict first, then run.

1. **Break it on purpose.** In `pop`, forget the `% capacity`. How many operations does the shrunk counterexample have, and what is the capacity?
2. **Test the test.** Change the model so that `push_overwriting` keeps the newest instead of the oldest. Does the property pass? What does that tell you about a model you wrote yourself?

## Other designs

- **`VecDeque` with a limit** is the model, and also a fine implementation.
- **A power-of-two ring with free-running counters** (`head` and `tail` never wrapped, masked on use): the design of lock-free queues, because "full" is `tail - head == cap`.
- **A ring of `MaybeUninit<T>`:** no `Option` per slot; needs `unsafe` and a careful `Drop`.

## In BusTub

BusTub has no ring buffer of its own, but queues of bounded size are everywhere (the disk scheduler's requests, the LRU-K history of a frame). More important is the method: **from module 1a on, nearly every stage test is a property against a model** (a map, a set, a sorted vector, a serial-order oracle), and you now know how they are put together.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `buf[(head + n) % cap] = value;` on a raw array | `self.slots[tail] = Some(value);` (checked index) |
| a full buffer silently overwrites, or returns `false` | `Err(value)`: the value comes back |
| a hand-written test with a dozen cases | a generated test with hundreds of random sequences, shrunk |
| compare against the standard library in a debugger | compare against `VecDeque` in the test |

**Port rule:** write the obviously-correct version first (the model), then test your clever version against it with random operations.

## Learn more

- [`proptest`](https://docs.rs/proptest) · [`VecDeque`](https://doc.rust-lang.org/std/collections/struct.VecDeque.html) · [`Option::take`](https://doc.rust-lang.org/std/option/enum.Option.html#method.take)
