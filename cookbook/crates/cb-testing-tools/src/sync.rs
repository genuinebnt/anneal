//! The standard loom "cfg facade": library code imports its sync primitives from here, so
//! `RUSTFLAGS="--cfg loom" cargo test` swaps std's atomics for loom's model-checked ones
//! without touching the algorithm. Without the cfg this is a zero-cost re-export of std.
#[cfg(loom)]
pub use loom::sync::Arc;
#[cfg(loom)]
pub use loom::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(loom))]
pub use std::sync::Arc;
#[cfg(not(loom))]
pub use std::sync::atomic::{AtomicUsize, Ordering};
