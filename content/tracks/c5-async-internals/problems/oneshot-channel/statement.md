Implement `channel::<T>() -> (Sender<T>, Receiver<T>)`, a channel that carries **one** value.

`Sender::send(self, v)` stores the value and wakes the receiver, or returns `Err(v)` if the receiver is already dropped.
`Receiver<T>` is a `Future<Output = Result<T, RecvError>>`: `Ok(v)` when a value arrives, `Err(RecvError)` if the sender is
dropped without sending. `try_recv` is the non-blocking version (`Empty` while the sender may still send, `Closed` once it cannot).

On `Pending` the receiver stores the waker from **this** poll's context (replace the old one: the tests poll with
two different wakers in turn) and the sender wakes it after releasing the lock.
