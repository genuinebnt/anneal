After a crash all the system has of its recent past is the **log**: a file of records, each saying one thing that happened. A transaction began, a record slot changed from one state to another, a transaction committed, rolled back, or a checkpoint was taken. This stage is the byte format of those records (`LogRecord` in `src/recovery/log_record.rs`) and, with it, the first question every log has to answer: **where does the log end?** The machine can die in the middle of writing a record, so the last record may be cut short, or garbled by a disk that wrote some sectors and not others. A record therefore carries enough to prove itself: its length and a checksum of its body.

> [!CHECK] A log of ten records is cut by a crash 3 bytes into the seventh. What should reading the log return, and why must reading *stop* there rather than skip the bad record and carry on with the eighth? A single bit flips in the body of record 4. What should happen, and what must never happen?
> ||Reading returns the first six records and stops: record 7 is incomplete, and nothing after it can be trusted (the bytes after a cut are not record boundaries at all; even if the eighth looked valid it was written *after* a record that never finished, so it cannot belong to the history). A flipped bit makes the checksum fail: the log ends at record 4 (records 1 to 3 stand). What must never happen is reading a damaged record as a *different but valid* one: a record that quietly says another transaction or another value would make recovery do the wrong thing without any error.||
>
> - What does the length tell you that the checksum does not, and the other way round?
> - What if the length field itself is damaged and huge?
> - What would a record of length 0 be?

## The task

In `src/recovery/log_record.rs` (the `LogRecord` enum, `txn()` and `parse_log`, which reads a whole log record by record, are given):

- `LogRecord::serialize(&self) -> Vec<u8>`: the bytes of one record. The format is yours, but it must carry the record's length and a checksum of its body, then the body: a kind and the fields. (A change holds a `Rid`, and the states before and after: each `None` or some bytes.)
- `LogRecord::deserialize(bytes) -> Option<(LogRecord, usize)>`: reads **one** record from the start of `bytes` and says how many bytes it took. `None` if the bytes are too short for a whole record, or the length, the checksum or the kind do not check out.

The tests: exact scenarios (every kind of record reads back as written; records follow each other and each says how long it was; every cut of a record is rejected; every damaged byte is either rejected or reads back as the same record; the log ends where the first bad record starts), and a property: **for random records, a stream cut at any byte, or with any one byte changed, parses to a prefix of what was written**, and never to a record that was not there.

## Your freedom

The whole format: field order, widths, byte order, which checksum (a sum, FNV, CRC32). The tests never look at the bytes, only at what comes back.

## The Rust toolbox

**Fixed-width integers as bytes.** `n.to_le_bytes()` gives `[u8; 8]`; `u64::from_le_bytes(slice.try_into().unwrap())` reads it back (the slice must be exactly 8 bytes: `bytes.get(a..b)?` returns `None` instead of panicking when it is too short).

**A tiny reader.** A closure `let mut take = |n| { let s = body.get(at..at + n)?; at += n; Some(s) };` keeps a position and returns `None` when the bytes run out; every `?` on it is a "not a record".

**`checked_add`.** A damaged length can be huge: `8usize.checked_add(len)?` turns an overflow into `None` instead of a panic.

**A checksum in ten lines.** FNV-1a: `h ^= byte; h = h.wrapping_mul(0x0100_0193)` over the body. It is not cryptographic; it only has to notice a cut or a flipped bit.

**`match` on a tag.** The kind byte selects the record; an unknown kind is `None`, not a panic.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `get(range)`, `extend_from_slice`, `to_le_bytes`.
- [F7 I/O & serialization](/t/f7-io-serialization): length-prefixed, checksummed records.
- L7 Enums & pattern matching: one `match` over every kind in both directions.
- The optional *write-ahead logging* and *serialization of values* concepts.

## Tests

- Round trips of every kind of record; consecutive records and their lengths; cuts and damage are rejected; the log ends at the first bad record.
- Property: random records round trip; any cut or any single damaged byte gives a prefix of the written records.

## Hints

### Check before you trust

Read the length, then check that the body is all there, then compute the checksum, then compare, and only then look inside. A body that parses but whose checksum differs must be rejected: the checksum is what makes a flipped bit visible.

### The end of a record is part of the record

After reading the fields, the position must be exactly at the end of the body: leftover bytes mean the record was not what the length said.

### Nothing is too short to be a record

`deserialize(&[])`, a lone byte and eight bytes of zeros must all be `None`: a zero length with a matching checksum of an empty body still has no kind byte.

## Performance

A log record is written once and read once, in order: the format should be cheap to produce (append bytes to a buffer) rather than cheap to search. A 4-byte length and 4-byte checksum per record are a few percent of a typical record; engines that log many tiny records batch them behind one checksum.

**Measure it.** Serialize and parse a million small change records with FNV-1a and with a plain byte sum; compare time and the number of damaged records each checksum fails to notice in a random-damage test.

## Experiment

Optional. Predict first, then run.

1. **No checksum.** Remove the check. Which tests fail, and which of them could only a damaged byte find?
2. **A weak checksum.** Use the sum of the bytes. Does the damage property still pass? Find the smallest damage it misses (two bytes that cancel).

## Other designs

- **Length plus checksum per record (ours, like most).**
- **A checksum per log page or segment** (PostgreSQL's WAL pages, with a CRC per record and a magic number per page).
- **A trailing length** so the log can be read backwards.
- **Self-synchronising records** (a marker byte sequence) so reading can resume after damage; most databases refuse to guess instead.

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a `struct LogRecord { uint32_t size_; lsn_t lsn_; ... }` written with `memcpy` | an `enum LogRecord` and an explicit `serialize` |
| `std::memcpy(&x, buf + off, sizeof x)` | `u64::from_le_bytes(bytes.get(off..off + 8)?.try_into().unwrap())` |
| a variable-length tail after a fixed header | length-prefixed fields read through a small cursor |

**Port rule:** a struct that is memcpy'd becomes an enum with a hand-written wire format; every unchecked read becomes a `get(..)?`.

## Learn more

- [`u64::to_le_bytes`](https://doc.rust-lang.org/std/primitive.u64.html#method.to_le_bytes) · [FNV hash](http://www.isthe.com/chongo/tech/comp/fnv/) · PostgreSQL's [WAL record format](https://www.postgresql.org/docs/current/wal-internals.html)
