Put a small text service on the network. Clients connect and send lines; for each line the service sends one line back. This is how many simple protocols work, and everything bytes do on a real connection applies to it: they arrive in any pieces, connections end at any moment, and some clients send far more than they should.

`start(max_line, handler)` listens on a free port of 127.0.0.1, serves clients on a background thread, one after another, for as long as the process lives, and returns the address.

For each client:

- A line ends at `\n`. One `\r` just before it is removed. For each line, in order, send `handler(line)` followed by `\n`. A reply to a line that has arrived is sent without waiting for more input.
- A line that is not valid UTF-8 gets `ERR not text`, and the service goes on.
- A line whose content (without the `\r\n` or `\n`) is longer than `max_line` bytes gets `ERR too long`, and the connection is closed. This must happen **as soon as it is certain**, without waiting for the line to end, and the service must not hold much more than `max_line` bytes of it. The client must be able to read the reply, followed by a normal end of stream, **even if it has already sent much more** (at least 60 KB more).
- When the client ends its side, an unfinished line is ignored, and the connection is closed.

Whatever one client does, including disconnecting at any moment, sending nothing, or a `handler` that panics on its line, the service then serves the next client. (A client that stays connected and silent may make others wait; later parts deal with that.)
