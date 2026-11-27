/// Names one planned event. No two calls of `after` on one agenda give equal tickets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

/// Events waiting for their time, on a clock that only this agenda moves.
pub struct Agenda<E> {
    // TODO: the fields (replace the placeholder)
    placeholder: std::marker::PhantomData<E>,
}

impl<E> Agenda<E> {
    pub fn new() -> Agenda<E> {
        todo!()
    }

    /// The time of the last event handed out; 0 before the first.
    pub fn now(&self) -> u64 {
        todo!()
    }

    /// Events planned and neither handed out nor cancelled.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Plans `event` for `delay` after `now`. `None`, and nothing changes, if that time does not fit in a `u64`.
    pub fn after(&mut self, delay: u64, event: E) -> Option<Ticket> {
        todo!()
    }

    /// Takes a pending event back.
    pub fn cancel(&mut self, ticket: Ticket) -> Option<E> {
        todo!()
    }

    /// Hands out the earliest pending event and moves the clock to its time.
    pub fn next(&mut self) -> Option<(u64, E)> {
        todo!()
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
    handle: impl FnMut(&mut Agenda<E>, u64, E),
) -> Stop {
    todo!()
}
