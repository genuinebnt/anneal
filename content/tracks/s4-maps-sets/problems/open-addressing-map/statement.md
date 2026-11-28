Build `OpenMap<V>`, a hash map from `u64` keys to `V`, on one `Vec<Slot<V>>` with linear probing.
Implement `new`, `insert` (returning the old value if the key existed), `get`, `remove` and `len`.
Removing must not break lookups of keys further along a probe chain.

Count tombstones as used. After every call, at least a quarter of the slots must be `Empty`, or a
lookup of a missing key could walk the whole table. Double when the table gets fuller than that, and
drop the tombstones whenever you rebuild. Under insert-then-remove churn, rebuilding at the same size
is enough: the slot count must stay at most `16 * max(largest len so far, 1)`.

`capacity()` returns the number of slots and `empty_slots()` the number of `Empty` ones. The tests
use them to check both rules.
