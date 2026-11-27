use std::collections::{HashSet, VecDeque};

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
    /// The first position not yet read.
    read: u64,
    /// Oldest first, because entries are read in order and removal keeps the order.
    waiting: VecDeque<Notice>,
}

fn notice(id: u64, entry: &Entry) -> Option<Notice> {
    matches!(entry.change, Change::Paid | Change::Cancelled).then_some(Notice {
        id,
        order: entry.order,
        change: entry.change,
    })
}

impl Relay {
    pub fn resume(from: u64) -> Relay {
        Relay {
            read: from,
            waiting: VecDeque::new(),
        }
    }

    pub fn run<W: Wire>(&mut self, log: &[Entry], wire: &mut W) {
        let start = usize::try_from(self.read).map_or(log.len(), |r| r.min(log.len()));
        for (i, entry) in log.iter().enumerate().skip(start) {
            self.waiting.extend(notice(i as u64, entry));
            self.read = i as u64 + 1;
        }
        self.waiting.retain(|n| wire.send(n) != Delivery::Acked);
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    pub fn saved(&self) -> u64 {
        self.waiting.front().map_or(self.read, |n| n.id)
    }
}

#[derive(Default)]
pub struct Inbox {
    seen: HashSet<u64>,
}

impl Inbox {
    pub fn new() -> Inbox {
        Inbox::default()
    }

    pub fn accept(&mut self, notice: &Notice) -> bool {
        self.seen.insert(notice.id)
    }
}
