use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    Corrupt(String),
    Missing(String),
}

pub struct Store {
    data: HashMap<String, String>,
}

impl Store {
    pub fn new(pairs: &[(&str, &str)]) -> Store {
        Store {
            data: pairs
                .iter()
                .map(|&(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// `Ok(None)` if `key` is absent, `Err(Corrupt(key))` if its value isn't an integer.
    pub fn get(&self, key: &str) -> Result<Option<i64>, StoreError> {
        match self.data.get(key) {
            None => Ok(None),
            Some(v) => v
                .parse()
                .map(Some)
                .map_err(|_| StoreError::Corrupt(key.to_string())),
        }
    }
}

pub fn require(store: &Store, key: &str) -> Result<i64, StoreError> {
    todo!()
}

pub fn sum_or_zero(store: &Store, keys: &[&str]) -> Result<i64, StoreError> {
    todo!()
}

pub fn first_corrupt(store: &Store, keys: &[&str]) -> Option<StoreError> {
    todo!()
}
