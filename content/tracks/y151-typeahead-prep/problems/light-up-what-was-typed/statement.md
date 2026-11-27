A shop's search box shows names such as `Crème Brûlée`. A visitor types `creme br`. The name should be offered, and the part they typed should be lit up: `Crème Br`.

**The search key.** Two texts are matched by their keys. `search_key(text)` builds one, going through the text one character at a time.

- A **mark** is any character from `U+0300` to `U+036F`. Marks are dropped. Some keyboards send `é` as `e` followed by a mark.
- A run of whitespace becomes one space. The key has no space at its start or end.
- Any other control character (`char::is_control`) is dropped. It does not end a run of whitespace.
- Every other character is lowercased with `char::to_lowercase`, which may give several characters. Each of those that is a mark is dropped. Each other one is replaced as the given function `plain` says, or kept when `plain` returns `None`.

So `"  CAFÉ\t Crème "` has the key `"cafe creme"`, and `Straße` has `"strasse"`. A key is its own key: `search_key(&search_key(t))` equals `search_key(t)` for every `t`.

**What to light up.** `typed_end(shown, typed)` answers for a name `shown` and the text `typed`.

- It returns `None` when the key of `shown` does not start with the key of `typed`.
- If the key of `typed` is empty, it returns `Some(0)`.
- Otherwise find the **shortest** start of `shown`, cut between two characters, whose key starts with the key of `typed`. Then add every mark that comes straight after it. Return the length of that start in **bytes**.

The caller slices `shown[..end]`, so `end` must be a character boundary. A character that gives two letters of the key is lit up whole: typing `s` lights up all of `ß`.

Names are short, at most a few hundred characters. Speed is not graded.
