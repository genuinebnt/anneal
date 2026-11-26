A log collector reads newline-delimited records from clients it does not trust. One client sends a gigabyte with no
newline, and the collector runs out of memory.

Write `BoundedLines::new(reader, max_len)`, an iterator over the lines of a `BufRead` that **never holds more than
about `max_len` bytes** of a line.

- Lines are separated by `\n`. One `\r` just before the `\n` is removed. A final line without a newline still counts
  (its trailing `\r`, if any, is part of it). Input ending in `\n` has no extra empty line after it.
- A line whose content (after removing the `\r\n` or `\n`) is longer than `max_len` bytes yields
  `Err(LineError::TooLong { line })`. The rest of that line is skipped and iteration continues with the next one.
- A line that is not valid UTF-8 yields `Err(LineError::NotUtf8 { line })`, and iteration continues. A line that is
  both too long and invalid is `TooLong`.
- `line` numbers start at 1 and count every line.
- A read error of kind `Interrupted` is retried. Any other error yields `Err(LineError::Io(kind))` once, and the
  iterator then ends. A partial line before the error is discarded.
- After `None`, it keeps returning `None`.

**Memory budget** (counted by the tests): reading a 4 MB line with `max_len = 100` may allocate at most **2048 bytes**
in total.
