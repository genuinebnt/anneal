#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Placed,
    Viewed,
    Paid,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub order: u64,
    pub change: Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    /// The position of its entry in the log.
    pub id: u64,
    pub order: u64,
    pub change: Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Acked,
    /// It may or may not have arrived.
    Unknown,
}

pub trait Wire {
    fn send(&mut self, notice: &Notice) -> Delivery;
}

pub struct Relay {
    // TODO: your fields
}

impl Relay {
    pub fn resume(from: u64) -> Relay {
        todo!()
    }

    pub fn run<W: Wire>(&mut self, log: &[Entry], wire: &mut W) {
        todo!()
    }

    pub fn waiting(&self) -> usize {
        todo!()
    }

    pub fn saved(&self) -> u64 {
        todo!()
    }
}

#[derive(Default)]
pub struct Inbox {
    // TODO: your fields
}

impl Inbox {
    pub fn new() -> Inbox {
        Inbox::default()
    }

    pub fn accept(&mut self, notice: &Notice) -> bool {
        todo!()
    }
}
