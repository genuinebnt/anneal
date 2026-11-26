This loads a service's limits from a key-value map. The printed messages look reasonable, but the tool that runs it
chooses an **exit code** from the error, and on-call engineers get codes that make no sense: a value that merely contains
the word `missing` is reported as a missing key, and wrapping the error one more time changes the answer.

Fix it. `map_err` is not allowed.

**The chain.** `load` returns an error whose chain (`err.chain()`, outermost first, each printed with `{}`) is:

| Failure | Chain |
|---|---|
| key absent | `loading api`, `missing key `api.workers`` |
| not a number | `loading api`, `reading `api.workers``, then the `ParseIntError`'s own message |
| zero workers | `loading api`, `workers must be at least 1` |

- A missing key's root error is a `MissingKey` holding the full key.
- A bad number's root error is the `ParseIntError` itself.
- `load` reads `<service>.workers` first, then `<service>.queue`.

**`exit_code(err)`** returns `2` if **any layer** of the chain is a `MissingKey`, else `3` if any layer is a
`ParseIntError`, else `1`. It must give the same answer however many layers of context, or of other people's error types,
are wrapped around the cause. It must not look at message text.

`report(err)` joins the chain with `: ` and already works.
