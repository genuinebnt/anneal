A tool keeps its data in one file, cut into pages of equal size. It reads and changes single pages and must never load the whole file.

Write `PageFile` with the standard library.

- `create(path, page_size, pages)` makes a **new** file of `pages` pages of zeros. If anything is already at `path`, it fails with `Io(AlreadyExists)` and leaves it alone.
- `open(path, page_size)` opens an existing file. Its page count is its length divided by the page size. A length that is not a multiple is `NotWholePages`. It never changes the file.
- A page size of 0 is `BadPageSize`.
- `read(n, page)` fills `page` with page `n`. `write(n, page)` replaces page `n`. Pages are numbered from 0.
- Both check that the buffer is exactly one page long (`WrongLength`), **then** that `n` is below the page count (`OutOfRange`). Neither ever makes the file longer.
- `grow(extra)` adds that many pages of zeros at the end and returns the number of the first new one (the old page count, also when `extra` is 0).
- `sync()` returns once the file's data is on the disk.

**Sizes.** A file's length must fit in an `i64`. If `create` or `grow` would pass that, or the multiplication overflows, return `OutOfRange` and **make or change nothing**. `n` may be any `u64`.

Failures of the operating system become `Io(kind)`; the `From` conversion is given.

`read` and `write` take `&self`, so several threads may share one file. Pick calls for which that is safe. One test runs eight threads on one file. It cannot force two calls to collide, so an unsafe choice may still pass it now and then.
