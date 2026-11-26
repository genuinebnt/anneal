//! `#![no_std]`: only `core` is linked implicitly. `alloc` (Vec, String, Box)
//! needs a global allocator and is opt-in; `std` (files, threads, HashMap)
//! needs an OS. Everything below except `to_vec`/`Error` impls uses core only.
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;
// Re-enable std only when asked. Note this doesn't change that the crate root is no_std.
#[cfg(feature = "std")]
extern crate std;

pub mod arena;
pub mod fmtbuf;
pub mod parse;
pub mod ring;
pub mod stack;

/// Error for capacity problems; `heapless` returns the rejected value instead
/// (see `push`), so no allocation or formatting is needed to report it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct CapacityError;

impl core::fmt::Display for CapacityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("capacity exceeded")
    }
}
// `core::error::Error` is stable in core, but we keep the std flavour to show the feature.
#[cfg(feature = "std")]
impl std::error::Error for CapacityError {}
