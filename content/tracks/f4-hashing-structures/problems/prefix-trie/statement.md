**Interview prompt. Time box: 60 minutes** (core in about 25). The tests are named `core_`, `part2_`, `part3_` and
`part4_`. The problem counts as solved when all parts pass.

Build the dictionary behind a search box. Words are any strings, including the empty string and non-ASCII text.
Do not use `BTreeMap` or `BTreeSet`.

## Core

`Dictionary`:

- `new()`;
- `insert(word)` returns `true` if the word was not already stored;
- `contains(word)` is `true` only for a stored word, not for a mere prefix of one;
- `remove(word)` returns `true` if the word was stored;
- `len()` is the number of stored words; `is_empty()` is given;
- `count_prefix(prefix)` is the number of stored words that start with `prefix` (every word starts with `""`).

All of these must cost in proportion to the length of the argument, **not** to the number of stored words.

## Part 2: completions

`complete(prefix, limit)` returns up to `limit` stored words that start with `prefix`, in ascending `String` order.
Its cost must depend on the prefix and on what it returns, not on how many other words match.

## Part 3: the longest stored beginning

`longest_stored_prefix(text)` returns the longest stored word that `text` starts with, as a slice **of `text`**, or
`None`.

## Part 4: no leftovers

`prefix_count()` returns the number of **distinct non-empty prefixes**, counted in chars, of all stored words. For
`{"to", "tea"}` these are `t`, `to`, `te`, `tea`: 4. After removals it must be exactly what it would be if the
removed words had never been inserted, and your structure must not keep growing under repeated insert and remove.
