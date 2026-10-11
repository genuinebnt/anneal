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
    // @begin 4b-c2
    let mut order: Vec<&Txn> = history.iter().collect();
    order.sort_by_key(|t| t.commit);
    for (i, t) in order.iter().enumerate() {
        for &(key, value) in &t.reads {
            // the snapshot value: the latest write committed at or before `start` (the sort is by commit time)
            let snapshot = order
                .iter()
                .filter(|o| o.commit <= t.start && o.id != t.id)
                .rev()
                .find_map(|o| o.writes.iter().rev().find(|w| w.0 == key).map(|w| w.1))
                .unwrap_or(0);
            if snapshot != value {
                return Err(Violation::StaleRead { txn: t.id, key });
            }
        }
        for later in &order[i + 1..] {
            // `later` committed after `t`: they overlap when `later` started before `t` committed
            if later.start < t.commit {
                if let Some(&(key, _)) = t.writes.iter().find(|w| later.writes.iter().any(|x| x.0 == w.0)) {
                    return Err(Violation::LostUpdate { a: t.id, b: later.id, key });
                }
            }
        }
    }
    Ok(())
    //~ todo!("4b-c2: check every read against the snapshot at its start, then overlapping writers of the same key")
    // @end
}
