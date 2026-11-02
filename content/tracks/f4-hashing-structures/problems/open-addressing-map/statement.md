Implement `OaMap<K, V, S = RandomState>`: a hash map stored in one `Vec` of slots (`Empty`, `Tombstone`, `Full(K, V)`), power-of-two capacity
starting at 8, **linear probing**: start at `hash & (capacity - 1)` and step to the next slot, wrapping.

`get` probes until it finds the key or an `Empty` slot (a `Tombstone` does not stop it). `remove` leaves a `Tombstone`. `insert` first searches the whole
chain for the key (replacing its value), and otherwise uses the first tombstone it passed, or the empty slot that ended the chain. Before inserting, if
`(live + tombstones + 1) / capacity` would exceed 7/8, rebuild: **double** the table if it is more than half full of live entries, else rebuild at the same size
(this drops the tombstones).

The hasher is generic (`S: BuildHasher`) so the tests can force every key to collide.
