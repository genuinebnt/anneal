An account has ten years of entries. Adding them all up each time it is opened is too slow. So now and then the program saves a **bookmark**: the total so far, to start from next time.

Then the machine crashed and lost the last entries, and new ones were added. The bookmark still described the old ones, and the program believed it.

The log is a slice of `Entry`. Every entry has an `id` that no other entry ever gets, even one written after a crash. Entries are only added at the end. A crash can remove entries from the end.

The **state** is whatever you get by calling `apply` on each entry in order, starting from `S::default()`. `rules` is a number that changes whenever `apply` changes.

You decide what a `Bookmark<S>` holds.

- **`mark(log, rules, state)`** makes a bookmark. The `state` is the state of that whole log under those rules.
- **`load(log, rules, bookmark, apply)`** returns the state of `log` under `rules`.
  - A bookmark is **good** when it was made with the same `rules`, and the entries it was made from are still the first entries of `log`. By the rules above, that is so when its last entry is still in the same place.
  - With a good bookmark that covers `n` entries, call `apply` exactly `len - n` times, on the entries after it.
  - Without one, call `apply` once on every entry, from `S::default()`.
- **`due(before, after, every)`** says whether to save a new bookmark, after the log grew from `before` to `after` entries. It is true when the length reached or passed a multiple of `every` that it had not reached before. `every == 0` means never. `before <= after` always.

A bookmark made from an empty log is allowed. It covers no entries.
