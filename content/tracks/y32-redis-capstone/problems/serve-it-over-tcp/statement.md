**Capstone, step 10 of 10.** Everything so far is a program that takes commands and returns replies. Now put it on the network. The starter holds the reference code of steps 1 to 9.

**Spoiler warning.** The starter gives away steps 1 to 9 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it
- *Hold requests until something arrives*: `Waiting` solves most of it
- *Come back the same after a restart*: step 8's records use its method
- *A key-value store with a write-ahead log that survives torn writes*: step 8 shows how a torn end is cut off
- *Capstone 1: records and frames* (key-value store series): the given `crc32` is its checksum, and later starters show how to decode with bounds checks
- *Shrink a log while it is still being written*: step 9's rewrite uses its method
- *Save a file that a crash cannot tear*: step 9 publishes files by its method

Write `start(listener, core, log, config)`. It serves the core to every client that connects, over Tokio, and returns at once. `Server::addr()` is the listening address. The tests use real sockets on this machine and a runtime with several threads.

**Time** comes only from `config.clock`. The server calls `core.request` with it.

It also calls `core.tick` and `core.cycle` **on its own**, every few milliseconds. So blocked pops time out and expired keys are freed without any client asking. `cycle` needs dice; any source will do.

**Each connection** is one client of the core.

- Parse the input with `parse_command` and `config.limits`. A client may send many commands at once, or one a byte at a time. Commands run in order. Empty commands get no reply.
- Send each delivery's frame, encoded, to its client, in order. After a delivery with `close`, end the connection; later input is not run.
- On a protocol error: send the replies to the commands before it, then an error starting `ERR Protocol error`, and end the connection. The error must arrive even if more input (60 KB or so) is on its way, and be followed by a normal end of stream.
- When a client ends its side or disconnects, the commands it sent before that still run. Then it is disconnected from the core, and its connection is closed. Replies still unsent may be lost.

**Slow readers.** Replies may queue freely. Published messages may not.

Suppose **`config.push_buffer` published messages** wait unwritten for one client, and one more comes. Drop that client: disconnect it from the core and close its connection, even if a write to it is stuck.

Other clients must not wait for it. A client that reads as fast as messages come must never be dropped.

**The log.** If `log` is `Some`, a reply or message leaves only **after** the records of everything that caused it, and of everything before that, have been written with `Aof::write`. Commands that are waiting together should share one write: 1000 commands sent at once must cost fewer than 200 syncs. Timed removals are logged too.

**If a write fails**, send nothing of what it covered, and stop serving: close every connection.

**`stats()`** says how many clients are connected to the core, and how many of those are blocked. It must catch up within a moment of any change. When a client has a reply, the counts include its own connection.

**`shutdown()`** stops accepting, closes the port and every connection (blocked and idle ones too), and returns when all of the server's tasks have ended. Replies not yet sent may be lost.
