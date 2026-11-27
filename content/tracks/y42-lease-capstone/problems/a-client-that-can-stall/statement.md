> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 and 2** (the lease service, the disk and its judge). Solve those steps first.
>
> You may replace the code with your own; keep the public names.

**Capstone, step 3 of 5.** Now the one who uses the lease. A **client** asks for a lease, then writes to the disk a few times under it. Between writes it renews the lease. It does this a fixed number of times.

The `Client` has **no clock and no network**. `handle(event)` takes one thing that reached it and returns at most one `Action`. The world that carries out actions comes in step 4.

`Client::new(leases, writes_per_lease)` wants `leases` leases. `leases_left()` says how many are still to be granted.

## The life of a client

It starts out **waiting to wake**.

| While | On the event | It does |
|---|---|---|
| waiting to wake | `Wake`, and a lease is still wanted | `Acquire` |
| waiting for the answer | `Granted(lease)` | counts one lease as used, then `Work` |
| waiting for the answer | `Denied` | `Backoff`; waits to wake |
| working | `WorkDone { now }` | looks at the lease: see below |
| passed the check | `PauseOver` | `Write { token }` |
| waiting for the write | `WriteDone` | `Renew { token }`, or `Rest` after the last write |
| waiting for the renewal | `Renewed` with `Some(end)` | takes the new end, then `Work` |
| waiting for the renewal | `Renewed` with `None` | `Rest`; waits to wake |

After every `Rest` the client waits to wake. If `writes_per_lease` is 0, a grant leads straight to `Rest`.

## The check before a write

On `WorkDone { now }`, the client compares `now` with the end of its lease, as last told. A lease is live **below** its end. If it is live, return `Passed`. If not, give the lease up: return `Rest`.

`PauseOver` means whatever held the client up after `Passed` is over. It carries **no time**. The client now writes. A second look at a clock would only move the same problem one line down.

## Old and repeated messages

- A `Granted` counts only if its token is **higher than every token this client was ever granted**. A grant delivered again must not be used again. Tokens start at 1, as in step 1: no grant has token 0, and the tests never send one.
- A `Renewed` counts only if its `token` is the token of the lease the client is waiting to renew.
- `WriteDone` and `Denied` carry no number, so this client cannot tell old ones from new. The world of step 4 never delivers a message twice.

## What the client ignores

The answer to a write may say `accepted: false`. The client **carries on exactly as if it had said `true`**. This is deliberate: later you run the same world against two disks, and everything but the disk's verdicts must stay the same.

**Any other event in any state** returns `None` and changes nothing. That includes a `Wake` when no more leases are wanted.
