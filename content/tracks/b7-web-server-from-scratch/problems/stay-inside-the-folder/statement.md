Your service lets clients download files from one folder. The client chooses the name. Everything outside that folder (configuration, keys, other users' files) must stay unreachable, whatever the name looks like and whatever the folder contains.

`resolve(root, raw)` turns the raw, still percent-encoded part of an address into a file to serve.

1. Decode `raw` **once**: `%XY`, with two hex digits in either case, is the byte `XY`. A `%` not followed by two hex digits, or a result that is not UTF-8, is `Err(Denied::BadRequest)`.
2. A decoded NUL or backslash is `Err(Denied::Forbidden)`.
3. Split at `/`. Empty segments and `.` are ignored. Any `..` segment is `Err(Denied::Forbidden)`, even one that would stay inside.
4. The file is the one at those segments under `root`. If, after following links, it lies outside the folder that `root` itself resolves to, the answer is `Err(Denied::Forbidden)`. A link that leads to something inside is fine, and so is a `root` that is itself a link.
5. If nothing is there, or what is there is not a regular file, or `root` does not exist: `Err(Denied::NotFound)`.

On success return the file's real location (with all links followed).

The folder only changes while no call is running.
