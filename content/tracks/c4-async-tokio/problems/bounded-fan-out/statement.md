Write `fan_out(inputs, limit, f)`: run `f(input)` for every input on spawned Tokio tasks, with **at most `limit`** of
them unfinished at any moment, and return the outputs **in input order**.

- `f` is called **only when its task is about to start**: never more than `limit` calls whose futures are unfinished.
  Inputs are started in order.
- A slot is refilled as soon as **any** task finishes. A slow input must not hold back the others.
- **The first error to happen** (in time, not in input order) is returned. No further inputs are started, the other
  tasks are cancelled, and **their futures are already dropped when `fan_out` returns**.
- If a task panics, `fan_out` panics with the same payload.
- No inputs gives `Ok(vec![])`. `limit == 0` panics.
