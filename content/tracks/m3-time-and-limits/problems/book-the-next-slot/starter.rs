pub struct Pacer {
    // your fields
}

impl Pacer {
    /// No message may be given a send time more than `max_wait_ms` ahead.
    pub fn new(max_wait_ms: u64) -> Self {
        todo!()
    }

    /// `Ok(send_time)`, or `Err(d)`: refused, and the same call `d` milliseconds later would be accepted.
    pub fn book(&mut self, now_ms: u64, cost_ms: u64) -> Result<u64, u64> {
        todo!()
    }

    /// Nothing may be sent before `until_ms`.
    pub fn pause_until(&mut self, now_ms: u64, until_ms: u64) {
        todo!()
    }

    /// How long a message booked now would have to wait.
    pub fn backlog_ms(&mut self, now_ms: u64) -> u64 {
        todo!()
    }
}
