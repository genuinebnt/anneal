Build the durability core of an LSM-style store (the shape of Bitcask, RocksDB's WAL, and Redis AOF). `Store` keeps a
`BTreeMap` in memory and an append-only **log** (a `Vec<u8>` standing for the file). Every `put` and `delete` appends a record
to the log first, then updates the map.

A record is `[crc: u32][klen: u32][vlen: u32][key][value]` (little-endian). `crc` is the provided `checksum` of everything
after the crc field. A delete is a record with `vlen == TOMBSTONE` (`u32::MAX`) and **no value bytes**; an empty value
(`vlen == 0`) is a different thing.

`Store::recover(log)` replays a log: it applies records in order and **stops** at the first one that is incomplete (the log ends
before its bytes do) or whose checksum does not match, ignoring that record and everything after it. It returns the rebuilt
store and the number of valid bytes; the store's own log is exactly that valid prefix, so new writes append cleanly after a
torn tail. Lengths in a damaged record can be anything (up to 4 GB): never trust them for an allocation or an index.

`compact` rewrites the log as one put record per live key in key order (no tombstones, no overwritten values).
