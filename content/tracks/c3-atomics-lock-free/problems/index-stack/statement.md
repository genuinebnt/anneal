Write `Stack`, a lock-free LIFO of `u64`s with a fixed capacity, shared by reference between any number of
threads. Values live in a pool of slots; slots are linked by index. There are two lists in the pool: the stack
itself and a free list of unused slots, and both are classic Treiber lists (push: set `next`, then CAS the head;
pop: CAS the head to the slot's `next`).

Pop reads `next[index]` *before* its CAS. If another thread pops that slot, pops its successor and pushes the slot
back, the head looks unchanged but `next` is stale, and a plain CAS corrupts the list. Pack a **32-bit version
tag** with the 32-bit index in each head and bump it on every successful CAS, so that CAS fails instead.

`push` returns `Err(value)` when the pool is empty; `pop` returns `None` when the stack is empty.
