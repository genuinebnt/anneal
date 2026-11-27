/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seat {
    Free,
    /// Held for a while by someone who has not paid.
    Held {
        who: String,
        until: u64,
    },
    Sold {
        who: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Hold any free seat for `ms` milliseconds.
    Hold { who: String, ms: u64 },
    /// Buy the seat held.
    Buy { who: String },
    /// Give the seat back, held or sold.
    Release { who: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// The command was done, about this seat.
    Done(usize),
    Refused,
}

/// One change to the hall.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Held {
        seat: usize,
        who: String,
        until: u64,
    },
    Sold {
        seat: usize,
    },
    Freed {
        seat: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Begin,
    Change(Effect),
    End,
}

/// The log does not describe a history of this hall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadLog {
    /// The index of the entry that is wrong.
    pub at: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hall {
    seats: Vec<Seat>,
    // add what you need
}

impl Hall {
    pub fn new(size: usize) -> Hall {
        todo!()
    }

    pub fn seats(&self) -> &[Seat] {
        &self.seats
    }

    /// Runs the commands as one unit, and returns their replies and what the log needs.
    pub fn run(
        &mut self,
        commands: &[Command],
        now: u64,
        dice: &mut dyn Dice,
    ) -> (Vec<Reply>, Vec<Entry>) {
        todo!()
    }

    /// The hall the log describes, and how many of its entries count.
    pub fn replay(size: usize, log: &[Entry]) -> Result<(Hall, usize), BadLog> {
        todo!()
    }
}
