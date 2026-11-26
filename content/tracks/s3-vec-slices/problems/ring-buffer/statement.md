`Ring<T>` holds up to `capacity` items. `push` adds to the newest end and, when full, evicts
and returns the oldest. `iter` yields oldest to newest.

A value stays in the slot it was written to until it's evicted: `push` writes one slot and moves
no other value. The tests check this through the addresses of the references `iter` yields.
