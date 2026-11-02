A C library gives you a struct and you must exchange it as raw bytes. Given
`struct header { uint8_t tag; uint32_t len; uint16_t flags; }` as the `#[repr(C)] Header` below:

- `layout()` returns `(size, align, offsets)` as the compiler computes them (use `size_of`, `align_of`, `offset_of!`).
- `to_bytes` writes the 12 bytes of its C image in native byte order, with the padding bytes **zero**.
- `from_bytes` reads a `Header` back from at least 12 bytes at *any* address (a copy).
- `view` returns a `&Header` pointing straight into the slice (no copy) but only if it is long enough **and** the slice's
  address is a multiple of `Header`'s alignment; otherwise `None`.
