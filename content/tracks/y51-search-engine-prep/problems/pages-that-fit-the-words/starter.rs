/// A rising list of page numbers that can only be read one number at a time.
pub trait PageList {
    /// How many numbers it holds. Asking is free.
    fn len(&self) -> usize;
    /// The number at place `i`, which must be below `len()`. The tests count every call.
    fn get(&self, i: usize) -> u32;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The numbers that are in every list, rising. No lists give no numbers.
pub fn in_every_list(lists: &[&dyn PageList]) -> Vec<u32> {
    todo!()
}

pub struct Shelf {
    // your fields
}

impl Shelf {
    pub fn new() -> Shelf {
        todo!()
    }

    /// Stores the next page and returns its number: 0, 1, 2, ...
    pub fn add(&mut self, text: &str) -> u32 {
        todo!()
    }

    /// The pages that hold `word`, in rising order.
    pub fn pages_with(&self, word: &str) -> Vec<u32> {
        todo!()
    }

    /// The pages with every word of `all`, at least one of `any` (unless it is empty) and none of `none`.
    pub fn find(&self, all: &[&str], any: &[&str], none: &[&str]) -> Vec<u32> {
        todo!()
    }
}
