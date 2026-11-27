Everything an index stores lives in 8 KiB pages, and a page is only bytes. To put a number, a page id, a record id or a key into a page you must say exactly how many bytes it takes and in what order they go, because the bytes outlive the program that wrote them. BusTub's C++ does this by casting the page's `char *` to a struct pointer, which depends on the machine's alignment and byte order and is undefined behaviour when it goes wrong. Safe Rust says it out loud: a trait, `FixedSize`, states a value's **size** and its **encoding**, and every page in the next modules reads and writes through it.

An index must also hold keys of many shapes: an integer, a short string, several columns side by side. BusTub's answer is `GenericKey<N>`, a key that is `N` opaque bytes, plus a **comparator** that knows how to read them. The page stores `N` bytes per entry and never looks inside; it asks the comparator which of two keys is smaller. Separating *storing* a key from *ordering* it is what lets one B+ tree serve every key type, and the same split appears in sort executors, merge joins and every ordered structure you will build. This stage builds both halves: values that survive a trip through bytes, and keys that know their order.

> [!CHECK] Two `i64` keys are stored as eight little-endian bytes each, and someone compares them with `memcmp` (byte by byte, from the first byte). Give a pair of numbers for which the byte order and the numeric order disagree, say which bytes decide it, and say where `-1` lands.
> ||Take 256 and 1. Little-endian, 1 is `01 00 …` and 256 is `00 01 …`; `memcmp` compares the first bytes, `00 < 01`, so it says 256 < 1. The least significant byte comes first, so it decides, which is backwards. `-1` is all `FF` and sorts above every positive number, because the sign lives in the top bit of the last byte. Byte order is not numeric order, so the comparator must read the integers (or the format must be built to sort, like big-endian with the sign bit flipped).||
>
> - Which byte of a little-endian integer is the most significant?
> - What bit pattern is -1, and why does the sign break a byte-wise comparison?
> - What does a comparator know that `memcmp` does not?

## The task

Implement `FixedSize` (in `fixed_size.rs`) for `i32`, `u32`, `i64`, `PageId`, `Rid`, `GenericKey<N>` and pairs `(A, B)`; `Rid::get` and `Rid::from_i64`; `GenericKey::set_from_integer` and `get_as_integer`; the comparators `GenericComparator<N>` and `IntComparator`; and `array_size`. The sizes are given (4, 4, 8, 4, 8, `N`, and a pair's two sizes added). The contract:

- **Values survive bytes, in place.** `decode(encode(x)) == x` for every value, at any position in a buffer, including `PageId::INVALID` (`-1`) and every negative number. `encode` writes exactly its `SIZE` bytes and nothing around them.
- **Where other code reads the bytes, the format is fixed.** Integers and page ids are little-endian; a key's integer sits in its first 8 bytes, little-endian, with the rest of the key zero, and a key encodes as its `N` bytes unchanged (module 2b hashes these exact bytes and checks the hashes against fixed expected values). `Rid::get` is BusTub's packing, the page id in the high 32 bits with its sign and the slot in the low 32, and `from_i64` undoes it.
- **Comparators are the integers' order.** `GenericComparator` orders keys exactly as the signed integers inside them, `IntComparator` as plain `i32`s; both are total orders (the boss checks the laws).
- **Capacity at compile time.** `array_size(metadata, entry)` is how many whole entries of `entry` bytes fit in a page after `metadata` bytes of header, as a `const fn`, so a page layout can size its array in a `const`.

> [!ASIDE] The steps, if you would rather not work them out
> 1. The integers: `copy_from_slice` of `to_le_bytes()` to encode, `from_le_bytes` of the slice turned into an array to decode.
> 2. `PageId` encodes its `i32`. A `Rid` either encodes `get()` as an `i64` or writes its page and slot as two 4-byte fields.
> 3. A key: zero `data`, then copy the integer's 8 bytes into the front; read them back the same way. Encode and decode copy all `N` bytes.
> 4. The comparators: read both integers and compare them with `cmp`.
> 5. A pair: split the output at `A::SIZE` and let each half encode itself; decode the same way.
> 6. `array_size`: the page size minus the metadata, divided by the entry size.

## Your freedom

How a `Rid` and a pair lay out their parts: page first or slot first, through `get()` or as two fields, the first value of a pair first or last. The tests only decode what you encoded. How a comparator reaches its answer: decode both integers, or compare the bytes from the most significant end with the sign flipped. And how you avoid writing five nearly identical integer impls: by hand, by delegating (`PageId` through `i32`), or with a small `macro_rules!`.

What is not free is the byte order of the integers and the key. That is the lesson inside the stage: an encoding is your choice until another program reads it, and from then on it is a format, a contract you keep.

## The Rust toolbox

**Integers know their bytes.** `x.to_le_bytes()` returns `[u8; N]`; `i32::from_le_bytes(array)` builds one back. `_be_` is big-endian, `_ne_` the machine's own. Little-endian is what x86 and ARM use natively and what BusTub's files contain.

**From a slice to an array.** `from_le_bytes` wants `[u8; 4]`, not `&[u8]`. `bytes.try_into().expect("4 bytes")` converts a slice of exactly the right length (`TryFrom<&[u8]> for [u8; 4]`); the `expect` message is your assertion that the caller kept the contract. `out.copy_from_slice(&bytes)` writes the other way and panics if the lengths differ, which is what you want for a size mistake.

**Packing with shifts.** `((page as i64) << 32) | slot as i64` puts the page in the high half. `slot as i64` of a `u32` zero-extends; `page as i64` of a negative `i32` sign-extends, which is what makes the high half right. `rid as u32` takes the low 32 bits; `(rid >> 32) as i32` takes the high 32 with the sign.

**Associated consts and const generics.** `const SIZE: usize;` in a trait lets generic code use `T::SIZE` in arithmetic, computed per type. `GenericKey<const KEY_SIZE: usize>` is generic over a *number*, so `GenericKey<8>` and `GenericKey<64>` are different types with arrays of the right length and no allocation; a pair's `SIZE` is the const expression `A::SIZE + B::SIZE`.

**Splitting a slice in two.** `out.split_at_mut(A::SIZE)` gives two disjoint mutable slices at once, which is how a pair writes both halves without the borrow checker objecting to two `&mut` into one slice.

**`Ordering` and total orders.** `a.cmp(&b)` returns `Less`, `Equal` or `Greater`; `sort_by(|a, b| cmp.compare(a, b))` sorts with a comparator. A comparator must be a total order: antisymmetric, transitive, `Equal` only for equal keys. One that is not makes a sorted structure misbehave far from the bug.

**A `const fn`.** `pub const fn array_size(metadata: usize, entry: usize) -> usize` can be used in a `const` item or an array length. Arithmetic is fine inside one; allocation and trait method calls on generics are not.

**Five impls, one shape.** `macro_rules! le { ($($t:ty),*) => { $(impl FixedSize for $t { ... })* } }` writes one impl per type from one template; `le!(i32, u32, i64);`. Worth it at three copies, not at two.

**Compiler messages you will meet.** "the trait bound `[u8; 4]: From<&[u8]>` is not satisfied": use `try_into()`. "mismatched types, expected `u32`, found `i32`": a cast is missing. The optional *integers and casts* concept has the rules.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): implementing a trait for several types; `trait KeyComparator<K>`.
- [L5 Generics & associated types](/t/l5-generics): `T::SIZE`; const generics are the same idea for numbers.
- [F2 Data layout](/t/f2-data-layout): bytes, sizes and alignment.
- [S3 Vec & slices](/t/s3-vec-slices): slicing, `copy_from_slice`, `split_at_mut`, `sort_by`.
- S8 The core traits: Implement by hand: `TryFrom` (slice to array), `Ord` for a comparator.
- S11 mem, ptr & alloc: Understand: `size_of`, `align_of`.
- [F7 I/O & serialization](/t/f7-io-serialization): Encodings: byte order, fixed-width encodings, `bytemuck` as the safe cast.
- Y5 Testing & verification: Understand it: round-trip properties and a `Vec` model.

## Tests

- Integers, page ids (`INVALID` included), rids, keys of 8 and 24 bytes and nested pairs survive a trip through bytes at any offset, and the bytes around them are untouched.
- `Rid::get` packs page and slot as BusTub does and `from_i64` undoes it for every pair; the default rid names no page, and page 0 is a real page.
- A key holds any `i64` and `set_from_integer` clears the rest of it; the comparators agree with integer order, so sorting by one sorts the integers, negatives first.
- The sizes are what a page layout counts on, and `array_size` gives whole entries, in a `const` too.

## Hints

### Which bytes are yours?

List every value in the stage and ask of each: does any code other than my own `decode` read these bytes? The integers and the key, yes (a hash function in the next module, and the key's integer in every comparison). A `Rid` and a pair, no. That list is where your freedom is, and where it is not.

### What does "untouched neighbours" catch?

An `encode` that writes eight bytes into a four-byte slot panics; one that writes into the wrong range of a larger buffer does not. The tests surround the slot with 0xAA bytes and check them afterwards. Which of your encodings computes its own offsets, and could write past them?

### Why does `set_from_integer` clear the key first?

A key is compared, hashed and stored as a whole: if stale bytes remain after the integer, two keys holding the same integer are different keys to a hash table. The test fills a key with junk before calling it.

### The sign

Take `Rid::new(PageId(-1), 5)`. Write `get()` by hand in hexadecimal, then check that `from_i64` gives back page -1 and slot 5. Then do the same for `PageId(0)`, slot 7, and make sure the two cannot be confused.

## Performance

These are a handful of instructions each: a copy, at most a byte swap, and a length check that is a single comparison. A comparison decodes two integers: two loads and a compare. Page code calls them millions of times per second, and a B+ tree does `O(log n)` comparisons per lookup, so no allocation, no `Vec`, and a comparator the compiler can inline (a trait method on a concrete type is resolved at compile time).

**Measure it.** In release mode, decode one million `i64`s from an 8 MiB buffer and time the loop: expect around a nanosecond per value. Then sort one million `GenericKey<8>` with your comparator and with `sort_unstable_by_key(|k| k.get_as_integer())`. The two should take about the same time; if not, find what differs.

## Experiment

Optional. Predict first, then run.

1. **Big-endian.** Switch your `i32` to big-endian. Predict which tests of this module still pass, then which test of module 2b would fail and why. What does that say about where a format is decided?
2. **Order-preserving bytes.** Store an integer big-endian with the top bit flipped (`(n as u64 ^ (1 << 63)).to_be_bytes()`). Now `memcmp` order equals numeric order. What did you gain (comparing without decoding, strings and integers in one key) and what did you lose (a plain format)?
3. **A composite key.** Build a comparator for a key of two integers `(a, b)` stored back to back. What does the total-order law require of the second field when the first ties?

## Other designs

- **`to_le_bytes` / `from_le_bytes` (ours).** Safe, explicit, portable.
- **`unsafe` pointer casts** (`ptr::read_unaligned`). What C++ does; identical after optimisation, and a place for undefined behaviour.
- **The `bytemuck` or `zerocopy` crates.** Safe zero-cost casts of byte slices to plain structs, when alignment is guaranteed.
- **Order-preserving ("memcomparable") keys.** The comparator becomes `memcmp`; used by many production systems, at the price of an encoding you must decode to read.
- **Typed keys** (`i32`, `String`) instead of byte keys. Fine with Rust generics; variable-length keys then need a different page layout.

## In BusTub

BusTub reads fields with `reinterpret_cast<ExtendibleHTableDirectoryPage *>(page->GetData())`, and its `RID::Get()` returns `(static_cast<int64_t>(page_id_)) << 32 | slot_num_`. Keys and comparators:

```cpp
template <size_t KeySize> class GenericKey {
  void SetFromInteger(int64_t key) { memset(data_, 0, KeySize); memcpy(data_, &key, sizeof(int64_t)); }
  char data_[KeySize];
};
template <size_t KeySize> class GenericComparator { int operator()(const GenericKey<KeySize> &lhs, const GenericKey<KeySize> &rhs) const; };
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `*reinterpret_cast<int32_t *>(data + off)` | `i32::from_le_bytes(data[off..off + 4].try_into().unwrap())` |
| `memcpy(data + off, &x, sizeof(x))` | `data[off..off + 4].copy_from_slice(&x.to_le_bytes())` |
| `sizeof(T)`, `constexpr` function | `T::SIZE` from the trait, `const fn` |
| `template <size_t KeySize> class GenericKey` | `struct GenericKey<const KEY_SIZE: usize>` |
| comparator class with `operator()` returning -1, 0, 1 | `trait KeyComparator` returning `Ordering` |
| `std::pair<KeyType, ValueType>` | a tuple `(K, V)` |

**Port rule:** a pointer cast over page bytes becomes an explicit read or write with a stated byte order, and a function object returning -1/0/1 becomes a trait method returning `Ordering`.

## Learn more

- [`i32::from_le_bytes`](https://doc.rust-lang.org/std/primitive.i32.html#method.from_le_bytes) · [`TryFrom` for arrays](https://doc.rust-lang.org/std/convert/trait.TryFrom.html) · [`slice::copy_from_slice`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_from_slice) · [`slice::split_at_mut`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_at_mut)
- [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html) · [const generics](https://doc.rust-lang.org/reference/items/generics.html#const-generics) · [Endianness](https://en.wikipedia.org/wiki/Endianness)
