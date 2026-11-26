Each data file stores a small filter, so that a lookup can skip files that surely lack the key. All tests passed in the process that wrote the files. After a restart, the service says that thousands of stored keys do not exist.

Fix `Filter`. The saved bytes are read by **other processes, later, built by other compiler versions**. So the format is fixed completely.

**Saved bytes:** one byte with the number of probes, a `u32` little-endian with the number of bit bytes, then the bit bytes. Bit `p` is bit `p % 8` (from the lowest) of byte `p / 8`.

**The hash** of a key is 64-bit FNV-1a (start `0xcbf29ce484222325`, and for each byte: XOR it in, then multiply by `0x100000001b3`, wrapping). Then XOR the result with itself shifted right by 32.

**Positions:** with `h` the hash, `step = h.rotate_left(31) | 1`, and `m` the number of bits, probe `i` is `(h + i * step) % m`, with wrapping add and multiply.

**`new(keys)`** uses 4 probes and `max(10 * keys.len(), 64)` bits, rounded up to whole bytes.

**`load(bytes)`** returns `None` unless the bytes are exactly one saved filter with at least one probe and at least one bit byte. It must never panic. A loaded filter uses the probe count **from the bytes**.

You may not use `RandomState` or `DefaultHasher`.
