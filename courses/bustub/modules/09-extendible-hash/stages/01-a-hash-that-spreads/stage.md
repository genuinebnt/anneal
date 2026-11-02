An extendible hash table decides where a key lives from the **bits of its hash**: the top bits choose a directory, the low bits choose a bucket, and a full bucket is split by looking at one more bit. That only works if the bits are well mixed: with a poor hash, keys 0 to 100 would share their low bits and every split would send them all to the same side. BusTub hashes with MurmurHash3, and its tests only come out as expected with exactly that function, so you port it. This is also the module's only stage with a fixed right answer: a hash function has no design freedom, and the tests are values recorded from running BusTub's C++.

> [!CHECK] You hash the integers 0, 1, 2, 3, ... and look only at the lowest bit of each hash. If the hash is good, what should you see? What would you see for the identity function `h(x) = x`? Which of the two is a better choice for a table that splits on the low bit, and why?
> ||A good hash gives a lowest bit that is 0 or 1 with equal probability and no pattern: about half of any run of keys. The identity function alternates 0, 1, 0, 1: perfectly balanced too, but only because the input happens to be a counter; keys that are all multiples of 8 would all have low bits 000, so every split on bits 0 to 2 would send them all the same way. A table must not depend on the shape of the keys, so it needs a function that mixes every input bit into every output bit: that is what a hash like Murmur does.||
>
> - Why do the low bits of the identity hash fail for keys that are multiples of 8?
> - What property do you want of every output bit?
> - Which bits does your table use first?

## The task

`murmur_hash3_x64_128(data, seed) -> [u64; 2]` (in `murmur3.rs`) is Austin Appleby's MurmurHash3 for x64 with 128-bit output, returning the two 64-bit halves. `HashFunction::<K>::get_hash(&key) -> u64` hashes a key's bytes (the key's `FixedSize` encoding from module 2a) with seed 0 and keeps the **first** half.

- The tests are BusTub's own function's outputs for inputs of every length from 0 to 40 bytes, for several seeds, for known strings, and for integer and `GenericKey` keys. There is nothing to design: your port must produce the same 128 bits.
- The table works with 32 bits of hash: the **low** half of the 64-bit value (`get_hash(&key) as u32`). That choice belongs to the table in the next stage.

## Your freedom

How you structure the port (a loop over blocks, an iterator over `chunks_exact(16)`) and how you read the tail. The output is fixed.

## The Rust toolbox

**Wrapping arithmetic.** C++ unsigned arithmetic wraps silently; Rust panics on overflow in debug builds. Every multiply and add on the hash state must be `wrapping_mul` / `wrapping_add`, and rotations are `rotate_left`. The compiler's "attempt to multiply with overflow" panic is this mistake.

**Reading blocks.** `data.chunks_exact(16)` gives 16-byte blocks and `.remainder()` the tail; `u64::from_le_bytes(block[0..8].try_into().unwrap())` reads a little-endian word (the C++ reads the machine's own byte order; BusTub runs on little-endian machines).

**The tail as a fall-through switch.** The C++ finishes with a `switch` that falls through from case 15 to case 1. In Rust write it as a loop over the remaining bytes that ORs each byte into a `u64` at the right shift, or as two such loops (bytes 8 to 14 for `k2`, bytes 0 to 7 for `k1`).

**Constants as typed literals.** `0x87c37b91114253d5_u64`; a `const C1: u64` inside the function keeps them next to their use.

**Test vectors.** Write your own small test with one or two values from the test file first and print both halves in hex (`{:#018x}`): the first mismatching length tells you which part of the algorithm (block, tail, finalisation) is wrong.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `chunks_exact`, `try_into` to an array.
- S8 The core traits: wrapping arithmetic and bit operations; the optional *integers and casts* concept covers `wrapping_*` and `as`.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Hashing: SipHash vs Fx vs identity hashers, `BuildHasher`, why low bits matter.
- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): Matrix, bits & math: `wrapping_*`, rotations, masks.

## Tests

- The empty input hashes to zero; inputs shorter than a block use only the tail; one full block and beyond; every length 0 to 40.
- The seed changes the hash.
- Known strings.
- `HashFunction` hashes an `i32` by its four bytes, a `GenericKey<8>` by its eight, keeps only the first half, and spreads small integers in their low bits.

## Hints

### Which length fails first?

The test file has the expected hashes for lengths 0 to 40. If lengths 0 to 15 pass and 16 fails, the block loop is wrong; if 16 passes and 17 fails, the tail after a block; if 0 fails, the finalisation. Print the first mismatching length.

### Rotations and shifts

`x ^= x >> 33` is a shift (bits fall off), `rotl64(x, 31)` is `x.rotate_left(31)` (bits wrap round). Mixing them up is the commonest bug.

### The tail

Bytes 8 to 14 go into the second word and bytes 0 to 7 into the first; byte `i` of a word goes to bits `8 * i`. Count from the end of the data, not the start.

## Performance

MurmurHash3 processes 16 bytes in a handful of multiplications: around 1 byte per cycle at worst, several bytes per cycle on a modern CPU. For a 4-byte key the cost is dominated by the finalisation, about 10 ns.

**Measure it.** Hash one hundred million `i32` keys with your port and with `std`'s default `SipHash`. Predict the ratio first: how much of SipHash's price is for protection against attackers who choose the keys?

## Experiment

Optional. Predict first, then run.

1. **A weak hash.** Replace the function with `x as u64` (identity) and run the table tests of the next stage once they exist: which keys fail to spread, and how do the splits behave?
2. **Avalanche.** Flip one bit of the input and count how many output bits change, over a thousand inputs. A good hash changes about half of them. What does yours do?

## Other designs

- **A line-by-line port (ours).** Follows the C++.
- **`murmur3` crate.** A packaged version; reading its source after your port is a good check.
- **Other hashes** (xxHash, wyhash, FxHash). Faster and simpler for in-memory tables; the tests would then change, since BusTub's expected outputs depend on Murmur.

## In BusTub

```cpp
template <typename KeyType> class HashFunction {
  auto GetHash(KeyType key) -> hash_t {
    return murmur3::MurmurHash3::Hash(reinterpret_cast<const char *>(&key), sizeof(KeyType)); // first of the two 64-bit halves
  }
};
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `uint64_t` arithmetic wraps | `u64` with `wrapping_mul` / `wrapping_add` |
| `ROTL64(x, r)` | `x.rotate_left(r)` |
| `reinterpret_cast<const uint64_t *>(blocks)[i]` | `u64::from_le_bytes(block.try_into().unwrap())` |
| `switch` with fall-through for the tail | a loop that ORs bytes in, or two loops |

**Port rule:** C++ unsigned wraparound becomes explicit `wrapping_*` operations; casting a byte pointer to a word becomes `from_le_bytes`.

## Learn more

- [`u64::rotate_left`](https://doc.rust-lang.org/std/primitive.u64.html#method.rotate_left) · [`slice::chunks_exact`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_exact)
- [MurmurHash on Wikipedia](https://en.wikipedia.org/wiki/MurmurHash) and Appleby's original <https://github.com/aappleby/smhasher>
