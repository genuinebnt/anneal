//! Which transaction of a deadlock cycle is aborted.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxnInfo {
    pub id: u32,
    pub start_ts: u64,
    pub locks: usize,
    pub work: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VictimPolicy {
    Youngest,
    FewestLocks,
    LeastWork,
}

/// The index in `cycle` of the transaction to abort under `policy`; ties go to the larger id. `None` for an empty cycle.
pub fn choose_victim(policy: VictimPolicy, cycle: &[TxnInfo]) -> Option<usize> {
    todo!("4d-c3: the best candidate by the policy's attribute; the larger id on ties")
}
