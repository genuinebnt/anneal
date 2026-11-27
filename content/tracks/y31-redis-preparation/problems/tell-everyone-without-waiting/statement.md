A trading floor sends price updates to many screens. Each screen listens to some topics. One frozen screen must not delay the others, and its unread updates must not pile up until the server runs out of memory.

Write `Hub<M>`. Each member has its own queue, a Tokio `mpsc` channel that holds at most `capacity` messages. The hub keeps the sending end.

- **`join(capacity)`** adds a member with a new id (never reused) and returns its receiving end. `capacity` is at least 1.
- **`subscribe(id, topic)`** and **`unsubscribe(id, topic)`** change what a member listens to. Both return the number of topics it is on afterwards, or `None` for an unknown id. Repeating either changes nothing.
- **`leave(id)`** removes the member from everything and drops its sending end.
- **`publish(topic, message)`** puts a copy in the queue of every member on the topic. **It never waits.**

`publish` returns a `Delivery`.

- `delivered` counts the queues that took the message.
- A member whose queue is **full** is **evicted**, as if it left. `evicted` lists these in id order.
- An evicted member's receiver can still read what was queued. Then it sees the end.
- A member whose receiver was dropped is removed too. It is neither counted nor listed.

- **`members()`** counts members. **`topics()`** counts topics with at least one member.

Each member sees messages in the order they were published. There may be 100000 members. `publish` must take time only for the members of its topic, plus at most a logarithm each. It must not clone a message more than once for each of them.

The tests read queues with `try_recv`; no async runtime is needed.
