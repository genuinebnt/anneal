use std::collections::BTreeMap;
use std::fmt::Display;

pub trait Store {
    fn get(&self, key: &str) -> Option<String>;
    /// Stores `value` formatted with Display.
    fn put(&mut self, key: &str, value: &dyn Display);
    /// The keys in sorted order.
    fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_>;
}

#[derive(Default)]
pub struct MemStore {
    map: BTreeMap<String, String>,
}

impl Store for MemStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }

    fn put(&mut self, key: &str, value: &dyn Display) {
        self.map.insert(key.to_string(), value.to_string());
    }

    fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.map.keys().map(String::as_str))
    }
}

/// Stores values uppercased.
#[derive(Default)]
pub struct UpperStore {
    map: BTreeMap<String, String>,
}

impl Store for UpperStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }

    fn put(&mut self, key: &str, value: &dyn Display) {
        self.map
            .insert(key.to_string(), value.to_string().to_uppercase());
    }

    fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.map.keys().map(String::as_str))
    }
}

/// A view of `inner` under a key prefix: key "k" here is "<prefix>k" in `inner`.
pub struct Prefixed {
    pub prefix: String,
    pub inner: Box<dyn Store>,
}

impl Store for Prefixed {
    fn get(&self, key: &str) -> Option<String> {
        self.inner.get(&format!("{}{}", self.prefix, key))
    }

    fn put(&mut self, key: &str, value: &dyn Display) {
        self.inner.put(&format!("{}{}", self.prefix, key), value);
    }

    /// Only the inner keys under the prefix, with the prefix removed.
    fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(
            self.inner
                .keys()
                .filter_map(move |k| k.strip_prefix(self.prefix.as_str())),
        )
    }
}

/// One of each basic store.
pub fn all_stores() -> Vec<Box<dyn Store>> {
    vec![
        Box::new(MemStore::default()),
        Box::new(UpperStore::default()),
    ]
}
