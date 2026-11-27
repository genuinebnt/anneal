**Capstone, step 2 of 8.** The bank's journal: where every event is kept, for ever. The starter holds working code for step 1.

**Decoding.** A `Stored` line has a `schema` and a JSON `payload`. Today's schema is 2, and its payload is the `Event` as serde writes it: `{"type": .., "data": {..}}`.

Schema 1 differed in two events only:

- `Deposited` had `cents` instead of `amount`;
- `Withdrawn` had `amt` instead of `amount`, and **no reason**. It reads as `"legacy"`.

Each version knows only its own fields. A field that only another version defines does not count. Other extra fields do not matter.

`decode` returns `UnknownSchema` for any schema but 1 and 2. A payload that is not a valid event of its schema is `BadPayload`. Decoding never changes the stored line.

**`MemStore`** implements `EventStore`. A line has its place `pos` in the whole log, from 0, and its `version` in its stream, from 1. Neither has gaps.

- `prepare(stream, expected, events)` first returns `Conflict` unless the stream is at version `expected`. Then every event must decode, or it returns `BadEvent`. On success it returns the lines with their places. It changes nothing.
- `admit(batch)` takes lines that already have places. They must continue the log: the next positions, and each stream's next versions. A batch may mix streams.
- `admit` returns `Empty`, or `OutOfSequence` or `Unreadable` for the **first** wrong line, checking its places before its payload. On any error, nothing is added.
- `append` is both. An empty list is checked like any other and adds nothing.
- `read_stream(stream, after)` gives the lines with a version above `after`. `read_all(from)` gives the lines from position `from`. Any number is allowed.
- `streams()` lists the streams with lines, sorted.
