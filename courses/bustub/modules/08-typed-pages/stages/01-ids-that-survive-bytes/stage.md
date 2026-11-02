Everything an index stores lives in 8 KiB pages, and a page is just bytes. To put a number, a page id or a record id into a page you must say exactly how many bytes it takes and in what order they go, because the bytes outlive the program that wrote them. BusTub's C++ does this by casting the page's `char *` to a struct pointer, which depends on the machine's alignment and byte order and is undefined behaviour when it goes wrong. Safe Rust says it out loud: a trait, `FixedSize`, states a value's **size** and its **encoding**, and the page code reads and writes through it.

> [!CHECK] You encode a `PageId(-1)` (the "no page" marker) as four bytes and decode it again. Which mistakes in the encoding would make it come back as 4 294 967 295 or as 255? Say what each mistake looks like in code.
> ||Decoding as `u32` instead of `i32` turns the all-ones pattern into 4 294 967 295; reading only the first byte, or casting through `u8`, gives 255. Both are signedness or width mistakes: the pattern in the bytes is right, the type it is read as is wrong. The round trip `decode(encode(x)) == x` for every `x`, including negative ones, catches both.||
>
> - What bit pattern is -1 as an `i32`?
> - What does `as u32` do to it, and what does `as i64` do?
> - Why must a page id be signed in this course?

## The task

Implement `FixedSize` (in `fixed_size.rs`) for `i32`, `u32`, `i64`, `PageId` and `Rid`, and `Rid::get` / `Rid::from_i64`.

- `SIZE` is the number of bytes (given: 4, 4, 8, 4, 8). `encode(&self, out)` writes exactly `SIZE` bytes into `out` (which is exactly `SIZE` long); `decode(bytes)` reads them back.
- The only promise is the round trip: `decode(encode(x)) == x` for every value, at every position in a buffer, **without touching the bytes around it**. Which byte order you use is your decision; the module's given helpers (`page_bytes.rs`) use little-endian, and so should you unless you have a reason.
- `PageId::INVALID` is `-1` and must survive.
- `Rid::get()` packs the page id into the **high 32 bits** and the slot number into the **low 32 bits** of an `i64` (BusTub's definition), `Rid::from_i64` undoes it. A negative page id keeps its sign; the slot never leaks into the page.

## Your freedom

Byte order, how you convert, whether a `Rid` encodes itself through `get()` or as two integers. The tests only decode what you encoded.

## The Rust toolbox

**Integers know their bytes.** `x.to_le_bytes()` returns `[u8; N]`; `i32::from_le_bytes(array)` builds one back. `_be_` is big-endian, `_ne_` the machine's own. Little-endian is what x86 and ARM use natively and what BusTub's files contain.

**From a slice to an array.** `from_le_bytes` wants `[u8; 4]`, not `&[u8]`. `bytes.try_into().expect("4 bytes")` converts a slice of exactly the right length (`TryFrom<&[u8]> for [u8; 4]`); the `expect` message is your assertion that the caller kept the contract.

**Writing into a slice.** `out.copy_from_slice(&self.to_le_bytes())` copies and panics if the lengths differ, which is what you want for a size mistake.

**Packing with shifts.** `((page as i64) << 32) | slot as i64` puts the page in the high half. Watch the casts: `slot as i64` of a `u32` is fine (zero-extended), `page as i64` of a negative `i32` sign-extends, which is what makes the high half right. `rid as u32` takes the low 32 bits; `(rid >> 32) as i32` takes the high 32 with the sign.

**An associated const in a trait.** `const SIZE: usize;` lets generic code use `T::SIZE` in array lengths and arithmetic, and the compiler computes it per type.

**Compiler messages you will meet.** "the trait bound `[u8; 4]: From<&[u8]>` is not satisfied": use `try_into()`. "mismatched types, expected `u32`, found `i32`": a cast is missing. Casting is `as`; read *integers and casts* if the rules are new.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): implementing a trait for several types.
- [L5 Generics & associated types](/t/l5-generics): the first problems, for `T::SIZE`.
- [F2 Data layout](/t/f2-data-layout): bytes, sizes and alignment.
- [S3 Vec & slices](/t/s3-vec-slices): slicing and copying.
- S8 The core traits: Implement by hand: `TryFrom` (slice to array), `Ord` for a comparator.
- S11 mem, ptr & alloc: Understand: `size_of`, `align_of`.
- [F7 I/O & serialization](/t/f7-io-serialization): Encodings: byte order, fixed-width encodings, `bytemuck` as the safe cast.
- Y5 Testing & verification: Understand it: round-trip properties and a `Vec` model.

## Tests

- `i32`, `u32`, `i64`, `PageId` (including `INVALID`) and `Rid` survive a trip through bytes at any offset, and the bytes around them are untouched.
- `Rid::get` packs page and slot as specified and `from_i64` undoes it for every pair.
- The sizes are 4, 4, 8, 4 and 8; the default `Rid` names no page; page 0 is a real page.

## Hints

### What does "untouched neighbours" catch?

An `encode` that writes eight bytes into a four-byte slot panics; one that writes into the wrong half does not. The tests surround the slot with 0xAA bytes and check them afterwards. Which of your two choices (slice math or array copies) can write too far?

### The sign

Take `rid = Rid::new(PageId(-1), 5)`. Write down `get()` by hand as hexadecimal and check that `from_i64` returns page -1 and slot 5.

## Performance

These are a handful of instructions each (a byte swap at most). The cost to keep in mind is `decode(...).expect(...)`: the length check is a single comparison. Page code calls these millions of times per second, so no allocation, no `Vec`.

**Measure it.** Decode one million `i64`s from a 8 MiB buffer in a loop and time it in release mode. You should see about a nanosecond per value.

## Experiment

Optional. Predict first, then run.

1. **Big-endian.** Switch your `i32` to big-endian. Do the tests still pass? What would break if you read a file written by a little-endian build?
2. **Truncate.** Encode a `Rid` through `get() as i32` by mistake. Which test catches it and how short is the counterexample?

## Other designs

- **`to_le_bytes` / `from_le_bytes` (ours).** Safe, explicit, portable.
- **`unsafe` pointer casts** (`ptr::read_unaligned`). What C++ does; faster in theory, identical after optimisation, and a place for undefined behaviour.
- **The `bytemuck` or `zerocopy` crates.** Safe zero-cost casts of byte slices to plain structs, when alignment is guaranteed.
- **Variable-length encodings** (varints). Smaller on disk; the price is that entries are no longer at `index * SIZE`.

## In BusTub

BusTub reads fields with `reinterpret_cast<ExtendibleHTableDirectoryPage *>(page->GetData())`, and its `RID` has `Get()` returning `int64_t`, `(static_cast<int64_t>(page_id_)) << 32 | slot_num_`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `*reinterpret_cast<int32_t *>(data + off)` | `i32::from_le_bytes(data[off..off + 4].try_into().unwrap())` |
| `memcpy(data + off, &x, sizeof(x))` | `data[off..off + 4].copy_from_slice(&x.to_le_bytes())` |
| `sizeof(T)` | `T::SIZE` from the trait (or `size_of::<T>()`) |
| `static_cast<int64_t>(page_id) << 32` | `(page.0 as i64) << 32` |

**Port rule:** a pointer cast over page bytes becomes an explicit read or write with a stated byte order.

## Learn more

- [`i32::from_le_bytes`](https://doc.rust-lang.org/std/primitive.i32.html#method.from_le_bytes) · [`TryFrom` for arrays](https://doc.rust-lang.org/std/convert/trait.TryFrom.html) · [`slice::copy_from_slice`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_from_slice)
- [Endianness](https://en.wikipedia.org/wiki/Endianness)
