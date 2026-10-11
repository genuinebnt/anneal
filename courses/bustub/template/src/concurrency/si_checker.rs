//! Checking a history against snapshot isolation.

#[derive(Debug, Clone)]
pub struct Txn {
    pub id: u32,
    pub start: u64,
    pub commit: u64,
    pub reads: Vec<(i64, i64)>,
    pub writes: Vec<(i64, i64)>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    StaleRead { txn: u32, key: i64 },
    LostUpdate { a: u32, b: u32, key: i64 },
}

/// Checks a history against snapshot isolation: every read sees the snapshot as of its transaction's start, and no two overlapping
/// transactions wrote the same key.
///
/// # Errors
///
/// The first [`Violation`] found.
pub fn check_si(history: &[Txn]) -> Result<(), Violation> {
    todo!("4b-c2: check every read against the snapshot at its start, then overlapping writers of the same key")
}
