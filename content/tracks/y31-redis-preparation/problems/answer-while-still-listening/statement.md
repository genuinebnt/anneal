A server connection reads commands from a client, hands them to a worker, and writes back the answers. The server also has news to send at any moment. The hard part is what happens **while** the worker is busy: clients leave, flood, stop reading, or must be cut off.

Write `serve(stream, work, notices, kill, max_line)`. It serves one connection and returns why it ended.

Lines end with `\n`, which is not part of the line. The tests send UTF-8; use `String::from_utf8_lossy`. `max_line` is at least 1.

**Batches.** Every complete line that has arrived goes to the worker in **one** `Batch`, in order. At most **one batch** is outstanding. When its replies come, write each followed by `\n`.

A batch of lines may get any number of replies. Lines that arrived meanwhile form the next batch. Send a batch as soon as you can; never wait for more input first.

**While a batch is outstanding:**

- Keep reading, so that a client that leaves is noticed **at once**. Then return `ClientLeft`, without waiting for the worker.
- But do not buffer without end: once you hold more than `max_line` bytes of unsent lines, stop reading until the replies are written. (You may hold a few kilobytes more, from the last read.)

**Notices.** Each string from `notices` is written followed by `\n`, as soon as no batch is outstanding. A batch's replies are written together, with no notice between them.

A notice that comes during a batch goes after its replies. When the notice channel closes, carry on without it.

**Long lines.** A line longer than `max_line` bytes ends the connection, **whether its `\n` has arrived or not**. First deliver the replies to the complete lines before it. Then write `ERR too long\n` and return `TooLong`.

**Ending.**

| Return | When |
|---|---|
| `ClientLeft` | a read returns end of stream (an unfinished line is dropped) |
| `Killed` | `kill` is cancelled: return **at once**, whatever you are waiting for, even a write to a client that is not reading |
| `WorkerGone` | the `work` channel is closed, or a batch's reply sender is dropped |
| `Io(kind)` | a read or write fails |

Waiting for input must not lose bytes when something else happens first. The tests use in-memory streams on a runtime whose clock is paused, so no part of your answer may depend on timeouts.
