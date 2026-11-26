**Capstone, step 2 of 7.** The store writes every change to a log file before it applies it. The starter holds the reference code of step 1, and the `LogFile` trait with its implementation for a real file.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 4, *A key-value store with a write-ahead log that survives torn writes*: the torn-end rule, in the step 1 code
- Part 5, *An append that fails half-way*: how to solve its real-file device

Write `Wal`, the log. It works on any `LogFile`, so tests can script failures.

**`open(path)`** creates the file if it is missing. It returns the log and the records the file holds. `with_file` does the same for a given file.

- If the end is torn, in any of the ways `replay` accepts, **cut it off the file**. Later appends must be readable.
- A `Corrupt` error from `replay`, or a failed read, is returned, and the file is not changed.

**`append(key, value)`** adds one frame with one call to the file's `append`. It does not sync.

- If the call fails, put the file back as it was, trying once, and return the error. The log stays usable.
- If that also fails, return the **append's** error. The log is now unusable.

**`sync()`** syncs the file. **`reset()`** empties it. If either call fails, return the error; the log is then unusable.

Once unusable, `append`, `sync` and `reset` return `KvError::Unusable` without touching the file. **`len()`** is the number of bytes of whole frames in the log.
