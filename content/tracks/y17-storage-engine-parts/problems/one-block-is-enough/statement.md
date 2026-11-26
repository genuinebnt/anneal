A lookup service keeps millions of sorted records on disk. Reading them is slow, so records are grouped into blocks. Only the **first key** of every block stays in memory. Today each lookup reads many blocks.

**`pack(records, max_bytes)`** cuts the records, in order, into blocks. A record's size is its key length plus its value length. A block takes records while their total size stays at most `max_bytes`. A record bigger than that gets a block of its own. No block is empty.

**`get(blocks, key)`** returns the value stored for `key`. It calls `fetch` **at most once**, and not at all when the first keys already show that the key is absent.

**`range(blocks, from, to)`** returns the records with `from <= key < to`. It fetches, in order and once each, exactly the blocks which **could** hold such a key, judging only by the first keys. An empty range (`from >= to`) fetches nothing.

`first_key` is cheap but counted. With 10 000 blocks, `get` may call it at most 32 times, and a `range` of ten records at most 48 times. Keys are compared byte by byte, as `Ord` on `[u8]` does.
