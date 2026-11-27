use std::collections::VecDeque;

/// The card for one copy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Book {
    pub out_to: Option<String>,
    pub held_for: Option<String>,
    pub queue: VecDeque<String>,
    pub retired: bool,
    pub lends: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Lend { member: String },
    Return { member: String },
    Reserve { member: String },
    Cancel { member: String },
    Retire,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Lent { member: String },
    Returned,
    Queued { member: String },
    LeftQueue { member: String },
    SetAside { member: String },
    HoldDropped,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Retired,
    OnLoan,
    HeldForAnother,
    NotYourLoan,
    AlreadyYours,
    NothingToCancel,
}

/// What follows from a command, or why it cannot be done.
pub fn decide(book: &Book, command: &Command) -> Result<Vec<Event>, Refusal> {
    todo!()
}

/// The card after one more thing has happened.
pub fn evolve(book: Book, event: &Event) -> Book {
    todo!()
}

/// The card after all of these, from an empty one.
pub fn replay(events: &[Event]) -> Book {
    todo!()
}
