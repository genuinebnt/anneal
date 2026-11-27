use solution::*;

#[allow(dead_code)]
fn open(l: &mut Ledger<impl EventStore>, name: &str, balance: i64) {
    l.execute(name, CmdId::Client(1), &Command::Open).unwrap();
    if balance > 0 {
        l.execute(name, CmdId::Client(2), &Command::Deposit { amount: balance }).unwrap();
    }
}

#[allow(dead_code)]
fn dep(stream: &str, amount: i64) -> (String, Command) {
    (stream.into(), Command::Deposit { amount })
}

#[allow(dead_code)]
fn spend(stream: &str, amount: i64) -> (String, Command) {
    (stream.into(), Command::Withdraw { amount, reason: "r".into() })
}

#[allow(dead_code)]
fn send(stream: &str, to: &str, amount: i64) -> (String, Command) {
    (stream.into(), Command::TransferOut { transfer: 1, to: to.into(), amount })
}

/// Picks by a list; after the list ends, always the first. Writes down how many there were to pick from.
#[allow(dead_code)]
fn by_list<'a>(picks: &'a [usize], asked: &'a mut Vec<usize>) -> impl FnMut(usize) -> usize + 'a {
    move |n| {
        asked.push(n);
        assert!(asked.len() < 10_000, "the day never ends");
        picks.get(asked.len() - 1).copied().unwrap_or(0).min(n - 1)
    }
}

/// A bank with these accounts, all opened before the day starts.
#[allow(dead_code)]
fn bank(accounts: &[(&str, i64)]) -> Ledger<MemStore> {
    let mut l = Ledger::new(MemStore::new(), 4);
    for (name, balance) in accounts {
        open(&mut l, name, *balance);
    }
    l
}

#[test]
fn the_courier_is_listed_while_it_has_lines_to_see() {
    let mut l = bank(&[("ann", 100)]);
    let scripts = vec![vec![dep("ann", 1)]];
    let mut asked = Vec::new();
    // pick the courier first: it sees the two lines of the setup
    let report = run_day(&mut l, &scripts, by_list(&[1], &mut asked)).unwrap();
    // courier; load; commit; courier again for the new line
    check!("how many could move", asked, vec![2, 1, 1, 1]);
    check!("applied", report.applied, 1);
}

#[test]
fn two_writers_who_load_together_collide() {
    let mut l = bank(&[("ann", 0)]);
    let scripts = vec![vec![dep("ann", 1)], vec![dep("ann", 2)]];
    let mut asked = Vec::new();
    // the courier first; then writer 0 loads, writer 1 loads, writer 0 commits, writer 1 tries
    let report = run_day(&mut l, &scripts, by_list(&[2, 0, 1, 0, 0], &mut asked)).unwrap();
    check!("conflicts", report.conflicts, 1);
    check!("applied", report.applied, 2);
    check!("balance", l.load("ann").balance, 3);
    check!("ids", l.store.read_all(1).iter().map(|s| s.cmd).collect::<Vec<_>>(), vec![command_id(0, 0), command_id(1, 0)]);
}

#[test]
fn a_transfer_is_delivered_during_the_day() {
    let mut l = bank(&[("ann", 100), ("bo", 0)]);
    let scripts = vec![vec![send("ann", "bo", 40), spend("ann", 500)]];
    let report = run_day(&mut l, &scripts, |_| 0).unwrap();
    check!("received", report.received, 1);
    check!("refused", report.refused, vec![(0, 1, DomainError::Insufficient { have: 60, need: 500 })]);
    check!("bo", l.load("bo").balance, 40);
}

#[test]
fn a_day_with_nothing_to_do() {
    let mut l = bank(&[]);
    let report = run_day(&mut l, &[], |_| panic!("nobody can move")).unwrap();
    check!("report", report, DayReport::default());
}

#[test]
fn the_choices_decide_the_order() {
    let scripts = vec![vec![dep("a", 1)], vec![dep("b", 1)]];
    let order = |pick_last: bool| {
        let mut l = bank(&[("a", 0), ("b", 0)]);
        run_day(&mut l, &scripts, |n| if pick_last { n - 1 } else { 0 }).unwrap();
        l.store.read_all(2).iter().map(|s| s.stream.clone()).collect::<Vec<_>>()
    };
    check!("first listed", order(false), vec!["a".to_string(), "b".to_string()]);
    check!("last listed", order(true), vec!["b".to_string(), "a".to_string()]);
}
