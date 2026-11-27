**Capstone, step 7 of 10.** Clients can now talk to each other: some listen on named channels, and others publish to them. The starter holds the reference code of steps 1 to 6.

**Spoiler warning.** The starter gives away steps 1 to 6 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes
- *Throw out what has gone stale*: `Deadlines` solves it
- *Who is in 40th place?*: `Ranked` solves its core, with `f64` scores
- *Which names fit the pattern?*: `glob_matches` solves it
- *Hold requests until something arrives*: `Waiting` solves most of it

**Subscriptions.** A client may subscribe to **channels** by name and to **patterns**, which are matched with `glob_matches`. Names are any bytes. Its **count** is the number of channels and patterns it is subscribed to.

**`SUBSCRIBE channel [channel ...]`** replies once per argument, in order: `["subscribe", channel, count]`, with the count after that one (an `Int`; the others are `Bulk`). Subscribing again changes nothing but is confirmed again.

**`UNSUBSCRIBE [channel ...]`** replies `["unsubscribe", channel, count]` for each argument, subscribed or not.

- With no arguments it leaves every channel, with one reply each, in byte order.
- With no arguments and no channels, it replies once: `["unsubscribe", Null, count]`.

**`PSUBSCRIBE`** and **`PUNSUBSCRIBE`** do the same for patterns, with the words `psubscribe` and `punsubscribe`.

**`PUBLISH channel message`** sends to every listener, and replies with the number of message frames sent.

- A subscriber of the channel gets `["message", channel, message]`.
- For **each** matching pattern it subscribed to, a client gets `["pmessage", pattern, channel, message]`. It may so receive one message several times.
- To one client: the `message` frame first, then the `pmessage` frames in byte order of their patterns.

These message frames have `published` set. Every other delivery does not. Each client receives its deliveries in the order the commands ran.

**Subscriber mode.** While a client's count is above 0, it may only send the four subscription commands, `PING` and `QUIT`. Anything else, even with the wrong number of arguments, replies:

`ERR Can't execute 'name': only (P)SUBSCRIBE / (P)UNSUBSCRIBE / PING / QUIT are allowed in this context`

with the name in lower case. `PING [message]` then replies the array `["pong", message]`, with an empty string if there is none.

**With the earlier steps:**

- `PUBLISH` may be queued, and is sent at `EXEC`.
- Any of the four subscription commands inside `MULTI` replies `ERR NAME is not allowed in transactions`, in upper case, and spoils the transaction.
- Held commands of a woken client may publish and subscribe.
- Leaving, by `disconnect` or `QUIT`, ends all subscriptions.

`SUBSCRIBE`, `PSUBSCRIBE` need at least one name; `PUBLISH` takes exactly two arguments. Tens of thousands of clients may be subscribed: `PUBLISH` must not look at clients that do not listen to that channel. It may try every pattern.
