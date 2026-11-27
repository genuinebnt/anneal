> **Spoiler before you open the editor.** The starter holds reference code for **capstone step 1** (the lease service). Solve that step first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 2 of 5.** A lease alone cannot keep two clients from writing at once. A client can stall for longer than its lease, wake up, and write while someone else holds the next one. So now you build the thing being written to, and a way to tell whether it came to harm.

## `Disk`

Each `Write` carries the **token** of the lease it was sent under, and a **value**. A client numbers its writes upwards, so of two writes with the same token, the one with the larger value was sent later.

- `write(w)` is a write arriving. It returns `true` if the disk accepts it.
- A **guarded** disk (`new(true)`) turns away a write whose token is **lower** than the highest token it has accepted. An equal token is accepted.
- An unguarded disk accepts everything.
- `arrived()` lists every write that reached it, `accepted()` the ones it took, both in order of arrival.
- `value()` is the value of the last accepted write, or `None` before the first.

This disk lives in memory and takes one call at a time. A real one must keep the highest token through a restart, and must check it and write in **one atomic step**. And the guard only helps once the disk has **seen** a newer token. A holder that reads, changes and writes must show the disk its token **before** it reads.

## `judge`

`judge(arrived, accepted, value)` looks at a disk **from outside**. It gets plain lists, so it can also judge a disk that is broken. It never sees whether the disk was guarded. Check two rules, **in this order**:

1. **Tokens never go back.** In `accepted`, no token is lower than the token of an earlier write. If this is broken, return `StaleWrite`. `stale` is the **first** write in the list whose token is lower than an earlier one. `newer` is the accepted write just before it.
2. **The newest write wins.** Look at all of `arrived`. The newest write is the one with the highest token and, among those, the largest value. `value` must be its value. If not, return `NewestLost` with that write and the `value` you were given.

If `arrived` is empty, rule 2 holds. Otherwise `Ok(())` means both rules hold. In `arrived`, no two writes share both token and value.

## Size

`accepted` may hold 200,000 writes. The tests run in a debug build, so do not compare every pair.
