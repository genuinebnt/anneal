/// The remote store. Each call sends one whole list.
pub trait Store {
    /// The list saved under `key`; empty when nothing was saved there.
    fn load(&mut self, key: &str) -> Vec<(String, u64)>;
    /// Replaces the list under `key`. At most `k` pairs, no name twice, only real players.
    fn save(&mut self, key: &str, list: Vec<(String, u64)>);
}

/// Puts one player into the list of one beginning. The list is saved only when it changes.
fn offer(store: &mut dyn Store, k: usize, beginning: &str, name: &str, score: u64) {
    let mut list = store.load(beginning);
    if let Some(at) = list.iter().position(|(other, _)| other == name) {
        if list[at].1 >= score {
            return;
        }
        list.remove(at);
    }
    // Best first: a higher score, then the smaller name. A player who is not in a full list can only enter it by
    // beating its last pair, and an old, lower score never does.
    let at = list.partition_point(|(other, theirs)| {
        *theirs > score || (*theirs == score && other.as_str() < name)
    });
    if at < k {
        list.insert(at, (name.to_string(), score));
        list.truncate(k);
        store.save(beginning, list);
    }
}

/// Applies the pairs in order. A score only ever goes up.
pub fn raise(store: &mut dyn Store, k: usize, changes: &[(&str, u64)]) {
    for &(name, score) in changes {
        // the empty beginning, then each longer one up to the whole name
        let ends = name.char_indices().map(|(at, _)| at);
        for end in ends {
            offer(store, k, &name[..end], name, score);
        }
    }
}

/// At most `k` names that start with `beginning`, best first.
pub fn best(store: &mut dyn Store, _k: usize, beginning: &str) -> Vec<String> {
    store
        .load(beginning)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}
