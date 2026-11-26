Your service sends messages to a partner who asks for them **one at a time**. Each message takes up the line for some milliseconds, given as its cost. Dropping messages is wasteful, so you give each one a time to be sent. But nobody should wait too long.

All times are `u64` milliseconds, passed to you. No clock is read, so `std::time` must not appear in your code. A time earlier than one already passed to any method counts as that latest time.

## What to write

`Pacer::new(max_wait_ms)` makes an idle line.

`book(now_ms, cost_ms)` plans one message. It would start at the earliest moment that is not before `now_ms`, not before the end of every message already accepted, and not before the end of any pause.

- If that start is at most `max_wait_ms` after `now_ms`, accept it: return `Ok(start)`. The line is then busy until `start + cost_ms`.
- Otherwise refuse with `Err(d)` and change nothing. `d` is the **smallest** number such that the same booking at `now_ms + d` is accepted, if nothing else happens in between.
- If the start would not fit in a `u64`, return `Err(u64::MAX)`.

`pause_until(now_ms, until_ms)` is for when the partner says "not before". No later booking may start before `until_ms`. Times already given out stay as they were.

`backlog_ms(now_ms)` is how long a message booked now would wait, or `u64::MAX` if that does not fit.

A pacer is booked millions of times a second, so its memory must not grow with the number of messages.
