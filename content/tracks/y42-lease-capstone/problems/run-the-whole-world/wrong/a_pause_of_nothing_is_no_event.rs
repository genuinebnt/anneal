use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

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

// ---------------------------------------------------------------------------------------------------------------------
// The agenda (preparation: "What happens next?"), without cancelling
// ---------------------------------------------------------------------------------------------------------------------

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
struct Agenda<E> {
    now: u64,
    planned: u64,
    queue: BinaryHeap<Slot>,
    /// The events, by their numbers.
    pending: HashMap<u64, E>,
}

impl<E> Agenda<E> {
    fn new() -> Agenda<E> {
        Agenda {
            now: 0,
            planned: 0,
            queue: BinaryHeap::new(),
            pending: HashMap::new(),
        }
    }

    /// Plans `event` for `delay` after `now`. `None` if that time does not fit in a `u64`.
    fn after(&mut self, delay: u64, event: E) -> Option<()> {
        let time = self.now.checked_add(delay)?;
        let number = self.planned;
        self.planned += 1;
        self.queue.push(Slot { time, number });
        self.pending.insert(number, event);
        Some(())
    }

    /// Hands out the earliest pending event and moves the clock to its time.
    fn next(&mut self) -> Option<(u64, E)> {
        let slot = self.queue.pop()?;
        let event = self.pending.remove(&slot.number)?;
        self.now = slot.time;
        Some((slot.time, event))
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The world (capstone step 4)
// ---------------------------------------------------------------------------------------------------------------------

/// The kinds of waiting in the simulated world, all in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Wait {
    /// Before a client's first wake.
    Start,
    /// One message on its way, in either direction.
    Net,
    /// Preparing a write, before the client looks at its lease.
    Work,
    /// A hold-up between the look and the write. Usually 0.
    Pause,
    /// Before trying again after a refusal.
    Backoff,
    /// Before waking again after a lease.
    Rest,
}

/// Decides how long everything takes.
pub trait Delays {
    fn wait(&mut self, client: u32, kind: Wait) -> u64;
}

/// What to simulate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub clients: u32,
    pub leases_per_client: u32,
    pub writes_per_lease: u32,
    pub ttl_ms: u64,
    /// Does the disk turn away stale tokens?
    pub guarded: bool,
    /// The most events a run may handle.
    pub max_events: u64,
}

/// One thing that happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// The service granted a lease.
    Granted { token: u64, expires_at: u64 },
    /// The service refused one.
    Refused,
    /// The client looked at its lease before writing.
    Checked { passed: bool },
    /// The client was held up after a look that passed.
    Paused { ms: u64 },
    /// The client sent a write.
    Sent { token: u64, value: u64 },
    /// The write reached the disk.
    Stored {
        token: u64,
        value: u64,
        accepted: bool,
    },
    /// The service answered a renewal.
    Renewal { token: u64, expires_at: Option<u64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub time: u64,
    pub client: u32,
    pub what: What,
}

/// Everything a finished run leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub trace: Vec<Entry>,
    /// The writes that reached the disk, in order of arrival.
    pub arrived: Vec<Write>,
    pub accepted: Vec<Write>,
    pub value: Option<u64>,
    /// The time of the last event.
    pub end_time: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimError {
    /// More than `max_events` events were needed.
    OutOfSteps,
    /// Something would happen after the last time a `u64` can name.
    TimeOverflow,
}

/// Something on its way: to a client, to the service or to the disk.
#[derive(Debug, Clone, Copy)]
enum Mail {
    ToClient {
        client: u32,
        event: ClientEvent,
    },
    /// The client's own work, reported with the time at which it ends.
    WorkEnds {
        client: u32,
    },
    Acquire {
        client: u32,
    },
    Renew {
        client: u32,
        token: u64,
    },
    Store {
        client: u32,
        token: u64,
        value: u64,
    },
}

struct World<'a> {
    agenda: Agenda<Mail>,
    delays: &'a mut dyn Delays,
    clients: Vec<Client>,
    service: LeaseService,
    disk: Disk,
    trace: Vec<Entry>,
    values_sent: u64,
}

impl World<'_> {
    fn note(&mut self, client: u32, what: What) {
        let time = self.agenda.now;
        self.trace.push(Entry { time, client, what });
    }

    fn post(&mut self, delay: u64, mail: Mail) -> Result<(), SimError> {
        self.agenda.after(delay, mail).ok_or(SimError::TimeOverflow)
    }

    /// Posts something that takes a wait of this kind.
    fn post_after(&mut self, client: u32, kind: Wait, mail: Mail) -> Result<(), SimError> {
        let delay = self.delays.wait(client, kind);
        self.post(delay, mail)
    }

    fn reply(&mut self, client: u32, event: ClientEvent) -> Result<(), SimError> {
        self.post_after(client, Wait::Net, Mail::ToClient { client, event })
    }

    fn deliver(&mut self, mail: Mail) -> Result<(), SimError> {
        let now = self.agenda.now;
        match mail {
            Mail::ToClient { client, event } => self.tell(client, event),
            Mail::WorkEnds { client } => self.tell(client, ClientEvent::WorkDone { now }),
            Mail::Acquire { client } => {
                let event = match self.service.acquire(now, client) {
                    Some(lease) => {
                        let Lease { token, expires_at } = lease;
                        self.note(client, What::Granted { token, expires_at });
                        ClientEvent::Granted(lease)
                    }
                    None => {
                        self.note(client, What::Refused);
                        ClientEvent::Denied
                    }
                };
                self.reply(client, event)
            }
            Mail::Renew { client, token } => {
                let renewed = self.service.renew(now, client, token);
                let expires_at = renewed.map(|lease| lease.expires_at);
                self.note(client, What::Renewal { token, expires_at });
                self.reply(client, ClientEvent::Renewed { token, expires_at })
            }
            Mail::Store {
                client,
                token,
                value,
            } => {
                let accepted = self.disk.write(Write {
                    time: now,
                    client,
                    token,
                    value,
                });
                self.note(
                    client,
                    What::Stored {
                        token,
                        value,
                        accepted,
                    },
                );
                self.reply(client, ClientEvent::WriteDone { accepted })
            }
        }
    }

    /// Gives a client an event and carries out what it asks for.
    fn tell(&mut self, client: u32, event: ClientEvent) -> Result<(), SimError> {
        let action = self.clients[client as usize].handle(event);
        if matches!(event, ClientEvent::WorkDone { .. }) {
            let passed = action == Some(Action::Passed);
            self.note(client, What::Checked { passed });
        }
        let wake = Mail::ToClient {
            client,
            event: ClientEvent::Wake,
        };
        match action {
            None => Ok(()),
            Some(Action::Acquire) => self.post_after(client, Wait::Net, Mail::Acquire { client }),
            Some(Action::Work) => self.post_after(client, Wait::Work, Mail::WorkEnds { client }),
            Some(Action::Passed) => {
                let ms = self.delays.wait(client, Wait::Pause);
                if ms > 0 {
                    self.note(client, What::Paused { ms });
                }
                let event = ClientEvent::PauseOver;
                if ms == 0 {
                    return self.tell(client, event);
                }
                self.post(ms, Mail::ToClient { client, event })
            }
            Some(Action::Write { token }) => {
                self.values_sent += 1;
                let value = self.values_sent;
                self.note(client, What::Sent { token, value });
                let mail = Mail::Store {
                    client,
                    token,
                    value,
                };
                self.post_after(client, Wait::Net, mail)
            }
            Some(Action::Renew { token }) => {
                self.post_after(client, Wait::Net, Mail::Renew { client, token })
            }
            Some(Action::Backoff) => self.post_after(client, Wait::Backoff, wake),
            Some(Action::Rest) => self.post_after(client, Wait::Rest, wake),
        }
    }
}

/// Runs the whole world until nothing is left to happen.
pub fn simulate(config: &Config, delays: &mut dyn Delays) -> Result<Report, SimError> {
    let mut world = World {
        agenda: Agenda::new(),
        delays,
        clients: (0..config.clients)
            .map(|_| Client::new(config.leases_per_client, config.writes_per_lease))
            .collect(),
        service: LeaseService::new(config.ttl_ms),
        disk: Disk::new(config.guarded),
        trace: Vec::new(),
        values_sent: 0,
    };
    for client in 0..config.clients {
        let event = ClientEvent::Wake;
        world.post_after(client, Wait::Start, Mail::ToClient { client, event })?;
    }
    let mut handled = 0;
    while let Some((_, mail)) = world.agenda.next() {
        if handled == config.max_events {
            return Err(SimError::OutOfSteps);
        }
        handled += 1;
        world.deliver(mail)?;
    }
    Ok(Report {
        end_time: world.agenda.now,
        arrived: world.disk.arrived().to_vec(),
        accepted: world.disk.accepted().to_vec(),
        value: world.disk.value(),
        trace: world.trace,
    })
}
