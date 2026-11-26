use std::collections::BTreeMap;

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

/// What the store must answer: what is in tables, and the writes that are only in the log.
#[derive(Default)]
struct Model {
    flushed: BTreeMap<u8, u8>,
    log: Vec<(u8, Option<u8>)>,
}

impl Model {
    fn contents(&self) -> BTreeMap<u8, u8> {
        let mut all = self.flushed.clone();
        for &(key, value) in &self.log {
            match value {
                Some(value) => all.insert(key, value),
                None => all.remove(&key),
            };
        }
        all
    }
}

fn compare<S: Subject>(store: &S, model: &Model, keys: u8) {
    let want = model.contents();
    for key in 0..keys {
        assert_eq!(store.get(key), want.get(&key).copied(), "get {key}");
    }
}

/// Panics when the stores built by `make` break the rules; returns when they keep them.
pub fn check<S: Subject>(make: impl Fn() -> S) {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut below = move |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % n
    };
    for round in 0..300u64 {
        // few keys, so that they are overwritten and deleted across many tables
        let keys = [2u8, 3, 6, 12][(round % 4) as usize];
        // some histories flush often, others let the log grow long
        let flush_weight = [1, 4, 12][(round % 3) as usize];
        let mut store = make();
        let mut model = Model::default();
        for _ in 0..150 {
            let key = below(u64::from(keys)) as u8;
            let choice = below(40 + flush_weight);
            match choice {
                0..=13 => {
                    let value = below(256) as u8;
                    store.put(key, value);
                    model.log.push((key, Some(value)));
                }
                14..=21 => {
                    store.delete(key);
                    model.log.push((key, None));
                }
                22..=27 => store.compact(),
                28..=33 => store.restart(),
                34..=39 => {
                    store.crash_tearing_last();
                    model.log.pop();
                }
                _ => {
                    store.flush();
                    model.flushed = model.contents();
                    model.log.clear();
                }
            }
            compare(&store, &model, keys);
        }
    }
}
