A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`encode_keys` and `decode_keys` in `src/storage/page/key_block.rs`: a block of **sorted** byte-string keys stored as, for each key, how many leading bytes it shares with the previous key, and then the rest. Decoding must reverse it exactly and reject malformed bytes without panicking.

## Why

Index keys are often long and alike (`/users/42/orders/1`, `/users/42/orders/2`), and an inner page that fits twice the keys is a shallower tree. Prefix compression is the standard answer. The real work is the decoder: it reads bytes from a disk, and a corrupt page must be an error, not a crash.

## The contract

- `encode_keys(keys)`: `keys` are sorted, each at most 255 bytes. The encoding is a count (1 byte) then, for each key, `shared` (1 byte, the length of the common prefix with the previous key; 0 for the first), `rest_len` (1 byte), and the rest.
- `decode_keys(bytes)` returns the keys, or `None` if the bytes are not exactly a well-formed block (truncated, `shared` longer than the previous key, trailing bytes).

## Invariants

These must hold after every step, whatever the input:

- `decode_keys(encode_keys(k)) == Some(k)` for every valid sorted `k`.
- The encoded length is `1 + sum(2 + (len(key) - shared_with_previous))`.
- Decoding never panics, on any bytes.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Keys that share long prefixes encode smaller than keys that do not.
- Any prefix of a valid block (cut short) is rejected.
- Decoding then encoding any accepted bytes gives the same bytes.

## Examples

Worked cases (the tests include them):

```text
["apple", "apply", "banana"] -> 3 | 0,5,apple | 4,1,y | 0,6,banana
```

## What the tests check

- Exact bytes for a small block.
- Round trips; the size formula.
- Malformed input: truncation, bad `shared`, trailing bytes.
- A property over random bytes: no panic and re-encoding agrees.

## Done when

All the `s2a_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Production B+tree features](/cookbook/p/btree-bulk-load-prefix-compression): prefix compression of sorted keys in B+ tree leaves.
