#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub player: u64,
    pub score: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadCursor;

pub struct Board {
    // TODO: your fields
}

impl Board {
    pub fn new() -> Self {
        todo!()
    }

    /// Adds the player, or changes their score.
    pub fn set(&mut self, player: u64, score: u64) {
        todo!()
    }

    /// Whether the player was on the board.
    pub fn remove(&mut self, player: u64) -> bool {
        todo!()
    }

    pub fn page(&self, cursor: Option<&str>, limit: usize) -> Result<Page, BadCursor> {
        todo!()
    }
}
