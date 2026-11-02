A tuple in a page is a row of values laid out in bytes. For that, every `Value` must be able to write itself into a byte slice and be read back from one, with a **format that never changes** (the bytes outlive the program) and that represents NULL without any extra marker. This stage is `storage_size`, `serialize_to` and `deserialize_from`, and the property that matters is the round trip: whatever you write at any position of a buffer comes back identical, and nothing next to it is disturbed.

> [!CHECK] A `VARCHAR` is stored as a 4-byte length, the text, and a zero byte, and the length counts the zero byte. A NULL `VARCHAR` is stored as the single length `u32::MAX` with no text. What would go wrong if an empty string were also stored as a length of zero with no zero byte? How does the reader tell NULL from "".
> ||The reader sees a length: `u32::MAX` means NULL; any smaller length means a string of `length - 1` bytes followed by a terminator. An empty string has length 1 (just the terminator), so it differs from NULL (`u32::MAX`) and from "no length at all". If "" were stored with length 0, the reader would have no way to know how many bytes follow the length for the *next* column (`length - 1` underflows), and a NULL and an empty string could be confused. The format must make the two cases distinct and every length decodable.||
>
> - Which stored length means NULL?
> - How many bytes does `"ab"` take?
> - What does the reader do with a length that is larger than the remaining bytes?

## The task

- `storage_size()`: the number of bytes `serialize_to` writes: the type's size for fixed types; for `Varchar` `4 + text + 1`; a NULL `Varchar` is 4.
- `serialize_to(&self, storage: &mut [u8])`: writes the value **little-endian** at the start of `storage` (at least `storage_size()` long). A NULL is its **reserved encoding** (`i32::MIN` for an `INTEGER`, `f64::MIN` for a `DECIMAL`, `u64::MAX` for a `TIMESTAMP`, `i8::MIN` for a `BOOLEAN`, the length `u32::MAX` for a `VARCHAR`). A boolean is one byte, 0 or 1. A `VARCHAR` is a `u32` length that counts the terminator, the text, and a zero byte.
- `deserialize_from(storage, type_id) -> Result<Value>`: the inverse; reads only what the type needs; a reserved encoding is a NULL; `Invalid` is an `UnknownType` error.

The tests: a property that every value (of every type, NULLs included) written at a random offset of a buffer full of `0xAA` leaves every byte outside its `storage_size()` bytes untouched and reads back equal; fixed-size types take exactly their type size; the byte formats are checked on examples (little-endian, the string layout, NULL encodings); reserved encodings read as NULLs; a longer buffer is fine.

## Your freedom

How you write and read the bytes. The **format is fixed** (BusTub's): later modules store tuples in pages with it, and the tests check it. Within that: loops, macros, or one function per type.

## The Rust toolbox

**`to_le_bytes` and `from_le_bytes`.** `storage[..4].copy_from_slice(&v.to_le_bytes())` writes; `i32::from_le_bytes(storage[..4].try_into().unwrap())` reads. The `unwrap` is on a slice of the right length that you made yourself.

**`copy_from_slice` panics on a length mismatch.** That is a feature: if `storage` is too short you find out immediately, not by corrupting a neighbour.

**The typed constructors do the NULL mapping.** Reading an `i32` with `Value::integer(x)` already returns a NULL for `i32::MIN`: reuse the constructors on the way in, and write `Value::null(t)`'s reserved number on the way out.

**Strings as bytes.** `s.as_bytes()` and `String::from_utf8_lossy(bytes)`; the stored length minus one is the text length.

**A match per type, in both directions.** Writing and reading are mirror images; write them next to each other and re-read each against the other.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): slicing with ranges, `copy_from_slice`, `try_into` to an array.
- S8 The core traits: byte order, encodings.
- The optional concepts *serialization of values* and *bytes, endianness and views*.
- [F7 I/O & serialization](/t/f7-io-serialization): Encodings: a value as bytes.

## Tests

- Every value of every type survives its bytes at any offset, touching nothing else; fixed-size types take their type size.
- The byte formats: little-endian integers, the string layout, the NULL encodings.
- Reserved encodings read as NULLs; the invalid type is an error; only the needed bytes are read.

## Hints

### Start with the fixed-size types

They are one line each. Make the property pass for them with `Varchar` excluded, then add the string.

### The terminator

The stored length is `text.len() + 1`: the one that counts the zero byte. Check "ab": `[3, 0, 0, 0, b'a', b'b', 0]`.

### A NULL varchar

Only 4 bytes are written: `u32::MAX`. The reader must not look for text after it.

## Performance

Writing a value is a copy of a few bytes; reading is the same. A row of ten integers is 40 bytes written in ten small copies, which the compiler merges. A tuple format that avoids per-value tagging (the schema knows the types) is what keeps this fast.

**Measure it.** Serialise and deserialise a million `Integer` values into one big buffer and compute bytes per nanosecond; compare with `memcpy` of the same amount.

## Experiment

Optional. Predict first, then run.

1. **Big-endian.** Switch integers to big-endian. Which test fails, and what would break if a file written by the old code were read by the new?
2. **No terminator.** Drop the zero byte and store the plain length. Which test notices, and which later module would break?

## Other designs

- **BusTub's format (ours).** Fixed widths, a length prefix and a terminator for text, reserved numbers for NULL.
- **A null bitmap per tuple** (PostgreSQL's): a bit per column, so integers can use every number; the fixed part is smaller for sparse rows.
- **Variable-length integers** (varints): small numbers take one byte; used by Parquet and Protocol Buffers; the tuple is no longer random access.
- **Order-preserving encodings:** bytes that sort like values, so `memcmp` is the comparison.

## In BusTub

BusTub has one `Type` class per SQL type (`IntegerType`, `VarlenType`, ...) with virtual `Add`, `CompareEquals`, `CastAs` and friends, and a `Value` that holds a tagged union. This course keeps the data model (the `TypeId` enum and the `Value` enum are given) and puts every operation in `Value`'s methods, matching on the variants.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `memcpy(storage, &val_.integer_, sizeof(int32_t))` | `storage[..4].copy_from_slice(&v.to_le_bytes())` |
| `*reinterpret_cast<const int32_t *>(storage)` | `i32::from_le_bytes(storage[..4].try_into().unwrap())` |
| `strlen`-style terminated strings | a stored length plus a terminator |
| `char *` from a `std::string` | `s.as_bytes()` |

**Port rule:** an unaligned pointer read becomes `from_le_bytes` over a slice; both are defined behaviour on any alignment.

## Learn more

- [`i32::to_le_bytes`](https://doc.rust-lang.org/std/primitive.i32.html#method.to_le_bytes) · [`String::from_utf8_lossy`](https://doc.rust-lang.org/std/string/struct.String.html#method.from_utf8_lossy)
- PostgreSQL's tuple layout (null bitmap and alignment): <https://www.postgresql.org/docs/current/storage-page-layout.html>
