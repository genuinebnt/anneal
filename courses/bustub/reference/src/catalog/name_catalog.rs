//! Table names and ids.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub enum NameError {
    Exists,
    Missing,
    Invalid,
}

pub struct NameCatalog {
    // @begin 3c-c2
    by_name: BTreeMap<String, u32>,
    next_id: u32,
    //~ _names: (),
    // @end
}

impl NameCatalog {
    /// An empty catalog. Ids start at 1.
    pub fn new() -> NameCatalog {
        // @begin 3c-c2
        NameCatalog { by_name: BTreeMap::new(), next_id: 1 }
        //~ todo!("3c-c2: no tables; ids start at 1")
        // @end
    }

    /// Registers `name` and returns its new id. Names compare case-insensitively.
    ///
    /// # Errors
    ///
    /// [`NameError::Invalid`] for an empty name, [`NameError::Exists`] if the name is taken.
    pub fn create(&mut self, name: &str) -> Result<u32, NameError> {
        // @begin 3c-c2
        if name.is_empty() {
            return Err(NameError::Invalid);
        }
        let key = name.to_ascii_lowercase();
        if self.by_name.contains_key(&key) {
            return Err(NameError::Exists);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.by_name.insert(key, id);
        Ok(id)
        //~ todo!("3c-c2: a new id for a new name")
        // @end
    }

    /// The id of the table `name`, if there is one.
    pub fn lookup(&self, name: &str) -> Option<u32> {
        // @begin 3c-c2
        self.by_name.get(&name.to_ascii_lowercase()).copied()
        //~ todo!("3c-c2: the id of the table")
        // @end
    }

    /// Forgets `name`; returns whether it existed. Its id is never reused.
    pub fn drop(&mut self, name: &str) -> bool {
        // @begin 3c-c2
        self.by_name.remove(&name.to_ascii_lowercase()).is_some()
        //~ todo!("3c-c2: forget the name (the id is not reused)")
        // @end
    }

    /// Gives the table `old` the name `new`, keeping its id. Renaming to the same name (in another case) is allowed.
    ///
    /// # Errors
    ///
    /// [`NameError::Invalid`] if `new` is empty, [`NameError::Missing`] if `old` does not exist, [`NameError::Exists`] if `new` is taken.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), NameError> {
        // @begin 3c-c2
        if new.is_empty() {
            return Err(NameError::Invalid);
        }
        let (old, new) = (old.to_ascii_lowercase(), new.to_ascii_lowercase());
        if !self.by_name.contains_key(&old) {
            return Err(NameError::Missing);
        }
        if old != new && self.by_name.contains_key(&new) {
            return Err(NameError::Exists);
        }
        let id = self.by_name.remove(&old).unwrap();
        self.by_name.insert(new, id);
        Ok(())
        //~ todo!("3c-c2: move the name, keep the id")
        // @end
    }

    /// The names, lowercased, in sorted order.
    pub fn names(&self) -> Vec<String> {
        // @begin 3c-c2
        self.by_name.keys().cloned().collect()
        //~ todo!("3c-c2: lowercase names, sorted")
        // @end
    }
}

impl Default for NameCatalog {
    fn default() -> Self {
        NameCatalog::new()
    }
}
