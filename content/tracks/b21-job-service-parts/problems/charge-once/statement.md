A customer taps "Pay". The network is slow, so the app sends the request again. If your service charges the card twice, you have a very bad day.

Payment APIs solve this with an **idempotency key**. The client makes up a key for one purchase and sends it with every attempt.

Write `Payments`. `charge(key, amount, send)` charges through `send(amount)`, an async call to the bank. The bank answers with a receipt, `Ok(String)`, or a refusal, `Err(String)`.

**A new key** calls `send(amount)`.

- A receipt is returned as `Ok(receipt)` and **remembered** for as long as the `Payments` lives.
- A refusal gives `Err(Declined(reason))` with the bank's string. Nothing is remembered.

**A key with a remembered receipt:**

- the same amount returns that receipt, without calling `send`;
- a different amount gives `Err(KeyReused)`.

**A key whose payment is on its way** (an earlier `charge` with it has not finished):

- the same amount gives `Err(InProgress)` **at once**. It does not wait;
- a different amount gives `Err(KeyReused)`.

In both cases `send` is **never called**, not even to build its future.

**A payment that does not succeed frees its key.** After a refusal, a later `charge` with that key counts as new, with any amount. The same holds when the `charge` future is dropped before it finishes (the client gave up), and when `send` or its future panics.

However many calls arrive at once, at most one `send` per key is on its way. Different keys never wait for each other. `charge` takes `&self` and is called from many tasks. Its future must be `Send` whenever `send` and its future are, so that it can be spawned.
