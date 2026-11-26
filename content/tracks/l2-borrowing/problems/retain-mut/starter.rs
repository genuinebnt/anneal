#[derive(Debug, PartialEq)]
pub struct Conn {
    pub id: u32,
    pub idle: u64,
    pub tokens: u32,
}

pub struct Pool {
    pub conns: Vec<Conn>,
    pub max_idle: u64,
    pub refill: u32,
    pub cap: u32,
    closed: Vec<u32>,
}

impl Pool {
    pub fn new(conns: Vec<Conn>, max_idle: u64, refill: u32, cap: u32) -> Self {
        Pool {
            conns,
            max_idle,
            refill,
            cap,
            closed: Vec::new(),
        }
    }

    fn is_stale(&self, c: &Conn) -> bool {
        c.idle > self.max_idle
    }

    /// How many connections are idle longer than `max_idle`.
    pub fn stale_count(&self) -> usize {
        self.conns.iter().filter(|c| self.is_stale(c)).count()
    }

    /// Ages every connection by `dt`. A connection now idle longer than `max_idle` is closed: removed, with its
    /// id appended to the closed list. Every other connection gains `refill` tokens, up to `cap`. Keeps the
    /// order, runs in one pass, and returns how many it closed.
    pub fn tick(&mut self, dt: u64) -> usize {
        todo!()
    }

    /// Spends one token of connection `id` and resets its idle time. `false` if there's no such connection
    /// or it has no tokens (then nothing changes).
    pub fn use_conn(&mut self, id: u32) -> bool {
        todo!()
    }

    /// Hands over the ids closed so far, oldest first, and starts a new list.
    pub fn drain_closed(&mut self) -> Vec<u32> {
        todo!()
    }
}
