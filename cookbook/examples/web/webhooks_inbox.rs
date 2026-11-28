//! RECIPE 1213 (web): after the signature passes, remember the event before you answer
//! LEVEL: intermediate
//! Use in: a webhook receiver, any endpoint whose sender retries.   Run: cargo run --release --example webhooks_inbox
//!
//! WHAT: the part of a receiver that no signature does for you. The sender retries until it sees a 2xx, so
//! the same event can arrive again, and a 2xx tells the sender it may forget the event.
//!  * `receive` stores the event under its id and only then answers 200. A repeat changes nothing.
//!  * `AckFirst` shows the mistake: it answers 200, keeps the event in memory and stores it later.
//!  * The "table" is a map that survives a pretend crash. It stands for a database table with a unique key
//!    on the event id. It does not show a real database, a disk, or how long to keep ids.
//!
//! It assumes the signature was already checked. It does not check one.
//! COMMON USES: payment and Git host webhooks, message consumers ("inbox" tables).
//! Read: https://docs.stripe.com/webhooks
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

/// What survives a crash. In a real service: rows with a unique key on the id, committed before the answer.
type Table = BTreeMap<u64, String>;

#[derive(Debug, PartialEq)]
enum Reply {
    Stored,    // 200
    Duplicate, // 200: the sender may stop
    Conflict,  // 409: a known id with another body. Keep the first, tell a person.
    TryAgain,  // 503: nothing was stored
}

fn receive(table: &mut Table, id: u64, body: &str, can_write: bool) -> Reply {
    match table.entry(id) {
        Entry::Occupied(old) if old.get() == body => Reply::Duplicate,
        Entry::Occupied(_) => Reply::Conflict,
        Entry::Vacant(_) if !can_write => Reply::TryAgain,
        Entry::Vacant(slot) => {
            slot.insert(body.to_string()); // one step: "is it new?" and "keep it"
            Reply::Stored
        }
    }
}

/// THE MISTAKE: the answer goes out while the event is only in memory.
#[derive(Default)]
struct AckFirst {
    waiting: Vec<(u64, String)>,
}

impl AckFirst {
    fn receive(&mut self, id: u64, body: &str) -> Reply {
        self.waiting.push((id, body.to_string()));
        Reply::Stored
    }
    fn store_later(&mut self, table: &mut Table) {
        for (id, body) in self.waiting.drain(..) {
            table.entry(id).or_insert(body);
        }
    }
    fn crash(&mut self) {
        self.waiting.clear();
    }
}

struct Rng(u64);
impl Rng {
    fn one_in(&mut self, n: u64) -> bool {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.is_multiple_of(n)
    }
}

/// A sender delivers events 0..n. It repeats each one until it SEES a 2xx. Writes fail and replies get lost.
/// Returns the table and the number of deliveries that were repeats of a stored event.
fn run_store_first(seed: u64, n: u64) -> (Table, u32) {
    let (mut table, mut rng, mut repeats) = (Table::new(), Rng(seed), 0);
    for id in 0..n {
        loop {
            let reply = receive(&mut table, id, &format!("event {id}"), !rng.one_in(5));
            if reply == Reply::Duplicate {
                repeats += 1;
            }
            let reply_lost = rng.one_in(4); // also covers a crash right after the write
            if matches!(reply, Reply::Stored | Reply::Duplicate) && !reply_lost {
                break;
            }
        }
    }
    (table, repeats)
}

/// The same sender, with a receiver that answers first. Returns how many answered events were lost.
fn run_ack_first(seed: u64, n: u64) -> u64 {
    let (mut table, mut inbox, mut rng) = (Table::new(), AckFirst::default(), Rng(seed));
    for id in 0..n {
        assert_eq!(inbox.receive(id, &format!("event {id}")), Reply::Stored);
        if rng.one_in(10) {
            inbox.crash(); // the sender has its 200 and will not send these again
        } else if rng.one_in(3) {
            inbox.store_later(&mut table);
        }
    }
    inbox.store_later(&mut table);
    n - table.len() as u64
}

fn main() {
    let mut table = Table::new();
    assert_eq!(receive(&mut table, 1, "paid 50", false), Reply::TryAgain);
    assert!(
        table.is_empty(),
        "a failed write leaves no trace, so the retry counts as new"
    );
    assert_eq!(receive(&mut table, 1, "paid 50", true), Reply::Stored);
    assert_eq!(receive(&mut table, 1, "paid 50", true), Reply::Duplicate);
    assert_eq!(receive(&mut table, 1, "paid 99", true), Reply::Conflict);
    assert_eq!(table[&1], "paid 50");

    let (mut repeats, mut lost) = (0, 0);
    for seed in 1..=200 {
        let (table, again) = run_store_first(seed * 0x9E37_79B9, 50);
        // every event the sender gave up on is stored, once, with its own body
        let expected: Table = (0..50).map(|id| (id, format!("event {id}"))).collect();
        assert_eq!(table, expected);
        repeats += again;
        lost += run_ack_first(seed * 0x9E37_79B9, 50);
    }
    assert!(repeats > 0 && lost > 0);
    println!("store first: 10000 events, none lost, {repeats} repeats recognised");
    println!("answer first: {lost} of 10000 events answered with 200 and then lost");
}
