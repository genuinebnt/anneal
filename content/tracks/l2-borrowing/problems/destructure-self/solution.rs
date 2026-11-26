#[derive(Debug, PartialEq)]
pub enum State {
    Idle,
    Busy { job: String, tries: u32 },
    Done { job: String, result: u64 },
}

pub struct Worker {
    pub state: State,
    pub log: Vec<String>,
    pub max_tries: u32,
}

impl Worker {
    /// Idle → Busy with `job` and 0 tries, logging "start <job>". Anything else: no change, `false`.
    pub fn start(&mut self, job: &str) -> bool {
        if self.state != State::Idle {
            return false;
        }
        self.log.push(format!("start {job}"));
        self.state = State::Busy {
            job: job.to_string(),
            tries: 0,
        };
        true
    }

    /// Busy: counts a try and logs "retry <job> #<tries>". When tries reaches `max_tries` the worker gives up:
    /// back to Idle, logging "give up <job>" instead, and `None`. Otherwise returns the new tries count.
    /// Not busy: no change, `None`.
    pub fn retry(&mut self) -> Option<u32> {
        let Worker {
            state,
            log,
            max_tries,
        } = self;
        let State::Busy { job, tries } = state else {
            return None;
        };
        *tries += 1;
        if *tries < *max_tries {
            log.push(format!("retry {job} #{tries}"));
            return Some(*tries);
        }
        log.push(format!("give up {job}"));
        *state = State::Idle;
        None
    }

    /// Busy → Done with the same job `String` (moved, not copied) and `result`, logging "done <job>".
    /// Anything else: no change, `false`.
    pub fn finish(&mut self, result: u64) -> bool {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Busy { job, .. } => {
                self.log.push(format!("done {job}"));
                self.state = State::Done { job, result };
                true
            }
            other => {
                self.state = other;
                false
            }
        }
    }

    /// Done → Idle, handing back the job and result. Anything else: no change, `None`.
    pub fn collect(&mut self) -> Option<(String, u64)> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Done { job, result } => Some((job, result)),
            other => {
                self.state = other;
                None
            }
        }
    }
}
