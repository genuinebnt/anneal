`FrameReader` reads frames from a byte stream: a **2-byte big-endian length**, then that many bytes. `pump` reads frames
while also reacting to ticks from another task, using `tokio::select!`.

With a slow sender and frequent ticks, frames arrive corrupted, go missing, or the reader reports an error on a good
stream. Reading frames **without** ticks works perfectly.

Fix `FrameReader` so that `next` is **cancel-safe**: its future may be dropped at any `.await`, any number of times, and
the following calls still return every frame exactly once, in order. Keep the public signatures.

`next` returns:
- `Ok(Some(payload))` for the next whole frame (a payload may be empty);
- `Ok(None)` when the stream ends cleanly, between two frames, and again on every later call;
- an error of kind `UnexpectedEof` when the stream ends inside a frame, header included;
- any other read error as it is.

`pump` pushes `Event::Tick` for each tick and `Event::Frame` for each frame, and returns when the stream ends cleanly.
It already has the right shape.
