use std::collections::HashMap;

pub struct Cache {
    map: HashMap<u32, String>,
    order: Vec<u32>,
    misses: usize,
}

impl Cache {
    pub fn new() -> Self {
        Cache {
            map: HashMap::new(),
            order: Vec::new(),
            misses: 0,
        }
    }

    /// Keys in the order they were first stored.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    pub fn misses(&self) -> usize {
        self.misses
    }

    /// The value for `key`. If it's missing, counts a miss, records the key's order, and stores `make()`.
    /// `make` runs only on a miss.
    pub fn get_or_make(&mut self, key: u32, make: impl FnOnce() -> String) -> &String {
        if !self.map.contains_key(&key) {
            self.misses += 1;
            self.order.push(key);
            self.map.insert(key, make());
        }
        &self.map[&key]
    }

    /// Like `get_or_make`, for editing the value in place.
    pub fn get_or_make_mut(&mut self, key: u32, make: impl FnOnce() -> String) -> &mut String {
        if !self.map.contains_key(&key) {
            self.misses += 1;
            self.order.push(key);
            self.map.insert(key, make());
        }
        self.map.get_mut(&key).unwrap()
    }
}
