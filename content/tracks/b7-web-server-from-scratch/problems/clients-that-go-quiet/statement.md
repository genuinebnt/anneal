A worker that serves one connection is stuck with it. A client that connects and says nothing, or sends its request one byte at a time, can hold the worker for as long as it likes. A handful of such clients can take a whole server out of action, whether they mean to or not.

`serve(stream, now, limits)` serves one connection of a line protocol: for every complete line (ending in `\n`, which is not part of the line) it writes `ok <line>\n` and flushes, in order, without waiting for more input.

It never blocks for long: **`stream.read` returns an error of kind `WouldBlock` or `TimedOut` when nothing has arrived for a short while**, and you call it again. `Interrupted` is retried too. `now()` is the time since some fixed moment; use it, and only it, to measure time.

`serve` returns how the connection ended:

- `Ending::ClientLeft`: the client ended the stream (an unfinished line is ignored).
- `Ending::Idle`: no byte of a next line had arrived `limits.idle` after the connection started, or after the last reply was written. Nothing is written.
- `Ending::TooSlow`: a line was still unfinished `limits.line` after its first byte arrived. Write `err too slow\n`. This applies **however the bytes are arriving**.
- Any other stream error is returned as `Err`.

When a limit has passed, `serve` notices by the next time `read` has returned, whatever it returned. Never act before a limit has passed. If a read both finishes a line and is the first return after that line's limit passed, the line is answered. For a line that begins in the same read as the one before it, its first byte arrived with that read.
