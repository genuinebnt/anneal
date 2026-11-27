use solution::*;

#[allow(dead_code)]
fn cid(n: u64) -> CmdId {
    CmdId::Client(n)
}

#[allow(dead_code)]
fn send(to: &str, amount: i64, transfer: u64) -> Command {
    Command::TransferOut { transfer, to: to.into(), amount }
}

/// A store whose appends can fail or be beaten to it, on command.
#[allow(dead_code)]
#[derive(Default)]
struct Shaky {
    inner: MemStore,
    appends: u64,
    /// the numbers (from 0) of the appends that fail with an I/O error
    fail: Vec<u64>,
    /// the numbers of the appends that find somebody else's deposit of 1 written first
    rival: Vec<u64>,
    /// before the append with this number, this account sends this much to that one
    rival_send: Option<(u64, &'static str, &'static str, i64)>,
}

impl EventStore for Shaky {
    fn append(&mut self, stream: &str, expected: u64, events: Vec<NewEvent>) -> Result<Vec<Stored>, StoreError> {
        let n = self.appends;
        self.appends += 1;
        assert!(n < 10_000, "far too many appends");
        if self.fail.contains(&n) {
            return Err(StoreError::Io(std::io::Error::other("disk")));
        }
        if self.rival.contains(&n) {
            let event = Event::Deposited { amount: 1 };
            let other = NewEvent::current(CmdId::Client(900_000 + n), 0, 0, &event);
            let now = self.inner.version(stream);
            self.inner.append(stream, now, vec![other]).unwrap();
        }
        if let Some((_, from, to, amount)) = self.rival_send.filter(|r| r.0 == n) {
            let event = Event::TransferredOut { transfer: 77, to: to.into(), amount };
            let other = NewEvent::current(CmdId::Client(800_000 + n), 0, 0, &event);
            let now = self.inner.version(from);
            self.inner.append(from, now, vec![other]).unwrap();
        }
        self.inner.append(stream, expected, events)
    }
    fn read_stream(&self, stream: &str, after: u64) -> Vec<Stored> {
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

/// Opens each account with its starting balance.
#[allow(dead_code)]
fn bank(accounts: &[(&str, i64)]) -> Ledger<Shaky> {
    let mut l = Ledger::new(Shaky::default(), 3);
    for (name, balance) in accounts {
        l.execute(name, cid(1), &Command::Open).unwrap();
        if *balance > 0 {
            l.execute(name, cid(2), &Command::Deposit { amount: *balance }).unwrap();
        }
    }
    l
}

#[allow(dead_code)]
fn balance(l: &Ledger<Shaky>, name: &str) -> i64 {
    l.load(name).balance
}

#[test]
fn returned_money_does_not_also_arrive_later() {
    let mut l = bank(&[("ann", 100)]);
    l.execute("ann", cid(3), &send("bo", 40, 1)).unwrap();
    Courier { next_pos: 0 }.tick(&mut l).unwrap();
    // bo opens an account afterwards, and a new courier starts from the beginning
    l.execute("bo", cid(1), &Command::Open).unwrap();
    let t = Courier { next_pos: 0 }.tick(&mut l).unwrap();
    check!("nothing happens", (t.received, t.bounced), (0, 0));
    check!("balances", (balance(&l, "ann"), balance(&l, "bo")), (100, 0));
}

#[test]
fn a_target_that_is_too_full_sends_it_back() {
    let mut l = bank(&[("ann", 100), ("rich", i64::MAX)]);
    l.execute("ann", cid(3), &send("rich", 40, 1)).unwrap();
    let t = Courier { next_pos: 0 }.tick(&mut l).unwrap();
    check!("bounced", t.bounced, 1);
    check!("ann", balance(&l, "ann"), 100);
}

#[test]
fn money_that_cannot_go_anywhere_is_reported() {
    let mut l = bank(&[("ann", i64::MAX)]);
    l.execute("ann", cid(3), &send("ghost", 10, 1)).unwrap(); // line 2
    l.execute("ann", cid(4), &Command::Deposit { amount: 10 }).unwrap();
    l.execute("ann", cid(5), &send("ann", 1, 2)).unwrap();
    let mut c = Courier { next_pos: 0 };
    // ann is full again before the courier comes, but for the 1 she sent to herself
    l.execute("ann", cid(6), &Command::Deposit { amount: 1 }).unwrap();
    let t = c.tick(&mut l).unwrap();
    check!("stuck", t.stuck.clone(), vec![2, 4]);
    check!("counts", (t.received, t.bounced), (0, 0));
    check!("moved on", c.next_pos >= 6, true);
}

#[test]
fn sending_to_yourself() {
    let mut l = bank(&[("ann", 10)]);
    l.execute("ann", cid(3), &send("ann", 10, 1)).unwrap();
    Courier { next_pos: 0 }.tick(&mut l).unwrap();
    check!("ann", balance(&l, "ann"), 10);
}

#[test]
fn somebody_else_writes_while_the_courier_does() {
    for _ in 0..20 {
        let mut l = bank(&[("ann", 100), ("bo", 0)]);
        l.execute("ann", cid(3), &send("bo", 30, 1)).unwrap();
        let next = l.store.appends;
        l.store.rival = vec![next, next + 1, next + 2];
        let t = Courier { next_pos: 0 }.tick(&mut l).unwrap();
        check!("received", t.received, 1);
        check!("bo", balance(&l, "bo"), 33);
    }
}

#[test]
fn a_failed_write_keeps_the_transfer_to_be_seen_again() {
    let mut l = bank(&[("ann", 100), ("bo", 0), ("cy", 0)]);
    l.execute("ann", cid(3), &send("bo", 10, 1)).unwrap(); // line 4
    l.execute("ann", cid(4), &send("cy", 20, 2)).unwrap(); // line 5
    let next = l.store.appends;
    l.store.fail = vec![next + 1];
    let mut c = Courier { next_pos: 0 };
    check!("io", matches!(c.tick(&mut l), Err(StoreError::Io(_))), true);
    check!("not past the failed one", c.next_pos <= 5, true);
    let t = c.tick(&mut l).unwrap();
    check!("the second arrives now", t.received, 1);
    check!("balances", (balance(&l, "bo"), balance(&l, "cy")), (10, 20));
}

#[test]
fn a_failed_return_is_tried_again() {
    let mut l = bank(&[("ann", 100)]);
    l.execute("ann", cid(3), &send("ghost", 10, 1)).unwrap();
    let next = l.store.appends;
    l.store.fail = vec![next];
    let mut c = Courier { next_pos: 0 };
    check!("io", c.tick(&mut l).is_err(), true);
    check!("bounced", c.tick(&mut l).unwrap().bounced, 1);
    check!("ann", balance(&l, "ann"), 100);
}

#[test]
fn it_starts_where_it_is_told() {
    let mut l = bank(&[("ann", 100), ("bo", 0)]);
    l.execute("ann", cid(3), &send("bo", 30, 1)).unwrap();
    let mut c = Courier { next_pos: 999 };
    check!("nothing", c.tick(&mut l).unwrap().received, 0);
    check!("never moves back", c.next_pos >= 999, true);
}

#[test]
fn random_days_with_crashes_and_failures_conserve_money() {
    let mut rng = anneal_prelude::Rng::new(7206);
    let names = ["a", "b", "c", "d"];
    for case in 0..150 {
        // "d" is never opened
        let mut l = bank(&[("a", 100), ("b", 100), ("c", 100)]);
        let mut courier = Courier { next_pos: 0 };
        let mut older = vec![0];
        let mut stuck = 0;
        let mut trace = Vec::new();
        for n in 10..50u64 {
            match rng.below(6) {
                0..=2 => {
                    let from = *rng.pick(&names[..3]);
                    let to = *rng.pick(&names);
                    let amount = 1 + rng.below(60) as i64;
                    let transfer = rng.below(3) as u64;
                    trace.push(format!("{from}->{to} {amount} #{transfer}"));
                    let _ = l.execute(from, cid(n), &send(to, amount, transfer));
                }
                3 => {
                    courier = Courier { next_pos: *rng.pick(&older) };
                    trace.push(format!("restart at {}", courier.next_pos));
                }
                _ => {
                    if rng.below(3) == 0 {
                        let next = l.store.appends;
                        l.store.fail = vec![next + rng.below(2) as u64];
                    }
                    if let Ok(t) = courier.tick(&mut l) {
                        stuck += t.stuck.len();
                    }
                    older.push(courier.next_pos);
                    trace.push("tick".into());
                }
            }
        }
        l.store.fail.clear();
        let mut courier = Courier { next_pos: *rng.pick(&older) };
        courier.tick(&mut l).unwrap();
        courier.tick(&mut l).unwrap();
        let input = format!("case {case}: {}", trace.join("; "));
        check!(input.clone(), stuck, 0);
        let total: i64 = ["a", "b", "c", "d"].iter().map(|s| balance(&l, s)).sum();
        check!(format!("no money is made or lost: {input}"), total, 300);
        check!(input.clone(), balance(&l, "d"), 0);
        // every transfer out has exactly one line that answers it
        let lines = l.store.read_all(0).to_vec();
        for line in &lines {
            if let Ok(Event::TransferredOut { .. }) = decode(line) {
                let answers = lines.iter().filter(|a| a.cmd == CmdId::Reaction(line.pos) || a.cmd == CmdId::Bounce(line.pos)).count();
                check!(format!("answers to line {}: {input}", line.pos), answers, 1);
            }
        }
    }
}

#[test]
fn a_transfer_sent_during_a_tick_is_not_skipped() {
    for _ in 0..20 {
        let mut l = bank(&[("ann", 100), ("bo", 0), ("cy", 50)]);
        l.execute("ann", cid(3), &send("bo", 30, 1)).unwrap();
        // cy's transfer lands in the log while the courier is delivering ann's
        l.store.rival_send = Some((l.store.appends, "cy", "bo", 20));
        let mut c = Courier { next_pos: 0 };
        let first = c.tick(&mut l).unwrap();
        let second = c.tick(&mut l).unwrap();
        check!("both are delivered, by the second tick at the latest", first.received + second.received, 2);
        check!("balances", (balance(&l, "bo"), balance(&l, "cy")), (50, 30));
    }
}
