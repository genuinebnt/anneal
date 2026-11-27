/// The remote store. Each call sends one whole list.
pub trait Store {
    /// The list saved under `key`; empty when nothing was saved there.
    fn load(&mut self, key: &str) -> Vec<(String, u64)>;
    /// Replaces the list under `key`. At most `k` pairs, no name twice, only real players.
    fn save(&mut self, key: &str, list: Vec<(String, u64)>);
}

/// Applies the pairs in order. A score only ever goes up.
pub fn raise(store: &mut dyn Store, k: usize, changes: &[(&str, u64)]) {
    todo!()
}

/// At most `k` names that start with `beginning`, best first.
pub fn best(store: &mut dyn Store, k: usize, beginning: &str) -> Vec<String> {
    todo!()
}
