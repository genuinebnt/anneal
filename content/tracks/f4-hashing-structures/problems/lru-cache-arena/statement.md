Write `LruCache<K, V>`, a cache that holds at most `capacity` entries and evicts the **least recently used** one.

- `new(capacity)`. A capacity of 0 is allowed.
- `get(&key)` returns the value and **marks the entry as most recently used**.
- `peek(&key)` returns the value and changes nothing.
- `put(key, value)` returns what happened:
  - `Put::Inserted`: a new entry, with room to spare;
  - `Put::Replaced(old)`: the key was present. Its value is replaced and it becomes the most recently used;
  - `Put::Evicted(k, v)`: a new key into a full cache: the least recently used entry was removed to make room. With
    capacity 0, the pair passed in comes straight back this way.
- `remove(&key)` takes an entry out and returns its value.
- `len()`, `is_empty()` (given), and `keys_by_recency()`, the keys from most to least recently used.

Requirements:

- `get`, `peek`, `put` and `remove` take **expected O(1)** time. The tests run 400 000 operations on 100 000 entries.
- No `unsafe`, and no crates.
- A value that leaves the cache is **dropped or returned at once**: the cache keeps no hidden copy.

`K` is `Hash + Eq + Clone`.
