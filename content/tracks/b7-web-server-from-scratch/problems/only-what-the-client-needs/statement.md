A client that downloaded a file yesterday should not have to download it again if it has not changed. A client whose download broke half way should be able to ask for the rest. Your service gets what the client said and what it knows about the file, and decides what to send.

`plan(ask, file)` returns:

- `Plan::NotModified`: the client's copy is current; send no body (status 304);
- `Plan::Part { first, last }`: send the bytes `first..=last` (status 206);
- `Plan::Unsatisfiable`: the client asked for bytes that do not exist (status 416);
- `Plan::Whole`: send everything (status 200).

**Is the copy current?** `file.tag` is a quoted string such as `"v7"`.

- If `ask.if_none_match` is present, it alone decides. It is a comma-separated list of tags, with optional spaces. It matches if any entry equals `file.tag` after removing a leading `W/`, or if an entry is `*`.
- Otherwise, the copy is current if `ask.if_modified_since` equals `file.last_modified` exactly.

If the copy is current the answer is `NotModified`, whatever else was asked.

**Which bytes?** `ask.range` counts only when `ask.method` is `GET`. The forms, where the numbers are ASCII digits and positions start at 0:

- `bytes=a-b`: positions a to b inclusive, with `a <= b`;
- `bytes=a-`: from a to the end;
- `bytes=-n`: the last n bytes.

Spaces around the whole value are allowed. A range reaching past the end is cut to the end. It is `Unsatisfiable` if no byte of the file is in it. Any other value, including several ranges, is ignored: `Whole`. Numbers may be larger than any integer type; they are still numbers, and very large: treat such a number as `u64::MAX`.

`content_range(plan, len)` is the `Content-Range` header for the answer: `bytes a-b/len` for a part, `bytes */len` for unsatisfiable, `None` otherwise.
