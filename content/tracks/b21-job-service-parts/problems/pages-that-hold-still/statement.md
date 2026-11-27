A game shows its leaderboard a page at a time. Scores change while a player scrolls. With "give me entries 20 to 30", one new score near the top shifts everything down, and the reader sees a row twice. A removal makes them miss one.

Write a `Board` whose pages stay correct while it changes.

**Board order** is by score, highest first. Equal scores are ordered by player id, lowest first.

- `set(player, score)` adds a player or changes their score. A player is on the board at most once.
- `remove(player)` takes a player off and says whether they were on it.
- `page(cursor, limit)` returns up to `limit` entries. `limit` is brought into `1..=100`, so 0 means 1.

**What a page holds.** With no cursor, it starts at the top. `next` is a cursor for the following page. Passing it back returns the first entries, in the board's **current** order, that come strictly after the **place** where the previous page ended. A place is the score and player id that the last entry had when that page was made.

So a cursor keeps working whatever happens to the board. This is true even if the last entry of its page was removed or got a new score. A new `limit` may be used with an old cursor.

**`next`** is `Some` exactly when, at the time of the call, at least one entry comes after the last one returned. An empty page has `next: None`.

**Cursors** are strings in any format you like. The strings `""`, `"abc"`, `"1 2"`, `"!"` and `"not-a-cursor"` are never cursors, and give `Err(BadCursor)`. The tests make no other claims about the format. Any score and any player id up to `u64::MAX` must work.

**Speed.** The board may hold hundreds of thousands of players, and a reader may walk all of it in small pages. The time for one page must not grow in step with the size of the board or with how far down the page is. `set` and `remove` must be fast too.
