//! Table names and ids.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub enum NameError {
    Exists,
    Missing,
    Invalid,
}

pub struct NameCatalog {
    _names: (),
}

impl NameCatalog {
    /// An empty catalog. Ids start at 1.
    pub fn new() -> NameCatalog {
        todo!("3c-c2: no tables; ids start at 1")
    }

    /// Registers `name` and returns its new id. Names compare case-insensitively.
    ///
    /// # Errors
    ///
    /// [`NameError::Invalid`] for an empty name, [`NameError::Exists`] if the name is taken.
    pub fn create(&mut self, name: &str) -> Result<u32, NameError> {
        todo!("3c-c2: a new id for a new name")
    }

    /// The id of the table `name`, if there is one.
    pub fn lookup(&self, name: &str) -> Option<u32> {
        todo!("3c-c2: the id of the table")
    }

    /// Forgets `name`; returns whether it existed. Its id is never reused.
    pub fn drop(&mut self, name: &str) -> bool {
        todo!("3c-c2: forget the name (the id is not reused)")
    }

    /// Gives the table `old` the name `new`, keeping its id. Renaming to the same name (in another case) is allowed.
    ///
    /// # Errors
    ///
    /// [`NameError::Invalid`] if `new` is empty, [`NameError::Missing`] if `old` does not exist, [`NameError::Exists`] if `new` is taken.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), NameError> {
        todo!("3c-c2: move the name, keep the id")
    }

    /// The names, lowercased, in sorted order.
    pub fn names(&self) -> Vec<String> {
        todo!("3c-c2: lowercase names, sorted")
    }
}

impl Default for NameCatalog {
    fn default() -> Self {
        NameCatalog::new()
    }
}
