An index must hold keys of many shapes: an integer, a short string, a composite of several columns. BusTub's answer is `GenericKey<N>`, a key that is just `N` opaque bytes, plus a **comparator** that knows how to read them. The page code never looks inside a key; it stores `N` bytes per entry and asks the comparator which of two is smaller. Separating *storing* a key from *ordering* it is what lets one B+ tree serve every key type, and the same trick appears in sort executors, merge joins and every ordered structure you will build.

> [!CHECK] Two `i64` keys are stored as eight little-endian bytes each. Compare them with `memcmp` (byte by byte, from the first byte). Give a pair of numbers for which the byte order and the numeric order disagree, and say which bytes decide it.
> ||Take 256 and 1. Little-endian, 1 is `01 00 …` and 256 is `00 01 …`; `memcmp` compares the first bytes, `00 < 01`, so it says 256 < 1. Negative numbers are worse: -1 is all `FF` and sorts above every positive number. Byte order is not numeric order, so the comparator must decode the integers (or the format must be chosen to sort correctly, like big-endian with the sign bit flipped).||
>
> - Which byte of a little-endian integer is the most significant?
> - Why does the sign break byte-wise comparison?
> - What does a comparator have that `memcmp` does not?

## The task

- `GenericKey<N>` (the `data: [u8; N]` is given): `set_from_integer(i64)` zeroes the key and stores the integer in its first 8 bytes; `get_as_integer()` reads it back. `FixedSize` for `GenericKey<N>`: `SIZE` is `N`, and encode/decode copy the bytes.
- `KeyComparator<K>` has `compare(&self, a, b) -> Ordering`. Implement `GenericComparator<N>` (orders keys by the integer in their first 8 bytes, signed) and `IntComparator` (plain `i32` order).
- `FixedSize` for a pair `(A, B)`: its `SIZE` is the two sizes added, and it lays out the first value, then the second. Index entries are `(key, value)` pairs.
- `array_size(metadata, entry)`: how many entries of `entry` bytes fit in a page after `metadata` bytes of header. It is a `const fn`, so a page layout can compute its capacity at compile time.

The tests check, for random values: the integer comes back and the rest of the key is zero; keys survive bytes at any offset; a comparator orders keys exactly as the integers inside them (so sorting by it sorts the integers, negatives first); pairs survive bytes, nested pairs too; `array_size` at compile time.

## Your freedom

How you copy bytes and read the integer; whether the comparator decodes the keys or compares with a trick. The comparator's result must be the integers' order.

## The Rust toolbox

**Const generics.** `GenericKey<const KEY_SIZE: usize>` is generic over a *number*, so `GenericKey<8>` and `GenericKey<64>` are different types with arrays of the right length and no allocation. Methods are written once in `impl<const KEY_SIZE: usize> GenericKey<KEY_SIZE>`.

**`Ordering` and `cmp`.** `a.cmp(&b)` returns `Less`, `Equal` or `Greater`. A comparator is a function to `Ordering`; `sort_by(|a, b| cmp.compare(a, b))` uses it, and `ord.reverse()` flips it.

**Splitting a slice in two.** `out.split_at_mut(A::SIZE)` gives two disjoint mutable slices at once, which is how a pair's `encode` writes both halves without the borrow checker objecting to two `&mut` into one slice.

**A `const fn`.** `pub const fn array_size(metadata: usize, entry: usize) -> usize` can be used in a `const` item or an array length. Arithmetic is fine in a `const fn`; allocation and trait method calls on generics (until recently) are not.

**Total orders.** A comparator must be a *total order*: antisymmetric (`cmp(a,b)` is the reverse of `cmp(b,a)`), transitive, and `Equal` only for equal keys. A comparator that is not will make a sorted structure misbehave in confusing ways far from the bug; the boss stage tests the laws.

## If this is new

- [L5 Generics & associated types](/t/l5-generics): type parameters; const generics are the same idea for numbers.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `trait KeyComparator<K>`.
- [S3 Vec & slices](/t/s3-vec-slices): `sort_by`, `split_at`, `copy_from_slice`.
- S8 The core traits: Implement by hand: `TryFrom` (slice to array), `Ord` for a comparator.
- [F7 I/O & serialization](/t/f7-io-serialization): Encodings: byte order, fixed-width encodings, `bytemuck` as the safe cast.

## Tests

- A key holds any `i64`; `set_from_integer` clears the rest of the key.
- Keys of 8 and 24 bytes survive bytes at any offset.
- Comparators agree with integer order; sorting with a comparator sorts the integers.
- Pairs survive bytes; sizes add; nesting works.
- `array_size`, including at compile time; keys of other sizes; the zero key.

## Hints

### Why does `set_from_integer` clear the key first?

A key is compared and hashed as a whole: if stale bytes remain after the integer, two keys with the same integer could differ. What does the test do to a key before calling it?

### The comparator reads, it does not memcmp

Write `compare` as "decode both integers, compare them". It is one line. The check-yourself question is why the shortcut of comparing the byte arrays is wrong.

### The pair

`SIZE` of the pair is `A::SIZE + B::SIZE`: a const expression in the impl. Encode the first into `out[..A::SIZE]` and the second into the rest.

## Performance

A comparison decodes two integers: two loads and a compare. A B+ tree does `O(log n)` of them per lookup, so keep the comparator branch-free and inlinable; trait method calls on a concrete comparator type are resolved at compile time and inlined.

**Measure it.** Sort one million `GenericKey<8>` by your comparator and by `sort_unstable_by_key(|k| k.get_as_integer())`. Should be the same; if not, find what differs.

## Experiment

Optional. Predict first, then run.

1. **Order-preserving bytes.** Store the integer big-endian with the top bit flipped (`(n as u64 ^ (1 << 63)).to_be_bytes()`). Then `memcmp` order equals numeric order. What did you gain (comparing without decoding, strings and integers in one key) and lose (the format is no longer plain)?
2. **A composite key.** Build a comparator for a key of two integers `(a, b)` stored back to back. What does the total-order law require of the second field when the first ties?

## Other designs

- **Decode in the comparator (ours).** Simple; the stored form is plain.
- **Order-preserving encodings** ("memcomparable"). The comparator is `memcmp`; used by many production systems.
- **Typed keys** (`i32`, `String`) instead of byte keys. Fine in Rust generics; variable-length keys then need a different page layout.

## In BusTub

```cpp
template <size_t KeySize> class GenericKey {
  void SetFromInteger(int64_t key) { memset(data_, 0, KeySize); memcpy(data_, &key, sizeof(int64_t)); }
  auto ToString() const -> std::string { return std::to_string(*reinterpret_cast<int64_t *>(const_cast<char *>(data_))); }
  char data_[KeySize];
};
template <size_t KeySize> class GenericComparator { int operator()(const GenericKey<KeySize> &lhs, const GenericKey<KeySize> &rhs) const; };
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `template <size_t KeySize> class GenericKey` | `struct GenericKey<const KEY_SIZE: usize>` |
| comparator class with `operator()` returning -1, 0, 1 | `trait KeyComparator` returning `Ordering` |
| `constexpr` function | `const fn` |
| `std::pair<KeyType, ValueType>` | a tuple `(K, V)` |

**Port rule:** a function object returning -1/0/1 becomes a trait method returning `Ordering`.

## Learn more

- [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html) · [`slice::split_at_mut`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_at_mut) · [const generics](https://doc.rust-lang.org/reference/items/generics.html#const-generics)
