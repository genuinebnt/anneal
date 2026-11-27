A small theatre sells seats from two windows. Both use one shared **register**. Customers also hand seats back, at any moment.

The network is poor. Any call to the register can come back as `Lost`: the reply never arrived. For `read` and `version` that is only a nuisance. For `add`, you cannot know whether the sale was written.

The register:

- `read()` returns a `View`: its `version`, the seats `left`, and every sale it holds (`id` to seats).
- `version()` returns the current version. It changes whenever anything in the register changes.
- `add(expected, sale)` writes the sale, **only if** the version is still `expected`. If not, it writes nothing and returns `Moved`.

Write `sell(register, sale, attempts)`. `sale.id` was chosen by the customer's phone. The phone may send the same sale again after an error, for as long as it likes.

**An attempt** starts with one `read()`. Then, in this order:

1. If the view already holds `sale.id` with the same number of seats, return `Ok(Sold::Already)`. If it holds that id with another number, return `IdReused`. Sales are never removed.
2. If the view has fewer seats left than the sale needs, the answer is `NotEnough { left }`. Give it **only if** `version()` still returns the view's version. `left` is the view's number.
3. Otherwise call `add` with the view's version. If it succeeds, return `Ok(Sold::Now)`.

Anything else ends the attempt: a `Lost` from any call, a version that has changed, or `Moved`. The next attempt starts with a new `read()`.

Make at most `attempts` attempts, then return `GaveUp`. With 0 attempts, make no call.

Whatever happens, the same sale must never be in the register twice. A sale always asks for at least one seat.
