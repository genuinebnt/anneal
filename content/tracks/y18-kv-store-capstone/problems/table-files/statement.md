**Capstone, step 3 of 7.** When enough has piled up in memory, the store writes it to a file that never changes again. The starter holds the reference code of steps 1 and 2, and the key filter, with `new`, `may_contain`, `save` and `load`.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 4, *A key-value store with a write-ahead log that survives torn writes*: the torn-end rule, in the earlier code
- Part 5, *An append that fails half-way*: the failure handling, in the step 2 log
- Part 12, *Fix: a saved filter that forgets its keys*: the solution, in the filter

**The file** of table `id` is `sst_<id>.sst`, with the id padded with zeros to at least six digits. Its parts, in order (numbers little-endian):

```
data    each entry as a record
        (step 1), in key order
index   count u32
        count times:
          key_len u32
          key
          offset  u64
filter  the saved filter
        of all keys
footer  data_end     u64
        filter_start u64
        record_count u64
        crc          u32
        magic        u32
```

`magic` is `0x53535431`.

The index has an entry for records 0, 16, 32 and so on: the key, and the offset where that record starts. `crc` is the `crc32` of every byte before it.

**`write`** builds the file under the temporary name `sst_<id>.tmp` (`.tmp` in place of `.sst`, so `sst_000001.tmp`), replacing any leftover of that name. It then syncs the file, renames it, and syncs the directory. A table is under its final name only when it is complete.

**`open`** returns `Corrupt` for a file that is too short, has a wrong magic or checksum, or does not make sense. A good checksum is not enough: no offset, length or count may lead to a panic later. Offsets in the index start at 0, increase, and lie in the data. Its keys increase. The index fills its part exactly, and is empty exactly when the data is.

**Memory and reads.** After `open`, a table keeps the index and the filter, not the records. `get` reads **at most one block** (the records from one index offset to the next) from the file. It reads nothing when the index or the filter already shows that the key is absent.
