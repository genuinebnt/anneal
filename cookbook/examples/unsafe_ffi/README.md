# Unsafe and FFI

Raw pointers, MaybeUninit, UnsafeCell, NonNull, Pin, layout, and calling C. Every unsafe block states its safety argument.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

7 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`unsafe_basics`](unsafe_basics.rs) | unsafe basics: the five superpowers, safe wrappers, and how to test them | intermediate | slice/Vec internals in std (`split_at_mut`, `swap`), FFI wrappers around C libraries, SIMD and | std only |
| [`maybeuninit_fixedvec`](maybeuninit_fixedvec.rs) | MaybeUninit and ManuallyDrop: an inline, heap-free `FixedVec<T, N>` | advanced | small-vector crates (smallvec/arrayvec style), ring buffers on embedded targets, arena slots, | std only |
| [`unsafecell_cells`](unsafecell_cells.rs) | UnsafeCell: build your own `Cell` and `RefCell` (interior mutability from first principles) | advanced | shared mutable state in single-threaded graphs/trees (Rc<RefCell<_>>), caches behind `&self`, | std only |
| [`nonnull_linked_list`](nonnull_linked_list.rs) | a doubly linked `Deque<T>` with `NonNull`, `Box::into_raw` and `PhantomData` | advanced | LRU caches (O(1) unlink), task/timer queues, std::collections::LinkedList, intrusive lists in kernels. | std only |
| [`pin_projection`](pin_projection.rs) | Pin, Unpin, PhantomPinned and structural pin projection (a hand-made `Future::map`) | advanced | tokio/futures combinators (`Map`, `Then`, `Select`), intrusive linked lists whose nodes are pinned | std only |
| [`repr_layout_ffi`](repr_layout_ffi.rs) | repr(transparent/u8/C/packed), offset_of!, extern "C" fn pointers, CStr, and a binary round trip | intermediate | wrapping C library handles, reading on-disk headers and network packet formats, passing callbacks to C APIs, | std only |
| [`ffi_c_interop`](ffi_c_interop.rs) | C interop both ways with only `libc`: extern "C" calls, callbacks, opaque handles, CString ownership, panic safety | advanced | sys crates (`libz-sys`, `openssl-sys` style) wrapped by safe crates, Rust cores embedded in Python/Node/Swift apps via a C ABI, | libc |
