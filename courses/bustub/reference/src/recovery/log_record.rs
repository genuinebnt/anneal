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
        // @begin 4c-01
        let mut body = Vec::new();
        match self {
            LogRecord::Begin { txn } => {
                body.push(1);
                body.extend_from_slice(&txn.to_le_bytes());
            }
            LogRecord::Change { txn, rid, before, after } => {
                body.push(2);
                body.extend_from_slice(&txn.to_le_bytes());
                body.extend_from_slice(&rid.page_id().0.to_le_bytes());
                body.extend_from_slice(&rid.slot_num().to_le_bytes());
                for state in [before, after] {
                    match state {
                        None => body.push(0),
                        Some(bytes) => {
                            body.push(1);
                            body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                            body.extend_from_slice(bytes);
                        }
                    }
                }
            }
            LogRecord::Commit { txn } => {
                body.push(3);
                body.extend_from_slice(&txn.to_le_bytes());
            }
            LogRecord::Abort { txn } => {
                body.push(4);
                body.extend_from_slice(&txn.to_le_bytes());
            }
            LogRecord::Checkpoint { active } => {
                body.push(5);
                body.extend_from_slice(&(active.len() as u32).to_le_bytes());
                for t in active {
                    body.extend_from_slice(&t.to_le_bytes());
                }
            }
        }
        let mut out = Vec::with_capacity(8 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum(&body).to_le_bytes());
        out.extend_from_slice(&body);
        out
        //~ todo!("4c-01: a length, a checksum of the body, and the body (a kind byte and the fields)")
        // @end
    }

    /// Reads one record from the start of `bytes`: the record and how many bytes it took. `None` if the bytes are too short for a whole
    /// record, or do not check out (a torn or damaged record: the log ends here).
    pub fn deserialize(bytes: &[u8]) -> Option<(LogRecord, usize)> {
        // @begin 4c-01
        let len = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
        let sum = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
        let body = bytes.get(8..8usize.checked_add(len)?)?;
        if checksum(body) != sum {
            return None;
        }
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> {
            let slice = body.get(at..at.checked_add(n)?)?;
            at += n;
            Some(slice)
        };
        let u64_at = |s: &[u8]| u64::from_le_bytes(s.try_into().unwrap());
        let record = match take(1)?[0] {
            1 => LogRecord::Begin { txn: u64_at(take(8)?) },
            2 => {
                let txn = u64_at(take(8)?);
                let page = i32::from_le_bytes(take(4)?.try_into().unwrap());
                let slot = u32::from_le_bytes(take(4)?.try_into().unwrap());
                let mut states = [None, None];
                for state in states.iter_mut() {
                    *state = match take(1)?[0] {
                        0 => None,
                        1 => {
                            let n = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                            Some(take(n)?.to_vec())
                        }
                        _ => return None,
                    };
                }
                let [before, after] = states;
                LogRecord::Change { txn, rid: Rid::new(crate::common::config::PageId(page), slot), before, after }
            }
            3 => LogRecord::Commit { txn: u64_at(take(8)?) },
            4 => LogRecord::Abort { txn: u64_at(take(8)?) },
            5 => {
                let n = u32::from_le_bytes(take(4)?.try_into().unwrap()) as usize;
                let mut active = Vec::new();
                for _ in 0..n {
                    active.push(u64_at(take(8)?));
                }
                LogRecord::Checkpoint { active }
            }
            _ => return None,
        };
        if at != body.len() {
            return None;
        }
        Some((record, 8 + len))
        //~ todo!("4c-01: None unless the length, the checksum and the kind all check out; else the record and the bytes it took")
        // @end
    }
}

// @begin 4c-01
/// FNV-1a over the body: cheap and good enough to notice a cut or a flipped bit.
fn checksum(body: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in body {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}
//~ // TODO(4c-01): helpers of your own (a checksum, a small reader for the fields)
// @end

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
