Build the scalar core of `memchr`: find, find-last and count a byte in a slice while examining eight bytes at a time.

`splat(b)` repeats a byte across a `u64`. `zero_bytes(w)` returns a mask with the high bit (`0x80`) set in exactly the
lanes of `w` that are zero. It has to be **exact in every lane**: the classic shortcut `(w - 0x0101..) & !w & 0x8080..`
can also flag a `0x01` lane that sits above a real zero lane (the borrow ripples up), which is harmless when you
only want the first hit and wrong when you count. Use a formulation whose lanes never interact.

`find_byte`, `rfind_byte` and `count_byte` XOR each little-endian word with `splat(needle)` (matching lanes become
zero), use `zero_bytes`, and handle the tail shorter than eight bytes. Slices of every length and starting offset
must work.
