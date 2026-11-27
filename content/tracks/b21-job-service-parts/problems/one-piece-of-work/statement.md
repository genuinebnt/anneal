Your service runs code it does not trust to behave: a decoder for uploaded files. It may crash, hang, or be no longer wanted. Whatever happens, the caller needs a clear answer, and nothing may be left running.

Write `supervise(cancel, limit, work)`. It runs `work` and says how it ended:

| `Ending` | When |
|---|---|
| `Done(value)` | `work` returned `value` |
| `Panicked(message)` | `work` panicked. The panic does **not** reach the caller of `supervise` |
| `Cancelled` | the `cancel` channel held `true` first |
| `TimedOut` | `limit` passed first, counted from the call |

**The panic message** is the panic payload when it is a `String` or a `&str`. For any other payload it is `"unknown panic"`.

**New tool: a watch channel.** `tokio::sync::watch` holds **one value**, the latest. The `Sender` replaces it, and each `Receiver` can look at it or wait for it to change. It is not a queue: a receiver may never see a value that was replaced quickly. `receiver.wait_for(|value| ..).await` first tests the current value, then each newer one, until the test is true. It returns an error if the sender is dropped while no value it can still see passes the test.

**Cancellation.** `cancel` is a `tokio::sync::watch` channel of `bool`. Cancel when its value is `true`, whenever that happens.

- If it is already `true` at the call, return `Cancelled` and never poll `work`.
- Sending `false`, any number of times, changes nothing.
- If the sender is **dropped** without having sent `true`, nobody can cancel any more. The work carries on.
- The sender may send `true` and be dropped right after. That still cancels.

**Nothing is left behind.**

- When `supervise` returns, with any ending, the future `work` **has already been dropped**. This holds even if `work` was busy on another thread when it was cancelled or timed out.
- If the `supervise` future is itself dropped before it finishes, `work` is cancelled: its future is dropped soon after.

`limit` may be `Duration::MAX`. The `supervise` future is `Send`, so it can be spawned. The tests never put two of these events at the same instant.
