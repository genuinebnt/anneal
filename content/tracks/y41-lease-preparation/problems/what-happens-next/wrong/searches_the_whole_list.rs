/// Names one planned event. No two calls of `after` on one agenda give equal tickets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

/// Events waiting for their time, on a clock that only this agenda moves.
pub struct Agenda<E> {
    now: u64,
    planned: u64,
    /// (time, number, event), in no particular order.
    pending: Vec<(u64, u64, E)>,
}

impl<E> Agenda<E> {
    pub fn new() -> Agenda<E> {
        Agenda {
            now: 0,
            planned: 0,
            pending: Vec::new(),
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
        self.pending.push((time, number, event));
        Some(Ticket(number))
    }

    /// Takes a pending event back.
    pub fn cancel(&mut self, ticket: Ticket) -> Option<E> {
        let at = self.pending.iter().position(|e| e.1 == ticket.0)?;
        Some(self.pending.swap_remove(at).2)
    }

    /// Hands out the earliest pending event and moves the clock to its time.
    pub fn next(&mut self) -> Option<(u64, E)> {
        let at = (0..self.pending.len()).min_by_key(|&i| (self.pending[i].0, self.pending[i].1))?;
        let (time, _, event) = self.pending.swap_remove(at);
        self.now = time;
        Some((time, event))
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
