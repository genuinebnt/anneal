`Observe` is a tower middleware that counts requests. It was added to a stack that has a concurrency limit, and since
then the service **panics** under load. Its `in_flight` gauge also climbs for ever, although clients that give up are
normal (a timeout layer drops their futures).

Fix `Observe`. Keep the public types.

- It must work over **any** `Service` that is `Clone`, and keep that service's rule: **`call` only on a value whose
  `poll_ready` returned ready**. Readiness, errors from `poll_ready` included, must come from the inner service.
- `Stats` counts, for calls made through the middleware:
  - `in_flight`: calls started (from the moment `call` returns) whose future has neither finished nor been dropped;
  - `ok`, `err`: futures that finished with `Ok` or `Err`;
  - `cancelled`: futures dropped before they finished, **including ones never polled**.
- Every call ends up in exactly one of `ok`, `err`, `cancelled`.
- Responses and errors pass through unchanged.
