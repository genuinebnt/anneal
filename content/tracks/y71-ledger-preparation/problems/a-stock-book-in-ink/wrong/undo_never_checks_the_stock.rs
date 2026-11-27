use std::collections::{HashMap, HashSet};

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
    entries: Vec<Entry>,
    stock: HashMap<String, u64>,
    undone: HashSet<usize>,
}

impl StockBook {
    pub fn new() -> StockBook {
        StockBook::default()
    }

    /// Rebuilds a book from entries that a book wrote.
    pub fn from_entries(entries: &[Entry]) -> StockBook {
        let mut book = StockBook::new();
        for entry in entries {
            book.apply(entry.clone());
        }
        book
    }

    /// The only place that changes anything. The entry has already been accepted.
    fn apply(&mut self, entry: Entry) -> usize {
        let (item, change) = match &entry {
            Entry::In { item, qty } => (item, i64::from(*qty)),
            Entry::Out { item, qty } => (item, -i64::from(*qty)),
            Entry::Undo { of, item, change } => {
                self.undone.insert(*of);
                (item, *change)
            }
        };
        let have = self.stock.entry(item.clone()).or_insert(0);
        *have = have.saturating_add_signed(change);
        self.entries.push(entry);
        self.entries.len() - 1
    }

    fn require(&self, item: &str, need: u64) -> Result<(), Refused> {
        let have = self.stock(item);
        if have >= need {
            Ok(())
        } else {
            Err(Refused::Short { have, need })
        }
    }

    pub fn receive(&mut self, item: &str, qty: u32) -> Result<usize, Refused> {
        if qty == 0 {
            return Err(Refused::Zero);
        }
        let item = item.to_string();
        Ok(self.apply(Entry::In { item, qty }))
    }

    pub fn ship(&mut self, item: &str, qty: u32) -> Result<usize, Refused> {
        if qty == 0 {
            return Err(Refused::Zero);
        }
        self.require(item, qty.into())?;
        let item = item.to_string();
        Ok(self.apply(Entry::Out { item, qty }))
    }

    pub fn undo(&mut self, entry: usize) -> Result<usize, Refused> {
        let (item, change) = match self.entries.get(entry) {
            None => return Err(Refused::NoSuchEntry),
            Some(Entry::Undo { .. }) => return Err(Refused::NotUndoable),
            Some(Entry::In { item, qty }) => (item.clone(), -i64::from(*qty)),
            Some(Entry::Out { item, qty }) => (item.clone(), i64::from(*qty)),
        };
        if self.undone.contains(&entry) {
            return Err(Refused::AlreadyUndone);
        }
        Ok(self.apply(Entry::Undo {
            of: entry,
            item,
            change,
        }))
    }

    pub fn stock(&self, item: &str) -> u64 {
        self.stock.get(item).copied().unwrap_or(0)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}
