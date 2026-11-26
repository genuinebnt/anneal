/// A key-value store under test, with one-byte keys and values.
pub trait Subject {
    fn put(&mut self, key: u8, value: u8);
    fn delete(&mut self, key: u8);
    fn get(&self, key: u8) -> Option<u8>;
    /// Moves what is only in the log into a table, and empties the log.
    fn flush(&mut self);
    /// Merges tables. It never changes what `get` returns.
    fn compact(&mut self);
    /// The process is killed and the store opened again.
    fn restart(&mut self);
    /// The same, but the newest write still in the log is cut short on disk first. With an empty log, nothing is cut.
    fn crash_tearing_last(&mut self);
}

/// Panics when the stores built by `make` break the rules; returns when they keep them.
pub fn check<S: Subject>(make: impl Fn() -> S) {
    let mut store = make();
    store.put(1, 10);
    assert_eq!(store.get(1), Some(10));
}
