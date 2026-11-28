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

#[allow(dead_code)]
fn only(b: &Books, finding: Finding) {
    check!(format!("{finding:?} in {:?}", b.lines), audit(&b.lines, None, false), vec![finding]);
}

#[test]
fn every_rule_of_the_account_is_checked() {
    let cases: Vec<(&str, Event)> = vec![
        ("ann", Event::Opened),
        ("ann", Event::Deposited { amount: 0 }),
        ("ann", wd(-5)),
        ("ann", Event::Reversed { target: 4, delta: 20 }),
        ("ann", Event::Reversed { target: 1, delta: 0 }),
        ("ann", Event::Reversed { target: 2, delta: -99 }),
        ("ann", Event::Deposited { amount: i64::MAX }),
        ("ann", Event::TransferredOut { transfer: 1, to: "bo".into(), amount: 1_000 }),
        ("new", Event::Deposited { amount: 5 }),
    ];
    for (stream, event) in cases {
        let mut b = clean();
        let at = b.add(stream, event);
        only(&b, Finding::Impossible { at });
    }
}

#[test]
fn a_bad_line_still_counts_for_what_comes_after() {
    let mut b = Books::default();
    b.open("a", 10);
    let bad = b.add("a", wd(50)); // version 3, and the balance is now -40
    let next = b.add("a", wd(1));
    check!("both", audit(&b.lines, None, false), vec![Finding::Impossible { at: bad }, Finding::Impossible { at: next }]);
}

#[test]
fn unreadable_lines_keep_their_place_in_the_history() {
    let mut b = Books::default();
    b.open("a", 10);
    let junk = b.add("a", Event::Opened);
    b.lines[junk as usize].schema = 9;
    b.add("a", wd(4)); // version 4
    b.add("a", Event::Reversed { target: 4, delta: 4 });
    check!("only the junk", audit(&b.lines, None, false), vec![Finding::Unreadable { at: junk }]);
    let reports = Reports::default();
    check!("reports cannot be trusted", audit(&b.lines, Some(&reports), false), vec![Finding::Unreadable { at: junk }, Finding::ReportsWrong]);
}

#[test]
fn a_command_that_ran_twice() {
    let mut b = clean();
    let first = b.lines[1].cmd;
    let at = b.line("ann", first, Event::Deposited { amount: 100 });
    only(&b, Finding::CommandRepeated { at });
}

#[test]
fn the_same_id_in_another_stream_is_fine() {
    let mut b = clean();
    let first = b.lines[1].cmd;
    b.line("bo", first, Event::Deposited { amount: 100 });
    check!("clean", audit(&b.lines, None, true), vec![]);
}

#[test]
fn answers_that_do_not_fit_their_transfer() {
    let base = || {
        let mut b = Books::default();
        b.open("ann", 100);
        b.open("bo", 0);
        b.open("cy", 0);
        let s = b.send("ann", "bo", 30);
        (b, s)
    };
    let tin = |transfer, from: &str, amount| Event::TransferredIn { transfer, from: from.into(), amount };
    let wrong: Vec<(&str, &str, u64, &str, i64)> = vec![
        // (what is wrong, the stream, transfer, from, amount)
        ("another account", "cy", 5, "ann", 30),
        ("another sender", "bo", 5, "cy", 30),
        ("another number", "bo", 6, "ann", 30),
        ("another amount", "bo", 5, "ann", 31),
    ];
    for (what, stream, transfer, from, amount) in wrong {
        let (mut b, s) = base();
        let at = b.line(stream, CmdId::Reaction(s), tin(transfer, from, amount));
        check!(what, audit(&b.lines, None, false), vec![Finding::Unexplained { at }]);
        check!(format!("{what}: the transfer is still open"), audit(&b.lines, None, true), vec![Finding::Unexplained { at }, Finding::Unanswered { at: s }]);
    }
    // after a wrong answer, the honest one is the first answer
    for (transfer, amount) in [(6, 30), (5, 31)] {
        let (mut b, s) = base();
        let at = b.line("bo", CmdId::Reaction(s), tin(transfer, "ann", amount));
        b.line("ann", CmdId::Bounce(s), tin(5, "bo", 30));
        check!("wrong, then right", audit(&b.lines, None, true), vec![Finding::Unexplained { at }]);
    }
    // an answer that does not fit answers nothing
    let (mut b, s) = base();
    let at = b.line("cy", CmdId::Reaction(s), tin(5, "ann", 30));
    check!("still open", audit(&b.lines, None, true), vec![Finding::Unexplained { at }, Finding::Unanswered { at: s }]);
    // a return must go to the sender and say it came from the target
    let (mut b, s) = base();
    let at = b.line("bo", CmdId::Bounce(s), tin(5, "ann", 30));
    check!("a return to the target", audit(&b.lines, None, false), vec![Finding::Unexplained { at }]);
    // an id that points at a line which sent nothing
    let (mut b, _) = base();
    let at = b.line("bo", CmdId::Reaction(1), tin(5, "ann", 100));
    check!("points at a deposit", audit(&b.lines, None, false), vec![Finding::Unexplained { at }]);
}

#[test]
fn an_answer_cannot_come_before_its_transfer() {
    let mut b = Books::default();
    b.open("ann", 100);
    b.open("bo", 0);
    let at = b.arrive(4, "ann", "bo", 30); // line 3
    b.send("ann", "bo", 30); // line 4
    check!("too early", audit(&b.lines, None, false), vec![Finding::Unexplained { at }]);
}

#[test]
fn both_answers() {
    let mut b = Books::default();
    b.open("ann", 100);
    b.open("bo", 0);
    let s = b.send("ann", "bo", 30);
    b.arrive(s, "ann", "bo", 30);
    let at = b.come_back(s, "ann", "bo", 30);
    only(&b, Finding::AnsweredTwice { at });
}

#[test]
fn out_of_sequence_hides_everything_after_it() {
    for change in 0..3 {
        let mut b = clean();
        b.add("ann", wd(10_000));
        match change {
            0 => b.lines[5].pos = 99,
            1 => b.lines[5].version += 1,
            _ => b.lines.swap(5, 6),
        }
        check!(change, audit(&b.lines, None, true), vec![Finding::OutOfSequence { at: 5 }]);
    }
}

#[test]
fn reports_that_lag_are_fine_and_reports_that_are_ahead_are_not() {
    let b = clean();
    for end in 0..=b.lines.len() {
        let r = Reports::from_log(&b.lines[..end]);
        check!(format!("counted {end}"), audit(&b.lines, Some(&r), true), vec![]);
    }
    let all = Reports::from_log(&b.lines);
    check!("ahead", audit(&b.lines[..6], Some(&all), false), vec![Finding::ReportsWrong]);
}

#[test]
fn several_findings_come_sorted() {
    let mut b = clean();
    let lost = b.send("ann", "bo", 1);
    let fake = b.add("bo", Event::TransferredIn { transfer: 5, from: "ann".into(), amount: 9 });
    let over = b.add("ann", wd(10_000));
    let got = audit(&b.lines, None, true);
    check!("all three", got, vec![Finding::Impossible { at: over }, Finding::Unexplained { at: fake }, Finding::Unanswered { at: lost }]);
}

#[test]
fn random_honest_books_pass_and_one_forgery_is_caught() {
    let mut rng = anneal_prelude::Rng::new(7208);
    let names = ["a", "b", "c"];
    for case in 0..200 {
        let mut b = Books::default();
        let mut balances = [100i64; 3];
        for n in names {
            b.open(n, 100);
        }
        let mut waiting: Vec<(u64, usize, usize, i64)> = Vec::new();
        for _ in 0..rng.below(30) {
            let who = rng.below(3);
            let amount = 1 + rng.below(40) as i64;
            match rng.below(5) {
                0 => {
                    b.add(names[who], Event::Deposited { amount });
                    balances[who] += amount;
                }
                1 if balances[who] >= amount => {
                    b.add(names[who], wd(amount));
                    balances[who] -= amount;
                }
                2 | 3 if balances[who] >= amount => {
                    let to = rng.below(3);
                    let s = b.send(names[who], names[to], amount);
                    balances[who] -= amount;
                    waiting.push((s, who, to, amount));
                }
                _ if !waiting.is_empty() => {
                    let (s, from, to, amount) = waiting.swap_remove(rng.below(waiting.len()));
                    if rng.below(4) == 0 {
                        b.come_back(s, names[from], names[to], amount);
                        balances[from] += amount;
                    } else {
                        b.arrive(s, names[from], names[to], amount);
                        balances[to] += amount;
                    }
                }
                _ => {}
            }
        }
        let input = format!("case {case}: {:?}", b.lines);
        let reports = Reports::from_log(&b.lines);
        let mut open: Vec<Finding> = waiting.iter().map(|w| Finding::Unanswered { at: w.0 }).collect();
        open.sort();
        check!(input.clone(), audit(&b.lines, Some(&reports), false), vec![]);
        check!(input.clone(), audit(&b.lines, None, true), open);
        // forge one line
        let who = rng.below(3);
        let at = match rng.below(2) {
            0 => b.add(names[who], wd(balances[who] + 1)),
            _ => b.add(names[who], Event::TransferredIn { transfer: 5, from: "a".into(), amount: 1 }),
        };
        let got = audit(&b.lines, None, false);
        check!(input, got.iter().any(|f| matches!(f, Finding::Impossible { at: x } | Finding::Unexplained { at: x } if *x == at)) && got.len() == 1, true);
    }
}

#[test]
fn long_books_stay_quick() {
    let mut b = Books::default();
    for i in 0..2_000 {
        b.open(&format!("acct-{i}"), 10);
    }
    for i in 0..10_000 {
        let from = format!("acct-{}", i % 2000);
        let to = format!("acct-{}", (i + 1) % 2000);
        let s = b.send(&from, &to, 1);
        b.arrive(s, &from, &to, 1);
    }
    let reports = Reports::from_log(&b.lines);
    check!("clean", audit(&b.lines, Some(&reports), true), vec![]);
}

#[test]
fn returns_that_do_not_fit_their_transfer() {
    let base = || {
        let mut b = Books::default();
        b.open("ann", 100);
        b.open("bo", 0);
        b.open("cy", 0);
        let s = b.send("ann", "bo", 30);
        (b, s)
    };
    let tin = |transfer, from: &str, amount| Event::TransferredIn { transfer, from: from.into(), amount };
    let wrong: Vec<(&str, &str, u64, &str, i64)> = vec![
        // (what is wrong, the stream, transfer, from, amount); an honest return is on "ann", from "bo"
        ("another account", "cy", 5, "bo", 30),
        ("another target named", "ann", 5, "cy", 30),
        ("another number", "ann", 6, "bo", 30),
        ("another amount", "ann", 5, "bo", 31),
    ];
    for (what, stream, transfer, from, amount) in wrong {
        let (mut b, s) = base();
        let at = b.line(stream, CmdId::Bounce(s), tin(transfer, from, amount));
        check!(what, audit(&b.lines, None, true), vec![Finding::Unexplained { at }, Finding::Unanswered { at: s }]);
    }
}

#[test]
fn unreadable_lines_use_up_their_command_id_too() {
    let mut b = clean();
    let first = b.lines[1].cmd;
    let junk = b.line("ann", first, Event::Opened);
    b.lines[junk as usize].schema = 9;
    check!("an unreadable repeat", audit(&b.lines, None, false), vec![Finding::Unreadable { at: junk }, Finding::CommandRepeated { at: junk }]);
    let mut b = clean();
    let junk = b.line("ann", CmdId::Client(500), Event::Opened);
    b.lines[junk as usize].schema = 9;
    let at = b.line("ann", CmdId::Client(500), wd(1));
    check!("a repeat of an unreadable line", audit(&b.lines, None, false), vec![Finding::Unreadable { at: junk }, Finding::CommandRepeated { at }]);
}
