/// The remote store. Each call sends one whole list.
pub trait Store {
    /// The list saved under `key`; empty when nothing was saved there.
    fn load(&mut self, key: &str) -> Vec<(String, u64)>;
    /// Replaces the list under `key`. At most `k` pairs, no name twice, only real players.
    fn save(&mut self, key: &str, list: Vec<(String, u64)>);
}

pub fn raise(store: &mut dyn Store, _k: usize, changes: &[(&str, u64)]) {
    for &(name, score) in changes {
        let mut all = store.load("everyone");
        match all.iter().position(|(n, _)| n == name) {
            Some(at) if all[at].1 >= score => continue,
            Some(at) => {
                all.remove(at);
            }
            None => {}
        }
        let at = all.partition_point(|(n, s)| *s > score || (*s == score && n.as_str() < name));
        all.insert(at, (name.to_string(), score));
        store.save("everyone", all);
    }
}

pub fn best(store: &mut dyn Store, k: usize, beginning: &str) -> Vec<String> {
    store
        .load("everyone")
        .into_iter()
        .filter(|(n, _)| n.starts_with(beginning))
        .take(k)
        .map(|(n, _)| n)
        .collect()
}
