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
fn money_arrives() {
    let mut l = bank(&[("ann", 100), ("bo", 0)]);
    l.execute("ann", cid(3), &send("bo", 30, 1)).unwrap();
    let mut c = Courier { next_pos: 0 };
    let t = c.tick(&mut l).unwrap();
    check!("counts", (t.received, t.bounced, t.stuck.len()), (1, 0, 0));
    check!("balances", (balance(&l, "ann"), balance(&l, "bo")), (70, 30));
    let arrived = l.store.read_all(0).last().unwrap().clone();
    check!("its id names the sending line", arrived.cmd, CmdId::Reaction(3));
}

#[test]
fn a_courier_that_forgot_delivers_nothing_twice() {
    let mut l = bank(&[("ann", 100), ("bo", 0)]);
    l.execute("ann", cid(3), &send("bo", 30, 1)).unwrap();
    Courier { next_pos: 0 }.tick(&mut l).unwrap();
    let again = Courier { next_pos: 0 }.tick(&mut l).unwrap();
    check!("received", again.received, 0);
    check!("bo", balance(&l, "bo"), 30);
}

#[test]
fn two_senders_may_pick_the_same_number() {
    let mut l = bank(&[("ann", 100), ("bo", 100), ("cy", 0)]);
    l.execute("ann", cid(3), &send("cy", 10, 7)).unwrap();
    l.execute("bo", cid(3), &send("cy", 20, 7)).unwrap();
    Courier { next_pos: 0 }.tick(&mut l).unwrap();
    check!("cy", balance(&l, "cy"), 30);
}

#[test]
fn money_sent_to_nobody_comes_back() {
    let mut l = bank(&[("ann", 100)]);
    l.execute("ann", cid(3), &send("ghost", 25, 1)).unwrap();
    let t = Courier { next_pos: 0 }.tick(&mut l).unwrap();
    check!("bounced", (t.received, t.bounced), (0, 1));
    check!("ann", balance(&l, "ann"), 100);
    let back = l.store.read_all(0).last().unwrap().clone();
    check!("line", (back.stream.clone(), back.cmd), ("ann".to_string(), CmdId::Bounce(2)));
    check!("event", decode(&back), Ok(Event::TransferredIn { transfer: 1, from: "ghost".into(), amount: 25 }));
}

#[test]
fn the_position_moves_on() {
    let mut l = bank(&[("ann", 5)]);
    let mut c = Courier { next_pos: 0 };
    c.tick(&mut l).unwrap();
    check!("next", c.next_pos, 2);
}
