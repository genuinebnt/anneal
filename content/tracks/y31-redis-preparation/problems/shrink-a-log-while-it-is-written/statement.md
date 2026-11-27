A counting service logs every change as a line, such as `visits 1`, and rebuilds its totals from the log at start. The log grows without end, so now and then it is **rewritten** as one line per name. Writing that may take seconds, and the service must keep counting meanwhile.

The file is `LOG`. Each line is `name delta` ending in `\n`.

Names have no spaces or line ends. Deltas are `i64` as Rust writes them. Totals wrap on overflow.

- **`open(disk)`** rebuilds the totals. A missing log is empty. A last line without its `\n` was never finished: cut it off with `truncate`. A whole line that is not a name, one space and a number is an error of kind `InvalidData`, and the log is not changed.
- **`add(name, delta)`** appends the line and syncs. Only then does it change the total, and it returns the new one. (The tests never make a disk call fail during `add`.)
- **`get(name)`** is the total, 0 if never added.

**The rewrite** comes in two halves, so the compact form can be written elsewhere while `add` goes on.

- **`start_rewrite()`** returns the compact log of the totals now: one line `name total\n` for every name ever added to (totals of 0 included), in name order. It returns `None` while a rewrite is running.
- While it runs, `add` works as usual, on the old log.
- **`finish_rewrite(compact)`** replaces the log with `compact` (exactly what `start_rewrite` returned) followed by every change made **since the start, each once**. With no rewrite running it fails with `InvalidInput`.
- **`cancel_rewrite()`** forgets a running rewrite, if any.

**Crashes and failures.** The disk's promises are in the `Disk` trait: appended bytes survive a power cut only after `sync`, and a rename is atomic and lasting.

- The power may fail at any call during `finish_rewrite`. A restart must find every total as it was: nothing lost, nothing counted twice.
- If a call fails, return an error. The service keeps working on whichever log is in place, and a later rewrite must work.
- If it returns `Ok`, the new log is safe.
- However `finish_rewrite` ends, that rewrite is over.
- You may use one other name, `TEMP`. A crash may have left a file of that name behind, holding anything.
