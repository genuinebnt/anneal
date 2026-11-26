**Capstone, step 1 of 7.** Over seven steps you build a key-value store that keeps its data in files and recovers after a crash. It supports put, delete, get and range scans. This step defines the bytes of its log.

**`crc32`** is the common CRC-32: reflected polynomial `0xEDB88320`, start with all ones, invert at the end. `crc32(b"123456789")` is `0xCBF43926`.

**A record** is one put or one delete. All numbers are little-endian:

```
tag        u8, 1 put, 0 delete
key_len    u32
key
value_len  u32
value
```

A delete has a value length of 0. Any other tag, or a delete with a value, is invalid. `decode_record` reads the record at `*pos` and moves `pos` past it. On any error it returns `KvError::Corrupt` and leaves `pos` alone. No input may make it panic.

**A frame** wraps one record for the log: `len` (u32), `crc` (u32), then the record, where `len` and `crc` describe the record's bytes.

**`replay(log)`** reads frames from the start and returns their records, with the number of bytes they take. A crash can leave a damaged end, so the log **ends** without error at the first of these:

- fewer than 8 bytes left, or fewer than `len` after the header;
- a `len` of 0 (no record is empty, and some crashes leave a tail of zeros);
- a checksum that does not match.

A frame whose checksum **does** match holds the bytes that were written. If those are not exactly one record, return `Corrupt`: that is a bug or real damage, not a torn write.
