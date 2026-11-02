A WebAssembly module is a binary file: the 8-byte header `00 61 73 6D` (`\0asm`) `01 00 00 00`, then **sections**, each
`[id: u8][size: u32 LEB128][payload of size bytes]`. Write the decoder real engines and tools (`wasmparser`, `wasmtime`,
`wasm-tools`) start with.

`read_u32` decodes one **LEB128** `u32` (7 bits per byte, least significant group first, bit 7 means another byte follows):
at most five bytes, and in the fifth only the low four bits may be set (anything else is `BadLeb`). Padded encodings such
as `80 00` for 0 are legal. Running out of bytes is `Truncated`.

`decode` builds a `Module` from sections `1` (types: `0x60`, parameter list, result list), `3` (functions: a type index each),
`7` (exports: name, kind `0..=3`, index) and `10` (code: per function a size, runs of `(count, type)` locals, then the
instruction bytes, which must end with `0x0B`). Sections `2, 4, 5, 6, 8, 9, 11` are valid but ignored (skip them by their
size). Section `0` is a custom section (a name, then raw bytes) and may appear anywhere, any number of times. Every other
section may appear at most once and in increasing id order (`SectionOutOfOrder`). Any other id is `BadSection(id)`.

Each section is read **inside its own payload**: ending early is `Truncated`, and leaving bytes unread is `SectionSize`.
The function and code sections must have the same length (`CountMismatch`), and a function may declare at most 50 000
locals in total (`TooManyLocals`).
