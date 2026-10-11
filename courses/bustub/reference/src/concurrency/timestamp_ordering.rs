//! One item under basic timestamp ordering.

#[derive(Debug, PartialEq, Eq)]
pub struct Abort;

pub struct ToItem {
    // @begin 4a-c5
    value: i64,
    rts: u64,
    wts: u64,
    //~ _item: (),
    // @end
}

impl ToItem {
    /// An item holding `initial` that nobody has read or written.
    pub fn new(initial: i64) -> ToItem {
        // @begin 4a-c5
        ToItem { value: initial, rts: 0, wts: 0 }
        //~ todo!("4a-c5: an item nobody has touched")
        // @end
    }

    /// Reads at timestamp `ts` and records it as the latest read.
    ///
    /// # Errors
    ///
    /// [`Abort`] if `ts` is older than the last write.
    pub fn read(&mut self, ts: u64) -> Result<i64, Abort> {
        // @begin 4a-c5
        if ts < self.wts {
            return Err(Abort);
        }
        self.rts = self.rts.max(ts);
        Ok(self.value)
        //~ todo!("4a-c5: refuse a read that is older than the last write; otherwise note it")
        // @end
    }

    /// Writes `value` at timestamp `ts`.
    ///
    /// # Errors
    ///
    /// [`Abort`] if `ts` is older than the last read or write.
    pub fn write(&mut self, ts: u64, value: i64) -> Result<(), Abort> {
        // @begin 4a-c5
        if ts < self.rts || ts < self.wts {
            return Err(Abort);
        }
        self.wts = ts;
        self.value = value;
        Ok(())
        //~ todo!("4a-c5: refuse a write that is older than a later read or write; otherwise apply it")
        // @end
    }

    /// The latest read timestamp and the latest write timestamp.
    pub fn timestamps(&self) -> (u64, u64) {
        // @begin 4a-c5
        (self.rts, self.wts)
        //~ todo!("4a-c5: (read timestamp, write timestamp)")
        // @end
    }
}
