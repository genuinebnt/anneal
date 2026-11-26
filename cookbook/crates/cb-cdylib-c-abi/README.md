# cb-cdylib-c-abi: exposing Rust through a stable C ABI

`crate-type = ["rlib", "cdylib", "staticlib"]` produces `libcb_cdylib_c_abi.{dylib,so}`
(dynamic) and `.a` (static). `include/cb_c_abi.h` is hand-written; `tests/c/client.c`
is a C program that `tests/c_client.rs` compiles with `cc`, links both ways, runs,
and checks stdout and exit code.

## Rules shown in `src/lib.rs`
- `#[unsafe(no_mangle)] pub extern "C" fn` (edition 2024 spelling) gives an
  unmangled C symbol with the C calling convention.
- **No Rust types across the boundary**: no `String`, `Vec`, `&str`, `Option<Box<T>>`
  (except niche-guaranteed pointer forms), `Result`, trait objects. Use `repr(C)`
  structs, primitives, raw pointers, `Option<extern "C" fn>`.
- **Opaque handles**: `Box::into_raw` in `*_new`, `Box::from_raw` in `*_free`; C
  sees only `typedef struct cb_counter cb_counter;`. Free functions accept NULL.
- **Errors**: integer codes (0 ok, negative error) plus a thread-local
  `cb_last_error_message()` (like `errno`/`GetLastError`).
- **Callbacks** take `void *user_data` that Rust just passes back, so C can
  smuggle in its own state (no closures cross the ABI).
- **Panics**: unwinding out of `extern "C"` aborts (or is UB in older ABIs). Wrap
  bodies in `catch_unwind` and map to `CB_ERR_PANIC`. Requires `panic = "unwind"`.
- **Ownership**: `cb_greet` returns Rust-allocated memory that must go back via
  `cb_string_free`, never `free()` (different allocators). Input strings are
  borrowed for the call only. Caller-owned buffers use a `cap`/`written` pair and
  report the required size.
- **Stability**: `CB_ABI_VERSION`/`cb_abi_version()`, size-prefixed structs
  (`struct_size`), `const _: () = assert!(size_of::<Point>() == 12)` plus a C
  `_Static_assert` so layout drift fails the build, never append to enums
  silently, never remove symbols.

## Generating the header
Instead of writing by hand use [cbindgen](https://github.com/mozilla/cbindgen)
(`cbindgen --crate cb-cdylib-c-abi -o include/cb_c_abi.h`, or from `build.rs`).
Going the other way (C headers to Rust) is `bindgen`.

## Run
`CARGO_TARGET_DIR=/tmp/cb_target_shared cargo test -q --release -p cb-cdylib-c-abi`
