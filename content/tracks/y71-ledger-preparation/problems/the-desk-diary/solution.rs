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
    let next_in_line = || {
        book.queue
            .front()
            .map(|m| Event::SetAside { member: m.clone() })
    };
    match command {
        Command::Lend { member } => {
            if book.retired {
                Err(Refusal::Retired)
            } else if book.out_to.is_some() {
                Err(Refusal::OnLoan)
            } else if book.held_for.as_ref().is_some_and(|h| h != member) {
                Err(Refusal::HeldForAnother)
            } else {
                Ok(vec![Event::Lent {
                    member: member.clone(),
                }])
            }
        }
        Command::Return { member } => {
            if book.out_to.as_ref() != Some(member) {
                return Err(Refusal::NotYourLoan);
            }
            let mut events = vec![Event::Returned];
            events.extend(next_in_line());
            Ok(events)
        }
        Command::Reserve { member } => {
            let theirs = book.out_to.as_ref() == Some(member)
                || book.held_for.as_ref() == Some(member)
                || book.queue.contains(member);
            if book.retired {
                Err(Refusal::Retired)
            } else if theirs {
                Err(Refusal::AlreadyYours)
            } else if book.out_to.is_none() && book.held_for.is_none() {
                Ok(vec![Event::SetAside {
                    member: member.clone(),
                }])
            } else {
                Ok(vec![Event::Queued {
                    member: member.clone(),
                }])
            }
        }
        Command::Cancel { member } => {
            if book.held_for.as_ref() == Some(member) {
                let mut events = vec![Event::HoldDropped];
                events.extend(next_in_line());
                Ok(events)
            } else if book.queue.contains(member) {
                Ok(vec![Event::LeftQueue {
                    member: member.clone(),
                }])
            } else {
                Err(Refusal::NothingToCancel)
            }
        }
        Command::Retire => {
            if book.retired {
                Err(Refusal::Retired)
            } else if book.out_to.is_some() {
                Err(Refusal::OnLoan)
            } else {
                Ok(vec![Event::Retired])
            }
        }
    }
}

/// The card after one more thing has happened.
pub fn evolve(mut book: Book, event: &Event) -> Book {
    match event {
        Event::Lent { member } => {
            book.out_to = Some(member.clone());
            book.held_for = None;
            book.lends = book.lends.saturating_add(1);
        }
        Event::Returned => book.out_to = None,
        Event::Queued { member } => book.queue.push_back(member.clone()),
        Event::LeftQueue { member } => book.queue.retain(|m| m != member),
        Event::SetAside { member } => {
            book.queue.retain(|m| m != member);
            book.held_for = Some(member.clone());
        }
        Event::HoldDropped => book.held_for = None,
        Event::Retired => {
            book.retired = true;
            book.held_for = None;
            book.queue.clear();
        }
    }
    book
}

/// The card after all of these, from an empty one.
pub fn replay(events: &[Event]) -> Book {
    events.iter().fold(Book::default(), evolve)
}
