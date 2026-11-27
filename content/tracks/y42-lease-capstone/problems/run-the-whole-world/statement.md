> **Spoiler before you open the editor.** The starter holds reference code for **capstone steps 1 to 3** (the lease service, the disk and its judge, the client). Solve those steps first.
>
> It also holds an agenda, which solves much of this preparation part:
>
> - **"What happens next?"**
>
> You may replace the code with your own; keep the public names.

**Capstone, step 4 of 5.** Now put the pieces into one world: some clients, one lease service, one disk, and a network that takes time. Nothing sleeps. How long each thing takes comes from `Delays`, which the caller gives you. A test can therefore script any run it likes.

One thing this world gets for free: the clients and the service read **one perfect clock**. Real clocks drift and jump.

## `simulate(config, delays)`

Make `config.clients` clients numbered from 0, each wanting `leases_per_client` leases with `writes_per_lease` writes. Make a service with `ttl_ms` (0 panics, as in step 1) and a disk that is guarded or not.

First, for each client **in number order**, plan its first `Wake` after `wait(client, Wait::Start)`. Then handle events until none is left. Events happen **in order of time**; equal times happen **in the order they were planned**.

An **event** is one arrival: at a client, at the service or at the disk. The end of a client's work is an event, and so is the end of its pause, **even a pause of 0**.

## What each action causes

| The client returns | What the world plans |
|---|---|
| `Acquire` | the request reaches the service after `Net`. The service answers as it arrives; the answer reaches the client after another `Net` |
| `Work` | after `Work`, `WorkDone` with `now` = the time **then** |
| `Passed` | after `Pause`, `PauseOver` |
| `Write { token }` | the write reaches the disk after `Net`. The disk's `WriteDone` reaches the client after another `Net` |
| `Renew { token }` | there after `Net`, answered with `Renewed`, back after `Net` |
| `Backoff` | after `Backoff`, `Wake` |
| `Rest` | after `Rest`, `Wake` (even if the client wants no more leases) |

- **Values.** Each write gets the next value, 1, 2, 3 and so on over the whole run, **when the client sends it**.
- The `time` of a `Write` on the disk is the time it **arrives**. The service and the disk act at the time of arrival.
- **Calling `wait`.** Call it exactly once for each delay in the list above, with that client's number. So the third `Net` call for client 1 is about the third message sent to or from client 1.

## The trace

`Report::trace` records these, in the order they happen, with the time and the client:

| Entry | Recorded when |
|---|---|
| `Granted` or `Refused` | an acquire request is answered at the service |
| `Renewal` | a renewal is answered at the service; `expires_at` is `None` if refused |
| `Checked { passed }` | a client handles `WorkDone`; `passed` says whether it returned `Passed` |
| `Paused { ms }` | right after a passed check, **only if** the pause is above 0 |
| `Sent` | a client sends a write |
| `Stored` | a write arrives at the disk |

`Report` also holds the disk's `arrived`, `accepted` and `value`, and `end_time`: the time of the last event, or 0 if there was none.

## Errors

- If more than `max_events` events would have to be handled, return `OutOfSteps`. Exactly `max_events` is fine.
- If any planned time does not fit in a `u64`, return `TimeOverflow`.

## Size

A run may have 300 clients and well over 100,000 events. The tests run it in a debug build.
