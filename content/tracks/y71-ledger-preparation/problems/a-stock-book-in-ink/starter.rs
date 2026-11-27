#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    In {
        item: String,
        qty: u32,
    },
    Out {
        item: String,
        qty: u32,
    },
    /// Cancels entry `of`. `change` is what this entry does to the stock.
    Undo {
        of: usize,
        item: String,
        change: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Zero,
    Short { have: u64, need: u64 },
    NoSuchEntry,
    NotUndoable,
    AlreadyUndone,
}

#[derive(Default)]
pub struct StockBook {
    // TODO: your fields
}

impl StockBook {
    pub fn new() -> StockBook {
        StockBook::default()
    }

    /// Rebuilds a book from entries that a book wrote.
    pub fn from_entries(entries: &[Entry]) -> StockBook {
        todo!()
    }

    pub fn receive(&mut self, item: &str, qty: u32) -> Result<usize, Refused> {
        todo!()
    }

    pub fn ship(&mut self, item: &str, qty: u32) -> Result<usize, Refused> {
        todo!()
    }

    pub fn undo(&mut self, entry: usize) -> Result<usize, Refused> {
        todo!()
    }

    pub fn stock(&self, item: &str) -> u64 {
        todo!()
    }

    pub fn entries(&self) -> &[Entry] {
        todo!()
    }
}
