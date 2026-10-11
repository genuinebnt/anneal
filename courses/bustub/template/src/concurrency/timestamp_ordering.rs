//! One item under basic timestamp ordering.

#[derive(Debug, PartialEq, Eq)]
pub struct Abort;

pub struct ToItem {
    _item: (),
}

impl ToItem {
    /// An item holding `initial` that nobody has read or written.
    pub fn new(initial: i64) -> ToItem {
        todo!("4a-c5: an item nobody has touched")
    }

    /// Reads at timestamp `ts` and records it as the latest read.
    ///
    /// # Errors
    ///
    /// [`Abort`] if `ts` is older than the last write.
    pub fn read(&mut self, ts: u64) -> Result<i64, Abort> {
        todo!("4a-c5: refuse a read that is older than the last write; otherwise note it")
    }

    /// Writes `value` at timestamp `ts`.
    ///
    /// # Errors
    ///
    /// [`Abort`] if `ts` is older than the last read or write.
    pub fn write(&mut self, ts: u64, value: i64) -> Result<(), Abort> {
        todo!("4a-c5: refuse a write that is older than a later read or write; otherwise apply it")
    }

    /// The latest read timestamp and the latest write timestamp.
    pub fn timestamps(&self) -> (u64, u64) {
        todo!("4a-c5: (read timestamp, write timestamp)")
    }
}
