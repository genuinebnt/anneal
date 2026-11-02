A database is a file of fixed-size pages, and a page is bytes. Everything in module 1a rests on two skills: **turning numbers into bytes and back** in a layout you choose, and **reading and writing a file at a byte offset**. This stage practises both on a small scale, in `src/rust_primer/bytes.rs`.

## The task

- `Header::to_bytes` / `Header::from_bytes`: a header of 16 bytes: `magic` (u32), `version` (u16), `flags` (u16), `len` (u64), each **little-endian**, in this order. `from_bytes` gives `None` unless it gets exactly 16 bytes.
- `pack_u32s` / `unpack_u32s`: a list of numbers as 4 bytes each, and back (a last group of fewer than 4 bytes is ignored).
- `PageFile`: a file of pages of `PAGE` (256) bytes. `open` creates the file if it is missing and **keeps** its contents if it is there. `write_page(i, data)` and `read_page(i)` work at byte `i * PAGE` with `write_all_at` / `read_at` (from `std::os::unix::fs::FileExt`): no seeking, and `&self`, not `&mut self`. A page that was never written reads as **zeros**, and so does the part of a page beyond the end of the file. `page_count` counts a last partly written page.

The tests: exact bytes for a header (a wrong endianness fails at once), round trips, wrong sizes, pages written out of order, holes, a file that ends in the middle of a page, and reopening; and three properties: headers and numbers round-trip, and **a page file behaves like an array of pages that start as zeros**.

## Your freedom

The header and the packing are fixed by the format (the tests spell out the bytes). `PageFile`'s fields are yours (it will need little more than the open file); how you fill a page that the file only partly covers is up to you.

## The Rust toolbox

**Integers to bytes.** `42u32.to_le_bytes()` is `[42, 0, 0, 0]`, a `[u8; 4]`; `u32::from_le_bytes([42, 0, 0, 0])` is `42`. There are `_be_` (big-endian) and `_ne_` (native) versions. Choose an endianness on purpose: a file written on one machine should read the same on another.

**Slices to arrays.** `from_le_bytes` wants an array, and you have a slice: `bytes[0..4].try_into()` converts a slice of exactly 4 bytes (it fails, with a `Result`, for any other length).

**Copying into a range.** `out[4..6].copy_from_slice(&x.to_le_bytes())` writes bytes into a slice (the two slices must have the same length, or it panics).

**Positional I/O.** `file.read_at(&mut buf, offset)` reads up to `buf.len()` bytes and tells you how many it read: it may be **fewer** (and `0` means the end of the file). `file.write_all_at(buf, offset)` keeps writing until everything is written. Neither moves any cursor, so they take `&File`: many threads can share a file.

```rust
use std::os::unix::fs::FileExt;
let mut page = [0u8; 256];
let n = file.read_at(&mut page[done..], offset + done as u64)?;   // n may be less than the rest of the page
```

**`chunks_exact`.** `bytes.chunks_exact(4)` yields slices of exactly 4 bytes and skips a short last one.

## If this is new

- S9 I/O & filesystem: `File`, `OpenOptions`, `read_at`.
- [S3 Vec & slices](/t/s3-vec-slices): ranges, `copy_from_slice`, `chunks_exact`.

## Tests

- A header's exact bytes; round trip; sizes other than 16 are refused.
- Packing four bytes per number; a short tail ignored.
- Pages written out of order; holes; far beyond the end of the file; reopening; a last page that is partly there.
- Properties: headers and numbers round-trip; a page file is an array of pages (random writes and reads against a `Vec`).

## Hints

### Little-endian means the small end first

`0x0102_0304u32.to_le_bytes()` is `[4, 3, 2, 1]`. If your header test shows the bytes reversed, you used `to_be_bytes`.

### A short read is not the end

`read_at` may return fewer bytes than the buffer holds even when the file has more. Loop until the page is full or a read returns `0`; the rest of the page stays zero.

### Do not truncate on open

`OpenOptions::new().read(true).write(true).create(true)` keeps the contents; add `.truncate(false)` to say so out loud (clippy asks for it).

## Performance

One `read_at` is one system call, whatever the page size (unless the page is in the operating system's cache, when it is a copy out of memory). Reading a page byte by byte would be 256 calls.

**Measure it.** Write 10 000 pages, then read them back in order and in random order. Both orders cost the same number of calls; on a spinning disk the random order would be far slower, on an SSD it is a few times slower.

## Experiment

Optional. Predict first, then run.

1. **Use big-endian.** Switch the header to `_be_` bytes. Which test names the first byte that is wrong?
2. **Drop the loop.** Call `read_at` once and trust it. On your machine the tests probably still pass: why is that no proof it is right?

## Other designs

- **`Seek` + `Read`/`Write` on `&mut File`:** one cursor, so one user at a time, and two calls per page (seek, then read).
- **Memory-mapping the file:** pages become slices; faults and errors become signals (see the optional *memory-mapped files* concept).
- **A library such as `byteorder` or `zerocopy`:** shorter code; here you write the bytes yourself once, because module 1a is made of this.

## In BusTub

`disk_manager.cpp` opens the database file and does `db_io_.seekp(offset); db_io_.write(page_data, BUSTUB_PAGE_SIZE);` and the matching `seekg` and `read`, and a read beyond the end of the file fills the page with zeros. Module 1a is exactly this, with positional I/O instead of a shared cursor.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `memcpy(buf, &x, 4)` (the machine's own byte order) | `x.to_le_bytes()` (the order you chose) |
| `pread(fd, buf, n, off)`; `n` may be short | `file.read_at(buf, off)?`, also short |
| `pwrite` in a loop until all is written | `write_all_at` |
| `reinterpret_cast<Header *>(buf)` | `Header::from_bytes(&buf[..16])`, copying the fields out |

**Port rule:** never cast a byte buffer to a struct; read each field out of its range, with an endianness.

## Learn more

- [`FileExt`](https://doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html) · [`u32::to_le_bytes`](https://doc.rust-lang.org/std/primitive.u32.html#method.to_le_bytes) · [`slice::chunks_exact`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_exact)
