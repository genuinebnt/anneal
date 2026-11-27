A pharmacy keeps tens of thousands of product names. A typed name may hold a slip or two. The names live in a **remote index**. Your code never holds them.

You write both sides of that index: where each name is filed, and what to fetch for a typed name.

**Filing.** `keys(name)` returns the keys to file a name under. A key is any string you like.

Before any question, the index calls `keys` once for each name. It files the name's **number** under each key returned. A key returned twice counts once.

`keys` sees one name at a time. It is not told the number, and the names come in no particular order.

**Asking.** `near(typed, slips, list, compare)` returns every name within `slips` slips of `typed`, as pairs `(number, slips needed)`. Sort by slips needed, then by number.

- `list(key)` fetches the numbers filed under `key`, in rising order. An unused key gives an empty list.
- `compare(number)` is a careful, **expensive** comparison. It answers `Some(d)` when that name is within `slips` slips of `typed`, with `d` the number needed. Otherwise it answers `None`.

`near` gets nothing else. It never sees a name, and it learns distances only from `compare`. Both functions must depend only on their arguments, so keep nothing in `static` or thread-local values.

The tests **cannot reliably enforce** this rule. A solution that stores names there may pass some runs, so check your code yourself.

**A slip** is one character added, left out or replaced, or two neighbouring characters changing places. Each counts as 1, and no character is touched by two slips. Characters are Rust `char`s. Case matters. No name or typed text holds the character `'\0'`.

**The rules.**

- No close name may be missed, whatever the texts are. Short names, empty names, repeated letters and any `usize` for `slips` are all allowed.
- Never call `compare` twice with the same number in one `near`.

**The budget.** One test files 4 000 random names of 12 to 16 lowercase letters. It asks with `slips` of 0, 1 or 2. Each typed text has at least 12 letters and is close to one name or to none. Each such `near` has three limits:

| What is counted | Most allowed in one `near` |
|---|---|
| calls of `list` | 200 |
| numbers received from `list`, over all its calls | 1 500 |
| calls of `compare` | the number of close names, plus 500 |

A list counts in full each time you fetch it. So fetching a list of every name is far over the limit. The test stops as soon as a limit is passed.

For other catalogues and shorter texts, these counts are not graded. The size of the index, the work in `keys`, memory and time are not graded.
