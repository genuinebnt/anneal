/// A large, sorted, read-only set of entries kept elsewhere. Every call is a round trip.
pub trait Catalogue {
    fn get(&self, key: &str) -> Option<String>;
    /// The entry with the smallest key that is `key` or later.
    fn at_or_after(&self, key: &str) -> Option<(String, String)>;
    /// The entry with the smallest key later than `key`.
    fn after(&self, key: &str) -> Option<(String, String)>;
}

/// Changes to a catalogue that have not been sent to it yet.
pub struct Draft<C: Catalogue> {
    base: C,
}

impl<C: Catalogue> Draft<C> {
    pub fn new(base: C) -> Draft<C> {
        todo!()
    }

    pub fn set(&mut self, key: &str, value: &str) {
        todo!()
    }

    pub fn delete(&mut self, key: &str) {
        todo!()
    }

    /// Forgets the unsaved change to `key`, if there is one.
    pub fn revert(&mut self, key: &str) {
        todo!()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        todo!()
    }

    /// Up to `limit` entries as the catalogue would look after saving, from key `from` on, in key order.
    pub fn page(&self, from: &str, limit: usize) -> Vec<(String, String)> {
        todo!()
    }

    /// What saving would send, in key order: a new value, or `None` for a delete.
    pub fn changes(&self) -> Vec<(String, Option<String>)> {
        todo!()
    }
}
