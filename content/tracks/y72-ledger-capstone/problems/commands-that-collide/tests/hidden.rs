use solution::*;

use std::cell::RefCell;

#[allow(dead_code)]
const A: &str = "ann";

#[allow(dead_code)]
fn id(n: u64) -> CmdId {
    CmdId::Client(n)
}

#[allow(dead_code)]
fn dep(amount: i64) -> Command {
    Command::Deposit { amount }
}

#[allow(dead_code)]
fn spend(amount: i64) -> Command {
    Command::Withdraw { amount, reason: "rent".into() }
}

/// A ledger with one open account holding `balance`.
#[allow(dead_code)]
fn bank(every: u64, balance: i64) -> Ledger<MemStore> {
    let mut l = Ledger::new(MemStore::new(), every);
    l.execute(A, id(1), &Command::Open).unwrap();
    if balance > 0 {
        l.execute(A, id(2), &dep(balance)).unwrap();
    }
    l
}

#[allow(dead_code)]
fn is_conflict(r: &Result<Outcome, CmdError>) -> bool {
    matches!(r, Err(CmdError::Store(StoreError::Conflict { .. })))
}

#[allow(dead_code)]
fn refusal(r: Result<Outcome, CmdError>) -> Option<DomainError> {
    match r {
        Err(CmdError::Domain(d)) => Some(d),
        _ => None,
    }
}

/// A store that writes down what is asked of it, and can let somebody else write first.
#[allow(dead_code)]
#[derive(Default)]
struct Watched {
    inner: MemStore,
    /// the `after` of every `read_stream`
    reads: RefCell<Vec<u64>>,
    /// how many of the next appends find that a deposit of 1 got there first
    rivals: u32,
    rival_ids: u64,
    /// fail the next append with an I/O error
    broken: bool,
    appends: u32,
}

impl EventStore for Watched {
    fn append(&mut self, stream: &str, expected: u64, events: Vec<NewEvent>) -> Result<Vec<Stored>, StoreError> {
        self.appends += 1;
        assert!(self.appends < 1000, "far too many appends");
        if std::mem::take(&mut self.broken) {
            return Err(StoreError::Io(std::io::Error::other("disk")));
        }
        if self.rivals > 0 {
            self.rivals -= 1;
            self.rival_ids += 1;
            let event = Event::Deposited { amount: 1 };
            let rival = NewEvent::current(CmdId::Client(1_000_000 + self.rival_ids), 0, 0, &event);
            let now = self.inner.version(stream);
            self.inner.append(stream, now, vec![rival]).unwrap();
        }
        self.inner.append(stream, expected, events)
    }
    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
        self.reads.borrow_mut().push(after);
        self.inner.read_stream(stream, after)
    }
    fn read_all(&self, from: u64) -> &[Stored] {
        self.inner.read_all(from)
    }
    fn version(&self, stream: &str) -> u64 {
        self.inner.version(stream)
    }
    fn streams(&self) -> Vec<String> {
        self.inner.streams()
    }
}

#[allow(dead_code)]
fn watched(every: u64) -> Ledger<Watched> {
    let mut l = Ledger::new(Watched::default(), every);
    l.execute(A, id(1), &Command::Open).unwrap();
    l
}

#[test]
fn execute_tries_again_after_each_conflict() {
    for _ in 0..20 {
        let mut l = watched(0);
        l.store.rivals = 3;
        let got = l.execute(A, id(2), &dep(10));
        check!("applied in the end", matches!(got, Ok(Outcome::Applied(1))), true);
        check!("balance", l.load(A).balance, 13);
    }
}

#[test]
fn a_retry_decides_again_from_what_is_there_now() {
    let mut l = watched(0);
    l.execute(A, id(2), &dep(5)).unwrap();
    l.execute(A, id(3), &dep(1)).unwrap();
    l.store.rivals = 1;
    let got = l.execute(A, id(4), &spend(6));
    check!("applied", matches!(got, Ok(Outcome::Applied(1))), true);
    check!("balance", l.load(A).balance, 1);
}

#[test]
fn execute_does_not_retry_other_failures() {
    let mut l = watched(0);
    l.store.broken = true;
    let before = l.store.appends;
    check!("io", matches!(l.execute(A, id(2), &dep(1)), Err(CmdError::Store(StoreError::Io(_)))), true);
    check!("one append", l.store.appends - before, 1);
    check!("refusal", refusal(l.execute(A, id(3), &spend(1))), Some(DomainError::Insufficient { have: 0, need: 1 }));
}

#[test]
fn load_reads_only_what_came_after_the_snapshot() {
    let mut l = watched(3);
    for n in 2..=7 {
        l.execute(A, id(n), &dep(1)).unwrap();
    }
    check!("snapshot", l.snapshot_version(A), Some(6));
    l.store.reads.borrow_mut().clear();
    check!("balance", l.load(A).balance, 6);
    check!("reads", l.store.reads.borrow().clone(), vec![6]);
}

#[test]
fn snapshots_can_be_turned_off_or_taken_every_time() {
    let l = bank(0, 5);
    check!("never", l.snapshot_version(A), None);
    let l = bank(1, 5);
    check!("every event", l.snapshot_version(A), Some(2));
    let l = bank(u64::MAX, 5);
    check!("huge", l.snapshot_version(A), None);
}

#[test]
fn snapshots_are_per_stream() {
    let mut l = bank(2, 5);
    l.execute("bo", id(1), &Command::Open).unwrap();
    check!("ann", l.snapshot_version(A), Some(2));
    check!("bo", l.snapshot_version("bo"), None);
    check!("bo's balance", l.load("bo").balance, 0);
}

#[test]
fn a_duplicate_is_known_after_a_snapshot() {
    let mut l = bank(2, 5);
    l.execute(A, id(3), &dep(1)).unwrap();
    l.execute(A, id(4), &dep(1)).unwrap();
    check!("old id", matches!(l.execute(A, id(2), &dep(5)), Ok(Outcome::Duplicate)), true);
    check!("old id, other command", refusal(l.execute(A, id(1), &dep(5))), Some(DomainError::IdReused));
}

#[test]
fn a_duplicate_is_noticed_before_the_rules() {
    let mut l = bank(0, 10);
    l.execute(A, id(3), &spend(10)).unwrap();
    // the money is gone, but this command was done
    check!("again", matches!(l.execute(A, id(3), &spend(10)), Ok(Outcome::Duplicate)), true);
}

#[test]
fn history_without_a_fingerprint_counts_as_done() {
    let mut store = MemStore::new();
    let old = NewEvent::current(id(1), 0, 0, &Event::Opened);
    store.append(A, 0, vec![old]).unwrap();
    let mut l = Ledger::new(store, 0);
    check!("same id", matches!(l.execute(A, id(1), &dep(3)), Ok(Outcome::Duplicate)), true);
}

#[test]
fn ids_belong_to_one_account_and_kinds_differ() {
    let mut l = bank(0, 5);
    l.execute("bo", id(1), &Command::Open).unwrap();
    check!("same number, other account", matches!(l.execute("bo", id(2), &dep(1)), Ok(Outcome::Applied(1))), true);
    check!("a reaction id with the same number", matches!(l.execute(A, CmdId::Reaction(2), &dep(1)), Ok(Outcome::Applied(1))), true);
    check!("a bounce id with the same number", matches!(l.execute(A, CmdId::Bounce(2), &spend(1)), Ok(Outcome::Applied(1))), true);
}

#[test]
fn a_rejected_command_is_not_remembered() {
    let mut l = bank(0, 0);
    l.execute(A, id(2), &spend(5)).unwrap_err();
    l.execute(A, id(3), &dep(5)).unwrap();
    check!("now it fits", matches!(l.execute(A, id(2), &spend(5)), Ok(Outcome::Applied(1))), true);
}

#[test]
fn many_writers_in_a_random_order_agree_with_the_log() {
    let mut rng = anneal_prelude::Rng::new(7203);
    for _ in 0..150 {
        let every = rng.below(4) as u64;
        let mut l = bank(every, 50);
        let mut waiting: Vec<(Account, u64, Command)> = Vec::new();
        let mut done: Vec<(u64, Command)> = Vec::new();
        for next in 10..70u64 {
            if waiting.is_empty() || rng.below(2) == 0 {
                let command = if rng.below(2) == 0 { dep(1 + rng.below(30) as i64) } else { spend(1 + rng.below(60) as i64) };
                // now and then an old command is sent once more
                let (n, command) = if !done.is_empty() && rng.below(5) == 0 { rng.pick(&done).clone() } else { (next, command) };
                waiting.push((l.load(A), n, command));
            } else {
                let (loaded, n, command) = waiting.swap_remove(rng.below(waiting.len()));
                let current = loaded.version == l.store.version(A);
                let before = l.store.version(A);
                let was_done = done.iter().any(|d| d.0 == n);
                let got = l.try_commit(A, &loaded, id(n), &command);
                let input = format!("every {every}, step {next}, id {n}, {command:?}");
                match got {
                    Ok(Outcome::Applied(count)) => {
                        check!(input.clone(), (current, was_done, count), (true, false, 1));
                        done.push((n, command));
                    }
                    Ok(Outcome::Duplicate) => check!(input.clone(), was_done, true),
                    Err(CmdError::Domain(d)) => {
                        check!(format!("a refusal needs a current account: {input}"), current, true);
                        check!(input.clone(), decide(&loaded, &command), Err(d));
                    }
                    Err(CmdError::Store(StoreError::Conflict { .. })) => {
                        check!(input.clone(), current, false);
                        check!(input.clone(), l.store.version(A), before);
                    }
                    Err(CmdError::Store(other)) => panic!("{other:?}"),
                }
                check!(input.clone(), l.load(A), l.state_at(A, u64::MAX));
                check!(input, l.load(A).balance >= 0, true);
            }
        }
        let events: Vec<Event> = l.store.read_all(0).iter().map(|s| decode(s).unwrap()).collect();
        check!("balance from the events", l.load(A).balance, replay(&events).balance);
    }
}

#[test]
fn every_kind_of_stale_refusal_is_a_conflict() {
    let reverse = |target| Command::Reverse { target };
    // (what happened before the load, the command, the refusal a current account gets)
    let cases: Vec<(&str, Vec<Command>, Command, DomainError)> = vec![
        ("already open", vec![Command::Open], Command::Open, DomainError::AlreadyOpen),
        ("not positive", vec![Command::Open], dep(0), DomainError::NonPositive),
        ("insufficient", vec![Command::Open], spend(500), DomainError::Insufficient { have: 1, need: 500 }),
        ("not reversible", vec![Command::Open], reverse(40), DomainError::NotReversible(40)),
        ("already reversed", vec![Command::Open, dep(5), reverse(2)], reverse(2), DomainError::AlreadyReversed(2)),
        ("overflow", vec![Command::Open, dep(i64::MAX - 1)], dep(5), DomainError::Overflow),
    ];
    for (what, before, command, why) in cases {
        let mut l = Ledger::new(MemStore::new(), 0);
        for (n, c) in (1..).zip(&before) {
            l.execute(A, id(n), c).unwrap();
        }
        let stale = l.load(A);
        l.execute(A, id(50), &dep(1)).unwrap();
        let got = l.try_commit(A, &stale, id(60), &command);
        check!(format!("{what}, from an old account"), is_conflict(&got), true);
        let fresh = l.load(A);
        check!(format!("{what}, from today's"), refusal(l.try_commit(A, &fresh, id(61), &command)), Some(why));
    }
    let mut l = Ledger::new(MemStore::new(), 0);
    let never_opened = l.load(A);
    l.execute(A, id(1), &Command::Open).unwrap();
    check!("not open, from an old account", is_conflict(&l.try_commit(A, &never_opened, id(2), &dep(1))), true);
}
