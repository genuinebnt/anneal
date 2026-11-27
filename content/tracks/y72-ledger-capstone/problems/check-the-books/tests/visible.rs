use solution::*;

use std::collections::BTreeMap;

/// Writes a journal by hand, giving each line its places.
#[allow(dead_code)]
#[derive(Default)]
struct Books {
    lines: Vec<Stored>,
    versions: BTreeMap<String, u64>,
    next_cmd: u64,
}

#[allow(dead_code)]
impl Books {
    fn line(&mut self, stream: &str, cmd: CmdId, event: Event) -> u64 {
        let version = self.versions.entry(stream.into()).or_default();
        *version += 1;
        let new = NewEvent::current(cmd, 0, self.lines.len() as u64 * 1000, &event);
        self.lines.push(Stored { pos: self.lines.len() as u64, stream: stream.into(), version: *version, cmd, print: 0, ts: new.ts, schema: new.schema, payload: new.payload });
        self.lines.len() as u64 - 1
    }
    /// A line made by a client with a fresh id.
    fn add(&mut self, stream: &str, event: Event) -> u64 {
        self.next_cmd += 1;
        self.line(stream, CmdId::Client(self.next_cmd), event)
    }
    fn open(&mut self, stream: &str, balance: i64) {
        self.add(stream, Event::Opened);
        if balance > 0 {
            self.add(stream, Event::Deposited { amount: balance });
        }
    }
    fn send(&mut self, from: &str, to: &str, amount: i64) -> u64 {
        self.add(from, Event::TransferredOut { transfer: 5, to: to.into(), amount })
    }
    fn arrive(&mut self, sent: u64, from: &str, to: &str, amount: i64) -> u64 {
        self.line(to, CmdId::Reaction(sent), Event::TransferredIn { transfer: 5, from: from.into(), amount })
    }
    fn come_back(&mut self, sent: u64, from: &str, to: &str, amount: i64) -> u64 {
        self.line(from, CmdId::Bounce(sent), Event::TransferredIn { transfer: 5, from: to.into(), amount })
    }
}

#[allow(dead_code)]
fn wd(amount: i64) -> Event {
    Event::Withdrawn { amount, reason: "r".into() }
}

/// Two accounts, one transfer that arrived and one that came back. Nothing is wrong.
#[allow(dead_code)]
fn clean() -> Books {
    let mut b = Books::default();
    b.open("ann", 100); // lines 0, 1
    b.open("bo", 50); // 2, 3
    let s = b.send("ann", "bo", 30); // 4
    b.arrive(s, "ann", "bo", 30); // 5
    let s = b.send("bo", "ghost", 10); // 6
    b.come_back(s, "bo", "ghost", 10); // 7
    b.add("ann", wd(20)); // 8: ann's version 4
    b.add("ann", Event::Reversed { target: 4, delta: 20 }); // 9
    b
}

#[test]
fn clean_books() {
    let b = clean();
    let reports = Reports::from_log(&b.lines);
    check!("drained, with reports", audit(&b.lines, Some(&reports), true), vec![]);
    check!("empty books", audit(&[], None, true), vec![]);
}

#[test]
fn an_overdraft_that_slipped_through() {
    let mut b = clean();
    let at = b.add("ann", wd(1000));
    check!("withdraw 1000", audit(&b.lines, None, true), vec![Finding::Impossible { at }]);
}

#[test]
fn money_from_nowhere() {
    let mut b = clean();
    let at = b.add("bo", Event::TransferredIn { transfer: 5, from: "ann".into(), amount: 30 });
    check!("a client made it", audit(&b.lines, None, true), vec![Finding::Unexplained { at }]);
}

#[test]
fn money_that_never_arrived() {
    let mut b = clean();
    let at = b.send("ann", "bo", 1);
    check!("on the way", audit(&b.lines, None, false), vec![]);
    check!("after a drained day", audit(&b.lines, None, true), vec![Finding::Unanswered { at }]);
}

#[test]
fn a_report_that_drifted() {
    let b = clean();
    let mut shorter = b.lines.clone();
    shorter[1] = {
        let mut other = Books::default();
        other.open("ann", 101);
        other.lines[1].clone()
    };
    let wrong = Reports::from_log(&shorter);
    check!("reports of other books", audit(&b.lines, Some(&wrong), true), vec![Finding::ReportsWrong]);
}

#[test]
fn a_hole_in_the_journal() {
    let mut b = clean();
    b.lines.remove(3);
    check!("line 3 is gone", audit(&b.lines, None, true), vec![Finding::OutOfSequence { at: 3 }]);
}
