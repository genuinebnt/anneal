A flight recorder adds one reading at a time to a file. Last month its recovery code found one bad reading near the start and "cleaned up" by cutting the file there. Eleven hours of good readings went with it.

The file is a row of **slots** of 16 bytes:

```
value   u64, little-endian
tag     4 bytes, any value
check   u32, little-endian: checksum of the 12 bytes before it
```

A slot is **good** when all 16 bytes are there and `check` matches. `checksum` is given. Sixteen zero bytes are not a good slot.

**What a crash can do.** Slots are written in order, at the end. A crash can damage only the end of the file:

- the last write stops early, leaving fewer than 16 bytes, or a slot only partly written;
- the file has grown but the new data never arrived, so it reads as **zeros**, for any number of bytes;
- or both: a damaged slot, followed only by zeros.

**`inspect(bytes)`** returns the values of the good slots before the first bad one, and what the `Tail` is:

- `Clean`: every byte belongs to a good slot.
- `Torn { at }`: the first bad slot starts at byte `at`, and **every byte after that slot is zero**. Fewer than 16 bytes at the end count as a bad slot with nothing after it.
- `Damaged { at }`: anything else. A crash as described cannot explain it.

**`repair(file)`** makes a file ready for new slots.

- If it is `Clean` or `Torn`, cut it to its good slots and return how many bytes were removed.
- If it is `Damaged`, return `Err(at)` and **leave every byte as it was**.

Files can be megabytes long.
