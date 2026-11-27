A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

Variable-length integers in `src/rust_primer/varint.rs`: `encode_varint` writes a `u64` using as few bytes as it needs, and `decode_varint` reads one back, saying how many bytes it took. The number is cut into groups of 7 bits, lowest group first; each group is one byte, and the high bit of a byte is set when another byte follows.

## Why

Databases and protocols are full of small numbers that are sometimes huge: lengths, offsets, counts. A fixed 8 bytes each wastes space; a varint spends one byte on most of them. It is also a compact exercise in reading a format from a description and being strict about bad input.

## The contract

- `encode_varint(v, out)` appends to `out`, always in the fewest bytes.
- `decode_varint(bytes)` returns `(value, bytes_used)` and ignores whatever follows the number.
- Bytes that are not one varint are an error, never a panic: `Truncated`, `Overflow` (does not fit in 64 bits) or `NonMinimal` (right number, more bytes than needed).

## Invariants

These must hold after every step, whatever the input:

- The encoding of `v` has exactly `ceil(bits(v) / 7)` bytes (one for 0).
- Every byte but the last has its high bit set; the last has it clear.
- `decode(encode(v)) == (v, len)` for every `v`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Encoding is monotone in size: a larger number never takes fewer bytes.
- Appending any bytes after an encoded number does not change what decodes.
- Decoding any byte string either fails with one of the three errors or gives a number whose encoding is exactly the bytes it consumed.

## Examples

Worked cases (the tests include them):

```text
0 -> [0x00]
127 -> [0x7f]
128 -> [0x80, 0x01]
300 -> [0xac, 0x02]
u64::MAX -> 10 bytes
[0x80, 0x00] -> NonMinimal
[0x80] -> Truncated
```

## What the tests check

- Exact bytes for the edges 0, 127, 128 and `u64::MAX`.
- A property over random numbers: round trip, fewest bytes, trailing bytes untouched.
- A property over streams, including streams cut in the middle of a number.
- A property over random bytes: no panic, and accepted input re-encodes to what was read.

## Done when

All the `sr_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Bytes](/cookbook/p/bytes): a 7-bit varint codec with its decode errors, next to endianness and bit tricks.
- [Hand-rolled serialisation on `std::io`](/cookbook/p/std-hash-collections-io-serialization-formats): varints in a real file format, rejecting overlong and truncated encodings.
- [From-scratch codecs](/cookbook/p/encodings-codecs): LEB128 and zigzag varints as protobuf and WebAssembly use them.
