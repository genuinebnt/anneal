// ---------------------------------------------------------------------------------------------------------------------
// The lease service (capstone step 1)
// ---------------------------------------------------------------------------------------------------------------------

/// Permission to be the one writer until a time, and the number of this grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lease {
    pub token: u64,
    pub expires_at: u64,
}

/// Grants one lease at a time, going by its own clock.
pub struct LeaseService {
    ttl_ms: u64,
    /// The latest time any call has named.
    now: u64,
    /// Grants made so far: the last token.
    granted: u64,
    /// The latest grant and who got it. It may have run out.
    last: Option<(u32, Lease)>,
}

impl LeaseService {
    pub fn new(ttl_ms: u64) -> LeaseService {
        assert!(ttl_ms > 0, "a lease must last for some time");
        LeaseService {
            ttl_ms,
            now: 0,
            granted: 0,
            last: None,
        }
    }

    /// Moves the clock, never backwards, and says whose lease is live.
    fn live(&mut self, now: u64) -> Option<(u32, Lease)> {
        self.now = self.now.max(now);
        self.last.filter(|(_, lease)| self.now < lease.expires_at)
    }

    fn lease_from_now(&self, token: u64) -> Lease {
        Lease {
            token,
            expires_at: self.now.saturating_add(self.ttl_ms),
        }
    }

    /// A new lease for `client`, unless somebody's lease is live.
    pub fn acquire(&mut self, now: u64, client: u32) -> Option<Lease> {
        if self.live(now).is_some() {
            return None;
        }
        self.granted += 1;
        let lease = self.lease_from_now(self.granted);
        self.last = Some((client, lease));
        Some(lease)
    }

    /// Makes the live lease last longer, if `client` holds it under `token`.
    pub fn renew(&mut self, now: u64, client: u32, token: u64) -> Option<Lease> {
        let (holder, held) = self.live(now)?;
        if holder != client || held.token != token {
            return None;
        }
        let lease = self.lease_from_now(token);
        self.last = Some((client, lease));
        Some(lease)
    }

    /// Who holds a live lease.
    pub fn holder(&mut self, now: u64) -> Option<u32> {
        self.live(now).map(|(client, _)| client)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The disk and its judge (capstone step 2)
// ---------------------------------------------------------------------------------------------------------------------

/// One write as it reached the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Write {
    pub time: u64,
    pub client: u32,
    pub token: u64,
    pub value: u64,
}

/// A way in which what happened to the disk was unsafe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breach {
    /// `stale` was accepted after `newer`, although its token is lower.
    StaleWrite { newer: Write, stale: Write },
    /// The disk does not end with the newest write that reached it.
    NewestLost { newest: Write, found: Option<u64> },
}

/// The shared resource that the lease is meant to protect.
pub struct Disk {
    guarded: bool,
    highest_token: u64,
    arrived: Vec<Write>,
    accepted: Vec<Write>,
}

impl Disk {
    pub fn new(guarded: bool) -> Disk {
        Disk {
            guarded,
            highest_token: 0,
            arrived: Vec::new(),
            accepted: Vec::new(),
        }
    }

    /// Takes a write in. `false`: it was turned away.
    pub fn write(&mut self, write: Write) -> bool {
        self.arrived.push(write);
        if self.guarded && write.token < self.highest_token {
            return false;
        }
        self.highest_token = self.highest_token.max(write.token);
        self.accepted.push(write);
        true
    }

    /// What the disk holds now.
    pub fn value(&self) -> Option<u64> {
        self.accepted.last().map(|w| w.value)
    }

    pub fn arrived(&self) -> &[Write] {
        &self.arrived
    }

    pub fn accepted(&self) -> &[Write] {
        &self.accepted
    }
}

/// Judges a disk from the outside: what reached it, what it accepted and what it holds at the end.
pub fn judge(arrived: &[Write], accepted: &[Write], value: Option<u64>) -> Result<(), Breach> {
    // While the tokens never fall, the write just before holds the highest one so far.
    if let Some(pair) = accepted.windows(2).find(|p| p[1].token < p[0].token) {
        return Err(Breach::StaleWrite {
            newer: pair[0],
            stale: pair[1],
        });
    }
    match arrived.iter().max_by_key(|w| (w.token, w.value)) {
        Some(&newest) if value != Some(newest.value) => Err(Breach::NewestLost {
            newest,
            found: value,
        }),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// A client (capstone step 3)
// ---------------------------------------------------------------------------------------------------------------------

/// Something that reaches a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientEvent {
    /// Time to try for a lease.
    Wake,
    Granted(Lease),
    Denied,
    /// The work before a write is finished. `now` is what the client's clock says.
    WorkDone {
        now: u64,
    },
    /// Whatever held the client up between the check and the write is over.
    PauseOver,
    WriteDone {
        accepted: bool,
    },
    /// The answer to a renewal of `token`: the new end of the lease, or `None` if it was refused.
    Renewed {
        token: u64,
        expires_at: Option<u64>,
    },
}

/// What a client wants to happen next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Ask the service for a lease.
    Acquire,
    /// Prepare the next write.
    Work,
    /// The lease looked good: the write goes out next.
    Passed,
    /// Send a write under `token`.
    Write { token: u64 },
    /// Ask the service to make the lease of `token` last longer.
    Renew { token: u64 },
    /// Refused: try again later.
    Backoff,
    /// Done with this lease, or lost it: wake again later.
    Rest,
}

/// The lease a client is working under.
#[derive(Debug, Clone, Copy)]
struct Held {
    lease: Lease,
    writes_left: u32,
}

/// What the client is waiting for.
#[derive(Debug, Clone, Copy)]
enum Waiting {
    ForWake,
    ForAnswer,
    ForWork(Held),
    ForPause(Held),
    ForWrite(Held),
    ForRenewal(Held),
}

/// A client of the lease service, with no clock and no network of its own.
pub struct Client {
    leases_left: u32,
    writes_per_lease: u32,
    /// The highest token ever granted to this client.
    newest_token: u64,
    waiting: Waiting,
}

impl Client {
    /// A client that wants `leases` leases and sends `writes_per_lease` writes under each.
    pub fn new(leases: u32, writes_per_lease: u32) -> Client {
        Client {
            leases_left: leases,
            writes_per_lease,
            newest_token: 0,
            waiting: Waiting::ForWake,
        }
    }

    /// Leases it still wants.
    pub fn leases_left(&self) -> u32 {
        self.leases_left
    }

    fn go(&mut self, waiting: Waiting, action: Action) -> Option<Action> {
        self.waiting = waiting;
        Some(action)
    }

    /// Takes one event. `None`: nothing to do, and nothing changed.
    pub fn handle(&mut self, event: ClientEvent) -> Option<Action> {
        use ClientEvent as E;
        match (self.waiting, event) {
            (Waiting::ForWake, E::Wake) if self.leases_left > 0 => {
                self.go(Waiting::ForAnswer, Action::Acquire)
            }
            // a grant seen before, delivered again, has a token that is not new
            (Waiting::ForAnswer, E::Granted(lease)) if lease.token > self.newest_token => {
                self.newest_token = lease.token;
                self.leases_left -= 1;
                if self.writes_per_lease == 0 {
                    return self.go(Waiting::ForWake, Action::Rest);
                }
                let held = Held {
                    lease,
                    writes_left: self.writes_per_lease,
                };
                self.go(Waiting::ForWork(held), Action::Work)
            }
            (Waiting::ForAnswer, E::Denied) => self.go(Waiting::ForWake, Action::Backoff),
            (Waiting::ForWork(held), E::WorkDone { now }) => {
                if now < held.lease.expires_at {
                    self.go(Waiting::ForPause(held), Action::Passed)
                } else {
                    self.go(Waiting::ForWake, Action::Rest)
                }
            }
            // no second look at a clock: a pause could follow that one too
            (Waiting::ForPause(mut held), E::PauseOver) => {
                held.writes_left -= 1;
                let token = held.lease.token;
                self.go(Waiting::ForWrite(held), Action::Write { token })
            }
            // the verdict changes nothing here
            (Waiting::ForWrite(held), E::WriteDone { .. }) => {
                if held.writes_left == 0 {
                    return self.go(Waiting::ForWake, Action::Rest);
                }
                let token = held.lease.token;
                self.go(Waiting::ForRenewal(held), Action::Renew { token })
            }
            (Waiting::ForRenewal(mut held), E::Renewed { token, expires_at })
                if token == held.lease.token =>
            {
                match expires_at {
                    Some(expires_at) => {
                        held.lease.expires_at = expires_at;
                        self.go(Waiting::ForWork(held), Action::Work)
                    }
                    None => self.go(Waiting::ForWake, Action::Rest),
                }
            }
            _ => None,
        }
    }
}
