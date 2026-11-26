> **Spoiler before you open the editor.** The starter contains reference code for steps 1 to 4. It has nothing from steps 5 and 6. It is a stand-in until the app can carry your own code from step to step.
>
> It solves steps 1 to 4 and both limiters of series part 3, "A rate limiter on a clock you control". It also solves the main ideas of part 2, "The busiest stretch" and part 4, "Give every message a send time". Solve those first. You may replace the given code, but keep the public names.


**Capstone, step 7 of 7.** The laboratory now answers the real question. Given a day of recorded traffic, what would each policy have done, and to whom?

## What to write

`compare(lineup, traffic, span_ms)` runs every `(Policy, Scope)` of the lineup over the same traffic. It returns one `Outcome` per entry, in order. Entries do not affect each other, and each starts fresh.

`traffic` is in the order the requests arrive. A time earlier than one before it counts as that latest time. This holds **across all clients**, and the effective times are used everywhere.

`Scope::Shared` uses one limiter for all requests. `Scope::PerClient` gives each client its own. The six policies are the ones you have met, with their settings as named.

An `Outcome` holds:

- `decisions`: one per request.
- `busiest`: the most admitted requests of all clients together in any half-open stretch of `span_ms` (at least 1).
- `clients`: for **every client that sent a request**, its allowed and denied counts and its own busiest stretch.
- `differs_from_first_at`: the index of the first request this entry decided differently from entry 0. It is `None` for entry 0 and for entries that always agree with it.
- `held`: how many limiters still matter at the time of the last request, in the sense of `is_fresh` from step 4. `TokenBucket` and `LeakyMeter` do not have `is_fresh` yet.

There are up to 200,000 requests and 50,000 clients.
