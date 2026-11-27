A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Pins` and `PinHandle` in `src/common/pin_handle.rs`: `Pins::pin()` returns a handle and raises the shared pin count by one; **cloning** a handle raises it; **dropping** a handle lowers it. `count()` is always the number of live handles, from any thread.

## Why

A pin count is a reference count with a meaning: while it is above zero the page may not be evicted. Doing it by hand (increment here, decrement there) is how counts drift; tying it to the handle's lifetime makes the count correct by construction, and `Clone` is the case people forget.

## The contract

- `Pins::new()` starts at 0; `pin()` returns a handle and counts it.
- `PinHandle::clone` counts as another pin.
- Dropping a handle uncounts it; `count()` reads the live total.

## Invariants

These must hold after every step, whatever the input:

- `count()` equals the number of live handles at every moment.
- The count never underflows.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Cloning and then dropping the clone leaves the count as it was.
- The count at the end of a scope is the count at the start, whatever happened inside.
- Handles moved to other threads still count, and uncount when dropped there.

## Examples

Worked cases (the tests include them):

```text
pin, pin -> 2; clone one -> 3; drop two -> 1; drop the last -> 0
```

## What the tests check

- Pin, clone, drop.
- Counts across threads.
- A property: random clones and drops against a live-handle count.

## Done when

All the `s1g_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Your own `Arc<T>` and `Weak<T>` from scratch](/cookbook/p/own-arc-refcount): a shared count kept exact by Clone and Drop, built by hand.
- [Buffer pool manager](/cookbook/p/buffer-pool-manager): page guards that hold a pin and release it on drop.
