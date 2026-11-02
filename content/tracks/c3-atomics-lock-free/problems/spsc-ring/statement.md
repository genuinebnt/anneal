`channel(capacity)` returns a `Producer` and a `Consumer` over a fixed ring. `Producer::push` hands the value
back (`Err(value)`) when the ring is full; `Consumer::pop` returns `None` when it is empty. Neither ever blocks
or takes a lock, and each end is used by one thread (hence `&mut self`). Exactly `capacity` items fit, and items
come out in the order they went in.

Items still inside when both ends are gone must be dropped exactly once. `is_abandoned` tells one side the other
was dropped, so a consumer can tell "empty for now" from "empty forever".
