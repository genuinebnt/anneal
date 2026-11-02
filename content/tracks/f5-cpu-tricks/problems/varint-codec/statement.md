Write the integer codec every binary protocol has.

- `encode_u64` / `decode_u64`: LEB128. `decode_u64` returns the value and how many bytes it consumed, accepts
  non-minimal encodings, and rejects a number that does not fit in 64 bits (`Overflow`) or an input that stops in
  the middle of one (`Truncated`).
- `zigzag` / `unzigzag` and `encode_i64` / `decode_i64`: signed values, small magnitudes stay short.
- `encode_sorted` / `decode_sorted`: a non-decreasing list as `count, first, delta, delta, ...`, the trick behind
  posting lists in search engines and sorted columns. Unsorted input is `Err(NotSorted { index })`.

The decoder reads bytes from the network: it must never panic, loop forever or allocate by a length it has not
verified.
