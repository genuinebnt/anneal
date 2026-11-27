A courier firm has years of delivery times for every route. The dispatch screen asks things like "how many deliveries on this route took an hour or less?". Keeping every time is too much. You may keep only a few numbers per route, and the screen accepts a good guess.

`summarise(times, limit)` builds a `Summary`. It holds `total`, the number of times, and `kept`: at most `limit` numbers. What those numbers mean is your choice.

`limit` is at least 2. The times come in no particular order, may repeat, and may be any `i64`.

`at_most(x)` guesses how many times were `x` or less.

**What is promised**

- If `total` is at most `limit`, every guess is exactly right.
- Otherwise a guess is off by at most `total / (limit - 1)`, rounded **up**.
- Below the smallest time, and at or above the largest, the guess is exactly right.
- A larger `x` never gets a smaller guess.

The tests call `at_most` on a summary rebuilt from `kept` and `total` alone. So nothing else can be remembered, and you cannot add fields.

Times crowd together in real data. Many routes have most deliveries within a few minutes of each other, and a few that took weeks.
