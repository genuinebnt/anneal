Build `channel(cap)`: a bounded queue split into a cloneable `Sender` and a cloneable `Receiver`, like a
`std::sync::mpsc::sync_channel` that also allows several consumers.

- `send` waits while full. `recv` waits while empty.
- When **every** `Sender` is dropped, `recv` still delivers what was queued, then returns `Err(RecvError)`; a
  `recv` that is already blocked must wake up and fail. `try_recv` reports `Empty` vs `Disconnected`.
- When **every** `Receiver` is dropped, `send` returns `Err(SendError(value))` (even one that was blocked on a
  full channel), and `try_send` returns `Disconnected(value)`; `Full(value)` if merely full.
- `recv_timeout` waits at most the given time in total. `Receiver` is an `Iterator` that ends on disconnect.
