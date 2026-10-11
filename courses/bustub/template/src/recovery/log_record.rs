//! What the write-ahead log says, one record at a time.
//!
//! A record is a transaction beginning, a **change** of one record slot (its state before and after: `None` means the slot holds no live record), a
//! commit, an abort (written once a rollback has been logged to the end) or a checkpoint. The byte format is free: all that is required is that
//! `deserialize(serialize(r))` gives `r` back and that a damaged or cut-off record is recognised as such.

use crate::common::rid::Rid;

pub type TxnId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogRecord {
    Begin { txn: TxnId },
    /// The slot `rid` goes from the state `before` to the state `after` (`None`: no live record). An insert is `None -> Some`, an update
    /// `Some -> Some`, a delete `Some -> None`, and the undo of any of them is the same record with the two states swapped.
    Change { txn: TxnId, rid: Rid, before: Option<Vec<u8>>, after: Option<Vec<u8>> },
    Commit { txn: TxnId },
    /// The transaction was rolled back: every undo of its changes is in the log before this record.
    Abort { txn: TxnId },
    /// Every page was written to disk when this was logged; `active` are the transactions that had begun and not finished.
    Checkpoint { active: Vec<TxnId> },
}

impl LogRecord {
    /// The transaction a record belongs to, if it belongs to one.
    pub fn txn(&self) -> Option<TxnId> {
        match self {
            LogRecord::Begin { txn } | LogRecord::Change { txn, .. } | LogRecord::Commit { txn } | LogRecord::Abort { txn } => Some(*txn),
            LogRecord::Checkpoint { .. } => None,
        }
    }

    /// The bytes of the record: enough to read it back and to notice that it was cut off or damaged.
    pub fn serialize(&self) -> Vec<u8> {
        todo!("4c-01: a length, a checksum of the body, and the body (a kind byte and the fields)")
    }

    /// Reads one record from the start of `bytes`: the record and how many bytes it took. `None` if the bytes are too short for a whole
    /// record, or do not check out (a torn or damaged record: the log ends here).
    pub fn deserialize(bytes: &[u8]) -> Option<(LogRecord, usize)> {
        todo!("4c-01: None unless the length, the checksum and the kind all check out; else the record and the bytes it took")
    }
}

// TODO(4c-01): helpers of your own (a checksum, a small reader for the fields)

/// Every record at the start of `bytes`, in order, with the offset where each begins; it stops at the first record that is cut off or does not
/// check out. Also the number of bytes those records take. This is what "the log" means after a crash.
pub fn parse_log(bytes: &[u8]) -> (Vec<(u64, LogRecord)>, usize) {
    let (mut out, mut at) = (Vec::new(), 0usize);
    while let Some((record, n)) = LogRecord::deserialize(&bytes[at..]) {
        out.push((at as u64, record));
        at += n;
    }
    (out, at)
}
