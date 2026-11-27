use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// Names one planned event. No two calls of `after` on one agenda give equal tickets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

/// A place in the queue. The event itself is not in here, so it needs no ordering.
#[derive(Debug, PartialEq, Eq)]
struct Slot {
    time: u64,
    number: u64,
}

impl Ord for Slot {
    /// Reversed, because `BinaryHeap` hands out the greatest first and we want the earliest.
    fn cmp(&self, other: &Slot) -> Ordering {
        (other.time, other.number).cmp(&(self.time, self.number))
    }
}

impl PartialOrd for Slot {
    fn partial_cmp(&self, other: &Slot) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Events waiting for their time, on a clock that only this agenda moves.
pub struct Agenda<E> {
    now: u64,
    planned: u64,
    queue: BinaryHeap<Slot>,
    /// The events that are still pending. A slot without an entry here was cancelled.
    pending: HashMap<u64, E>,
}

impl<E> Agenda<E> {
    pub fn new() -> Agenda<E> {
        Agenda {
            now: 0,
            planned: 0,
            queue: BinaryHeap::new(),
            pending: HashMap::new(),
        }
    }

    /// The time of the last event handed out; 0 before the first.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Events planned and neither handed out nor cancelled.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Plans `event` for `delay` after `now`. `None`, and nothing changes, if that time does not fit in a `u64`.
    pub fn after(&mut self, delay: u64, event: E) -> Option<Ticket> {
        let time = self.now.checked_add(delay)?;
        let number = self.planned;
        self.planned += 1;
        self.queue.push(Slot { time, number });
        self.pending.insert(number, event);
        Some(Ticket(number))
    }

    /// Takes a pending event back.
    pub fn cancel(&mut self, ticket: Ticket) -> Option<E> {
        self.pending.remove(&ticket.0)
    }

    /// Hands out the earliest pending event and moves the clock to its time.
    pub fn next(&mut self) -> Option<(u64, E)> {
        while let Some(slot) = self.queue.pop() {
            self.now = slot.time;
            if let Some(event) = self.pending.remove(&slot.number) {
                return Some((slot.time, event));
            }
        }
        None
    }
}

/// Why a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Nothing is pending any more.
    Finished,
    /// The budget is used up and events are still pending.
    OutOfSteps,
}

/// Hands out events one by one to `handle`, which may plan and cancel more.
pub fn run<E>(
    agenda: &mut Agenda<E>,
    max_steps: u64,
    mut handle: impl FnMut(&mut Agenda<E>, u64, E),
) -> Stop {
    for _ in 0..max_steps {
        match agenda.next() {
            Some((time, event)) => handle(agenda, time, event),
            None => return Stop::Finished,
        }
    }
    if agenda.is_empty() {
        Stop::Finished
    } else {
        Stop::OutOfSteps
    }
}
