Write the thin safe layer you need around a C library's string functions. Rust links against the C library already, so
`extern "C" { fn strlen(...) }` just works.

- `c_strlen`: build a `CString`, call C's `strlen` on it. An interior NUL byte in the Rust string is an error (`NulError`).
- `from_c`: read a C string that someone else owns into a `String`; a null pointer is `None`; invalid UTF-8 becomes
  U+FFFD instead of failing.
- `make_greeting` / `free_greeting`: produce `"hello, {name}"` for C to hold on to, and take it back. The pointer must be freed
  by Rust's `CString::from_raw`, not by `free`.
- `c_prefix_len`: the length of the C string at the start of a byte buffer, or `None` if there is no NUL terminator.
