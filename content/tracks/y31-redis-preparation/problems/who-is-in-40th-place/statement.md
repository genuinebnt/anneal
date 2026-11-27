A game keeps a leaderboard that changes all the time. Players ask: what is my place? Who is in 40th place? How many have fewer than 1000 points?

Write `Board`. Each player has a name and an `i64` score. The order is by score, **lowest first**, and players with equal scores are ordered by name, as bytes.

- `set(name, score)` adds the player or changes their score, and returns the old score.
- `remove(name)` removes the player and returns their score.
- `score(name)`, `len()`, `is_empty()`.
- `rank(name)` is the player's place, counting from 0.
- `count_below(score)` is how many players have a **strictly lower** score.
- `range(start, count)` lists up to `count` players as `(name, score)`, from place `start` on.

**Speed.** The board may hold 200000 players, and all of these calls are made constantly. A call that walks or shifts the whole board is too slow. `set`, `remove`, `rank` and `count_below` should take about `log n` steps, and `range` about `log n + count`.

Use the standard library only, and no `unsafe`.
