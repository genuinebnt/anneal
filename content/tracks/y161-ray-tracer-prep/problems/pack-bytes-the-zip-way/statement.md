Many file formats keep their data in a **zlib stream**. A writer is allowed to skip compression entirely, and every reader must still accept the result. You write such a writer, and a strict reader for it.

## The stream

1. Two header bytes.
2. One or more **blocks**.
3. The Adler-32 checksum of the **data**, 4 bytes, big endian.

A **stored block** is one byte, then a length `n` (2 bytes, **little** endian), then `!n` the same way, then `n` bytes of data. The first byte is `1` on the last block and `0` on the others. `n` is at most 65535.

**Adler-32:** start with `a = 1`, `b = 0`. For each byte, `a = (a + byte) mod 65521`, then `b = (b + a) mod 65521`. The result is `b * 65536 + a`.

## `wrap`

Writes the header `0x78 0x01`, then the data in **as few stored blocks as possible**. There is always **at least one** block, even with no data.

## `unwrap`

Returns the data only if the whole input is a sound stream of stored blocks. In every other case it returns `None`. No input may panic.

**The header** bytes `x` and `y` are sound when:

- the low 4 bits of `x` are 8, and its high 4 bits are at most 7;
- `x * 256 + y` is divisible by 31;
- bit 5 of `y` (value 32) is 0.

Each block's first byte must be exactly 0 or 1. Its two lengths must agree. Blocks may have any length from 0 to 65535. After the last block come **exactly** the 4 checksum bytes, and they must match.
