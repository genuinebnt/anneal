A file manager shows a tree of folders. People make folders, delete empty ones and drag them elsewhere, with everything inside. Other parts of the program remember a folder by its **number**, which must keep working after a move.

## `Tree`

It starts with only the top folder, number `TOP` = 0.

- `make(path)` makes one folder and returns its new number. Numbers are never used twice.
- `find(path)` is the number of the folder at `path`.
- `list(path)` is the names inside it, sorted.
- `path_of(id)` is the folder's current path, such as `/a/b`, and `/` for the top. A removed or unknown number gives `None`.
- `remove(path)` removes an empty folder.
- `rename(from, to)` gives the folder at `from` the new **full** path `to`. Everything inside goes with it.

## Paths

A path starts with `/`, else it is `BadPath`. Empty parts are ignored.

Parts are followed one at a time. `.` stays, and `..` goes to the parent (the top's parent is the top). A name goes into that folder, or fails with `NotFound`. So `/nope/..` is `NotFound`.

For `make`, `remove` and both paths of `rename`, the **last part** must be a real name. No last part (the top), `.` or `..` is `BadPath`. This is checked before the way to it is followed.

## Errors

- `make`: the way must exist, and the name be free (`Exists`).
- `remove`: the folder must exist, and be empty (`NotEmpty`).
- `rename` checks in this order:
  1. `from` is a good path to an existing folder.
  2. `to` is a good path whose parent exists.
  3. That parent is **not the moved folder or inside it** (`IntoItself`).
  4. If `to` is where the folder already is, return `Ok` and change nothing.
  5. Otherwise the name must be free (`Exists`).

A failed call changes nothing. A chain may be 1500 folders deep. The tests run in a debug build.
