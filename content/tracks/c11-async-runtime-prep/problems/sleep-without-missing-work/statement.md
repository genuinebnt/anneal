An idle worker should sleep. But "look for work, find none, go to sleep" has a gap. Work that arrives after the look and before the sleep wakes nobody, because nobody is asleep yet. With a lock-free queue you can't close that gap by holding the queue's lock.

Write `Gate`. A worker uses it like this:

```rust
loop {
    if let Some(job) = queue.pop() { run(job); continue; }
    let ticket = gate.prepare();
    match queue.pop() {             // look once more
        Some(job) => { gate.cancel(ticket); run(job); }
        None => gate.sleep(ticket), // returns when there may be work
    }
}
```

and whoever adds work calls `queue.push(job)` and then `gate.notify_one()`. The gate knows nothing about the queue.

- `prepare()` returns a `Ticket`. Its holder now counts as **waiting**, whether or not it is asleep yet.
- `cancel(ticket)` stops waiting without sleeping.
- `sleep(ticket)` blocks until a notification releases this ticket, then stops waiting.
- `waiting()` is the number of tickets prepared and not yet cancelled or returned from `sleep`.

Which notifications release a ticket:

- `sleep(t)` returns **only** if `notify_one` or `notify_all` was called **after** `prepare` returned `t`. So a notification made when nobody was waiting releases nothing later. A spurious wake-up of a thread must not make it return.
- A notification made after `prepare` counts even if it comes **before** `sleep` is called: that `sleep` returns at once.
- `notify_all` releases every ticket that is waiting when it is called.
- After `k` calls of `notify_one`, made while `n` tickets were already waiting, at least `min(k, n)` of those tickets are released. Releasing **more** than necessary is allowed.
- A notification is never used up by a ticket that is cancelled. If two are waiting, one `notify_one` arrives and one of them cancels, the **other** is released.

**The given lock and signal.** Use `monitor::Lock` and `monitor::Signal` from the starter where you would use `Mutex` and `Condvar`, and keep that module as it is. They behave like the real ones, except that tests can force a schedule:

- `Signal::notify_one` wakes the **newest** sleeper. A real `Condvar` may wake any one, so your gate must be right whichever it is.
- A test can run code on your thread **the moment it releases a `Lock`**. One test uses this to let a newer ticket go to sleep in the middle of your `notify_one`. The newcomer runs on its own thread. A lock you still hold may block it. The test then waits two seconds and retries after your next release. So you may hold one `Lock` while releasing another.
- A test can make your next wait **wake up for no reason**. Otherwise the given `Signal` never does that, so only that test checks the rule about spurious wake-ups.

A gate that sleeps some other way fails those tests. Sleeping threads must block, not spin. The tests can't measure that.
