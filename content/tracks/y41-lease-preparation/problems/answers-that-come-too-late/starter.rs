/// Something that happened to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Its owner wants `request` answered.
    Ask { request: u64 },
    /// An answer arrived, to the message sent as `attempt`.
    Reply { attempt: u64, answer: u64 },
    /// The timer set for `attempt` went off.
    TimeUp { attempt: u64 },
    /// Its owner no longer cares.
    GiveUp,
}

/// Something the caller wants done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Send { attempt: u64, request: u64 },
    SetTimer { attempt: u64, wait_ms: u64 },
    Answered { request: u64, answer: u64 },
    Failed { request: u64 },
    Busy { request: u64 },
}

/// Asks one question at a time and asks again when no answer comes.
pub struct Caller {
    // TODO: the fields
}

impl Caller {
    pub fn new(first_wait_ms: u64, max_wait_ms: u64, max_attempts: u32) -> Caller {
        todo!()
    }

    /// Takes one input and returns what is to be done, in order.
    pub fn handle(&mut self, input: Input) -> Vec<Output> {
        todo!()
    }
}
