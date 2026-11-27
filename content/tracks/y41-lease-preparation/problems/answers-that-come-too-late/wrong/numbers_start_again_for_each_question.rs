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

/// The question being asked now.
struct Open {
    request: u64,
    /// Its attempts are `first..=latest`.
    first: u64,
    latest: u64,
    /// The wait of the latest attempt.
    wait_ms: u64,
}

/// Asks one question at a time and asks again when no answer comes.
pub struct Caller {
    first_wait_ms: u64,
    max_wait_ms: u64,
    max_attempts: u32,
    /// Attempts ever made. It never starts again, so a number names one message for good.
    attempts: u64,
    open: Option<Open>,
}

impl Caller {
    pub fn new(first_wait_ms: u64, max_wait_ms: u64, max_attempts: u32) -> Caller {
        assert!(max_attempts > 0, "at least one attempt is needed");
        Caller {
            first_wait_ms,
            max_wait_ms,
            max_attempts,
            attempts: 0,
            open: None,
        }
    }

    /// Takes one input and returns what is to be done, in order.
    pub fn handle(&mut self, input: Input) -> Vec<Output> {
        match input {
            Input::Ask { request } => {
                if self.open.is_some() {
                    return vec![Output::Busy { request }];
                }
                let wait_ms = self.first_wait_ms.min(self.max_wait_ms);
                self.attempts = 0;
                let first = self.attempts + 1;
                self.attempt(request, first, wait_ms)
            }
            Input::Reply { attempt, answer } => match &self.open {
                Some(open) if (open.first..=open.latest).contains(&attempt) => {
                    let request = open.request;
                    self.open = None;
                    vec![Output::Answered { request, answer }]
                }
                _ => Vec::new(),
            },
            Input::TimeUp { attempt } => match &self.open {
                Some(open) if attempt == open.latest => {
                    let (request, first) = (open.request, open.first);
                    if open.latest - first + 1 >= u64::from(self.max_attempts) {
                        self.open = None;
                        return vec![Output::Failed { request }];
                    }
                    let wait_ms = open.wait_ms.saturating_mul(2).min(self.max_wait_ms);
                    self.attempt(request, first, wait_ms)
                }
                _ => Vec::new(),
            },
            Input::GiveUp => {
                self.open = None;
                Vec::new()
            }
        }
    }

    fn attempt(&mut self, request: u64, first: u64, wait_ms: u64) -> Vec<Output> {
        self.attempts += 1;
        let attempt = self.attempts;
        self.open = Some(Open {
            request,
            first,
            latest: attempt,
            wait_ms,
        });
        vec![
            Output::Send { attempt, request },
            Output::SetTimer { attempt, wait_ms },
        ]
    }
}
