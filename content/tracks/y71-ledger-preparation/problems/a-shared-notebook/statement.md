The night shift and the day shift share one notebook. Each reads a page, thinks, and then writes. Sometimes both write what only one of them should have: the second never saw the first's line.

Write `Journal<T>`. It holds **streams** of records, and it only ever grows.

Every record has:

- `pos`: its place among **all** records, counted from 0, with no gaps;
- `stream`: the stream's name;
- `version`: its place **within its stream**, counted from 1, with no gaps;
- `item`: what was written.

A stream's **version** is the version of its newest record, or 0 when it has none.

**`append(stream, expected, items)`** adds the items to the stream, in order. It works only if the stream's version is exactly `expected`. Otherwise it returns `Conflict { expected, actual }` and changes **nothing**. On success it returns the positions of the new records as a range.

An empty batch is checked the same way. It adds nothing, and returns an empty range that starts at the next free position.

**Reading:**

- `version(stream)` is the stream's version.
- `read_stream(stream, after)` yields that stream's records with a version **above** `after`, oldest first.
- `read_all(from)` is every record with `pos >= from`.
- `streams()` is the names of the streams that have records, sorted.

Any number may be passed, however large: reading past the end gives nothing.

There can be hundreds of thousands of records in as many streams. Reading one stream must cost about the number of records it returns, not the size of the journal.
