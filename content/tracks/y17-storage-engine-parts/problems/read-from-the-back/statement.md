A game ships thousands of assets in one large file. The loader reads the whole file to find any one of them. It also crashes on a damaged download.

The file is a **bundle**. The data of all entries comes first. A directory follows it, and a fixed footer ends the file. All numbers are little-endian.

```
data       any bytes
directory  count     u32
           count times:
             name_len  u16
             name      UTF-8
             offset    u64
             len       u64
footer     directory_offset u64
           the bytes "BND1"
```

An entry's bytes are the `len` bytes at `offset` from the start of the file. A name or an entry may be empty. `names()` keeps the directory's order.

**`Bundle::open(reader)`** reads only what it needs: the footer and the directory, at most that many bytes in all. It returns `Corrupt` unless **all** of this holds:

- the file is long enough for a footer, and the footer ends in `BND1`;
- the directory starts at or before the footer;
- the directory holds exactly `count` entries, with no bytes left over;
- every name is UTF-8, and no name appears twice;
- every entry lies wholly in front of the directory: `offset + len` is at most the directory's offset, also when `len` is 0.

**`get(name)`** reads only that entry's bytes.

The reader may return **fewer bytes than asked** from a single `read`. An error from the reader is `Io` with its kind. The file may be hostile: no input may cause a panic, or an allocation much larger than the file.
