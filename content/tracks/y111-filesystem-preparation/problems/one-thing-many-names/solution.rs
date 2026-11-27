use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfError {
    Missing,
    Taken,
}

struct Slot<T> {
    item: T,
    names: usize,
}

/// Items that can be known by several names. An item is kept while it has at least one.
pub struct Shelf<T> {
    names: HashMap<String, u64>,
    items: HashMap<u64, Slot<T>>,
    next: u64,
}

impl<T> Shelf<T> {
    pub fn new() -> Shelf<T> {
        Shelf {
            names: HashMap::new(),
            items: HashMap::new(),
            next: 0,
        }
    }

    pub fn add(&mut self, name: &str, item: T) -> Result<(), ShelfError> {
        if self.names.contains_key(name) {
            return Err(ShelfError::Taken);
        }
        self.next += 1;
        self.items.insert(self.next, Slot { item, names: 1 });
        self.names.insert(name.to_owned(), self.next);
        Ok(())
    }

    fn id(&self, name: &str) -> Result<u64, ShelfError> {
        self.names.get(name).copied().ok_or(ShelfError::Missing)
    }

    pub fn also(&mut self, existing: &str, new: &str) -> Result<(), ShelfError> {
        let id = self.id(existing)?;
        if self.names.contains_key(new) {
            return Err(ShelfError::Taken);
        }
        self.names.insert(new.to_owned(), id);
        if let Some(slot) = self.items.get_mut(&id) {
            slot.names += 1;
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        let id = self.names.get(name)?;
        Some(&self.items.get(id)?.item)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut T> {
        let id = self.names.get(name)?;
        Some(&mut self.items.get_mut(id)?.item)
    }

    pub fn names_of(&self, name: &str) -> usize {
        self.id(name)
            .ok()
            .and_then(|id| self.items.get(&id))
            .map_or(0, |slot| slot.names)
    }

    /// How many items are kept.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// One name fewer for an item: gives the item back if that was its last.
    fn release(&mut self, id: u64) -> Option<T> {
        let slot = self.items.get_mut(&id)?;
        slot.names -= 1;
        if slot.names > 0 {
            return None;
        }
        self.items.remove(&id).map(|slot| slot.item)
    }

    pub fn remove(&mut self, name: &str) -> Result<Option<T>, ShelfError> {
        let id = self.names.remove(name).ok_or(ShelfError::Missing)?;
        Ok(self.release(id))
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<Option<T>, ShelfError> {
        let id = self.id(from)?;
        let target = self.names.get(to).copied();
        if target == Some(id) {
            return Ok(None); // `to` is `from`, or already another name of the same item
        }
        self.names.remove(from);
        self.names.insert(to.to_owned(), id);
        Ok(target.and_then(|old| self.release(old)))
    }
}

impl<T> Default for Shelf<T> {
    fn default() -> Shelf<T> {
        Shelf::new()
    }
}
