#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// `false`: the object is damaged. Its lists below are rubbish.
    pub readable: bool,
    /// The objects it refers to, by id. The same one may be listed more than once.
    pub children: Vec<usize>,
    /// The pages that hold its content.
    pub pages: Vec<usize>,
    /// How many references to it the store believes there are.
    pub links: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    /// `objects[id]`, with `None` for an unused id. Object 0 is the root.
    pub objects: Vec<Option<Object>>,
    /// `in_use[page]`: whether the store believes the page is taken.
    pub in_use: Vec<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Finding {
    NoRoot,
    Unreadable { id: usize },
    Dangling { from: usize, to: usize },
    BadPage { id: usize, page: usize },
    ClaimedTwice { page: usize },
    WrongLinks { id: usize, stored: u32, found: u32 },
    Lost { id: usize },
    Unmarked { page: usize },
    Leaked { page: usize },
}

/// Everything that is wrong, sorted, each finding once.
pub fn audit(store: &Store) -> Vec<Finding> {
    todo!()
}

/// Fixes what can be fixed without risk, and returns how many findings that was.
pub fn repair(store: &mut Store) -> usize {
    todo!()
}
