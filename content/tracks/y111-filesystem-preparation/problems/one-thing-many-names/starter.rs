use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfError {
    Missing,
    Taken,
}

/// Items that can be known by several names. An item is kept while it has at least one.
pub struct Shelf<T> {
    // TODO: the fields (remove this placeholder)
    items: PhantomData<T>,
}

impl<T> Shelf<T> {
    pub fn new() -> Shelf<T> {
        todo!()
    }

    pub fn add(&mut self, name: &str, item: T) -> Result<(), ShelfError> {
        todo!()
    }

    pub fn also(&mut self, existing: &str, new: &str) -> Result<(), ShelfError> {
        todo!()
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        todo!()
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut T> {
        todo!()
    }

    pub fn names_of(&self, name: &str) -> usize {
        todo!()
    }

    /// How many items are kept.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn remove(&mut self, name: &str) -> Result<Option<T>, ShelfError> {
        todo!()
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<Option<T>, ShelfError> {
        todo!()
    }
}

impl<T> Default for Shelf<T> {
    fn default() -> Shelf<T> {
        Shelf::new()
    }
}
