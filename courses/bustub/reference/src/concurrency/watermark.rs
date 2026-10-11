//! The **watermark**: the smallest read timestamp of the running transactions, or the latest commit timestamp when none runs.
//!
//! No transaction can need a version older than the newest one at or below the watermark, so everything older is garbage.

use std::collections::BTreeMap;

use super::transaction::Timestamp;
use crate::common::exception::{Exception, ExceptionType, Result};

pub struct Watermark {
    /// The timestamp of the last commit.
    commit_ts: Timestamp,
    /// The read timestamps of the running transactions, each with how many have it.
    current_reads: BTreeMap<Timestamp, usize>,
}

impl Watermark {
    pub fn new(commit_ts: Timestamp) -> Watermark {
        Watermark { commit_ts, current_reads: BTreeMap::new() }
    }

    /// A transaction with this read timestamp begins. A read timestamp older than the last commit is an error ("read ts < commit ts").
    pub fn add_txn(&mut self, read_ts: Timestamp) -> Result<()> {
        // @begin 4a-01
        if read_ts < self.commit_ts {
            return Err(Exception::new(ExceptionType::Execution, "read ts < commit ts"));
        }
        *self.current_reads.entry(read_ts).or_insert(0) += 1;
        Ok(())
        //~ todo!("4a-01: an error if read_ts < self.commit_ts; otherwise count one more transaction reading at read_ts")
        // @end
    }

    /// A transaction with this read timestamp commits or aborts.
    pub fn remove_txn(&mut self, read_ts: Timestamp) {
        // @begin 4a-01
        if let Some(count) = self.current_reads.get_mut(&read_ts) {
            *count -= 1;
            if *count == 0 {
                self.current_reads.remove(&read_ts);
            }
        }
        //~ todo!("4a-01: count one transaction less at read_ts, forgetting the timestamp when none is left")
        // @end
    }

    /// Records that a transaction committed at `commit_ts`.
    pub fn update_commit_ts(&mut self, commit_ts: Timestamp) {
        self.commit_ts = commit_ts;
    }

    /// The smallest read timestamp among the running transactions; the last commit timestamp if there are none.
    pub fn get_watermark(&self) -> Timestamp {
        // @begin 4a-01
        self.current_reads.keys().next().copied().unwrap_or(self.commit_ts)
        //~ todo!("4a-01: the smallest timestamp somebody reads at, or commit_ts when nobody runs")
        // @end
    }
}
