A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`crc32`, `seal` and `unseal` in `src/storage/table/checksum.rs`: the standard CRC-32 (IEEE 802.3, reflected, polynomial `0xEDB88320`, initial value and final xor `0xFFFFFFFF`) of a byte slice; `seal(payload)` returns the payload followed by its CRC as 4 little-endian bytes; `unseal(bytes)` returns the payload if the CRC matches and an error naming the problem otherwise.

## Why

A disk lies in small ways: a bit flips, a write is torn. A checksum on every page turns silent corruption into an error you can act on (use the replica, run recovery). CRC-32 detects every single-bit error and every burst up to 32 bits, and a table makes it fast enough to run on every page read.

## The contract

- `crc32(b"123456789") == 0xCBF43926`; `crc32(b"") == 0`.
- `seal(p)` is `p` plus 4 bytes. `unseal` fails with `TooShort` for fewer than 4 bytes and `Mismatch` when the stored CRC is not the CRC of the rest.

## Invariants

These must hold after every step, whatever the input:

- `unseal(seal(p)) == Ok(p)`.
- The CRC of the data is a function of the bytes only (no state between calls).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Flipping any single bit of a sealed buffer makes `unseal` fail.
- Appending or removing a byte makes `unseal` fail (with overwhelming likelihood: for these tests, always).
- Two different payloads of equal length seal to different CRCs in the cases tested.

## Examples

Worked cases (the tests include them):

```text
crc32("123456789") = 0xCBF43926
seal("abc") = "abc" + crc32("abc") little-endian
```

## What the tests check

- The standard check values.
- Round trips.
- Every single-bit flip of a sealed buffer is caught.
- A property over random payloads.

## Done when

All the `s3b_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Hand-rolled serialisation on `std::io`](/cookbook/p/std-hash-collections-io-serialization-formats): a table-driven CRC-32 sealing records, with hostile-input decoding.
- [A write-ahead log](/cookbook/p/wal-append-log): checksummed log records that detect torn writes and corruption.
