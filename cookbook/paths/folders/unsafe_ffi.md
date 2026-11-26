# Chapter `unsafe_ffi`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`repr_layout_ffi`](../../examples/unsafe_ffi/repr_layout_ffi.rs) | intermediate | repr(transparent/u8/C/packed), offset_of!, extern "C" fn pointers, CStr, and a binary round trip. |
| 2 | [`unsafe_basics`](../../examples/unsafe_ffi/unsafe_basics.rs) | intermediate | unsafe basics: the five superpowers, safe wrappers, and how to test them. |
| 3 | [`ffi_c_interop`](../../examples/unsafe_ffi/ffi_c_interop.rs) | advanced | C interop both ways with only `libc`: extern "C" calls, callbacks, opaque handles, CString ownership, panic safety. |
| 4 | [`maybeuninit_fixedvec`](../../examples/unsafe_ffi/maybeuninit_fixedvec.rs) | advanced | MaybeUninit and ManuallyDrop: an inline, heap-free `FixedVec<T, N>`. |
| 5 | [`nonnull_linked_list`](../../examples/unsafe_ffi/nonnull_linked_list.rs) | advanced | a doubly linked `Deque<T>` with `NonNull`, `Box::into_raw` and `PhantomData`. |
| 6 | [`pin_projection`](../../examples/unsafe_ffi/pin_projection.rs) | advanced | Pin, Unpin, PhantomPinned and structural pin projection (a hand-made `Future::map`). |
| 7 | [`unsafecell_cells`](../../examples/unsafe_ffi/unsafecell_cells.rs) | advanced | UnsafeCell: build your own `Cell` and `RefCell` (interior mutability from first principles). |
