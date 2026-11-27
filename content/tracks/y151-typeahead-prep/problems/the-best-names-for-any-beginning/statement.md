A game shows the best players whose name begins with what you type. It must answer on every keystroke, even for an empty text, when **every** player fits.

Many game servers share the answers, so everything is kept in a **remote store** of lists. Your functions keep nothing between calls. All they know is what the store holds.

**The store.** It is given as the trait `Store`:

- `load(key)` returns the list saved under `key`. A key never saved gives an empty list.
- `save(key, list)` replaces the list under `key`.

A key is any string you like. A list holds pairs `(name, score)`. Each `load` and `save` sends the **whole list** over the network.

The store refuses (the test fails) a saved list that breaks one of these:

- it holds at most `k` pairs;
- no name appears in it twice;
- each pair is a name that has been raised, with a score it has been raised with.

**Your functions.** `k` is the same in every call on one store. It is any `usize`, 0 and `usize::MAX` included.

- `raise(store, k, changes)` takes a list of `(name, score)` pairs and applies them **in order**. A new name joins with that score. For a known name, the score counts only when it is **greater** than the one it has. One list may name a player several times.
- `best(store, k, beginning)` returns at most `k` names that start with `beginning`, highest score first. Equal scores are listed by name, smallest first.

Names are any strings, the empty one included. "Starts with" is `str::starts_with`. Scores **never go down**. Your design may rely on that.

Keep nothing in `static` or thread-local values. This is a rule of the exercise, but the tests **cannot reliably enforce it**. A solution that breaks it may pass some runs, so check your code yourself.

**The budget.** Some tests count the calls on the store, with `k = 10`, names of at most 12 characters, and up to 5 000 players. Let `L` be the number of characters of a changed name.

| What is counted | Most allowed |
|---|---|
| `load` calls in one `raise` | the sum over its pairs of `4 * (L + 1)` |
| `save` calls in one `raise` | the same sum |
| `load` calls in one `best` | 8 |
| `save` calls in one `best` | 0 |

The test stops as soon as a limit is passed. Since a list holds at most 10 pairs here, one `best` can see at most 80 of them. Memory, the size of keys and time are not graded.
