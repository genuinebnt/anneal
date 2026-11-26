//! An LSM-tree-lite key-value store: write-ahead log, memtable, immutable sorted tables with bloom filters, full compaction.
//!
//! Split from the single-file capstone `cookbook/examples/capstones/mini_kv_store.rs`. The behaviour is meant to be the same,
//! including the limitations that remain, which `tests/limitations.rs` pins down.

mod bloom;
mod codec;
#[cfg(test)]
mod faults;
mod merge;
mod sstable;
mod store;
mod wal;

pub use bloom::Bloom;
pub use codec::{crc32, decode_entry, encode_entry, Entry, Key, KvError, Result, Val};
pub use merge::merge_sources;
pub use sstable::{SsTable, FOOTER, INDEX_EVERY};
pub use store::{Stats, Store};
