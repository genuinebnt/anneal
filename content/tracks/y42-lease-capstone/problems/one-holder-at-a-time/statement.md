**Capstone, step 1 of 5.** In five steps you build a simulation of several clients that take turns writing to one shared disk. A **lease service** decides whose turn it is. At the end you will find, on purpose, a run in which the data is corrupted.

A **lease** is permission with an end time. If its holder crashes, nobody has to clean up: the lease runs out and another client gets one.

## What to write

`LeaseService::new(ttl_ms)` makes a service whose leases last `ttl_ms`. It panics if `ttl_ms` is 0. Every call says what time it is. Times are milliseconds in a `u64`.

- A lease with `expires_at` is **live** at every time **below** `expires_at`. At `expires_at` it has run out.
- `acquire(now, client)` grants a new lease if no lease is live. It returns `None` otherwise, **also to the client that holds the live one**. A new lease has `expires_at = now + ttl_ms`.
- **Tokens.** The first grant has token 1, and each later grant has the next number. A refused call uses no number.
- `renew(now, client, token)` works only if a lease is live, `client` holds it **and** `token` is its token. It sets `expires_at = now + ttl_ms` and keeps the token. A refused renewal changes no lease. A lease that has run out cannot be renewed, even if nobody else asked.
- `holder(now)` is the client whose lease is live, if any.

## The service's clock never goes back

Calls may carry a time **earlier** than one seen before. The service then acts as if the call were made at the **latest time it has seen**. This holds for all three methods, and refused calls count as seen.

If `now + ttl_ms` does not fit in a `u64`, the end time is `u64::MAX`.
