A `Hub` delivers messages to subscribers. `Hub::subscribe` gives the caller a handle, an `Rc<Subscriber>`. The
subscription should last **as long as the caller keeps a handle**.

A long-running service using this grows without limit, and closed sessions still receive messages. Fix the ownership.
No `unsafe`.

- `Hub::new()` and `Hub::subscribe(&hub, name)`.
- `publish(text)` appends `text` to the inbox of every **live** subscriber, in subscription order, and returns how many
  received it.
- A subscriber is gone as soon as the caller's last handle is dropped: its memory is released **then**, not at some
  later call.
- The hub is gone as soon as the caller's last handle to the hub is dropped, even while subscribers are alive.
- `subscriber.post(text)` publishes `"<name>: <text>"` to every live subscriber **except itself** and returns
  `Some(count)`, or `None` if the hub is gone.
- `entries()` is the number of entries the hub is storing. After any `publish` or `post` it equals the number of live
  subscribers.
