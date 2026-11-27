#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Sale,
    Refund,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub pos: u64,
    pub day: u32,
    pub shop: String,
    pub kind: Kind,
    pub cents: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    pub expected: u64,
    pub got: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Board {
    // TODO: your fields
}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }

    pub fn next(&self) -> u64 {
        todo!()
    }

    pub fn apply(&mut self, batch: &[Record]) -> Result<usize, Gap> {
        todo!()
    }

    pub fn net(&self, shop: &str) -> i128 {
        todo!()
    }

    pub fn day(&self, day: u32) -> (u128, u128) {
        todo!()
    }

    pub fn top(&self, n: usize) -> Vec<(String, i128)> {
        todo!()
    }

    pub fn rebuild(log: &[Record]) -> Board {
        todo!()
    }

    pub fn as_of(log: &[Record], pos: u64) -> Board {
        todo!()
    }
}
