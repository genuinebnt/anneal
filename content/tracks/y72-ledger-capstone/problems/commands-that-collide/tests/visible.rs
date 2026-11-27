use solution::*;

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

#[test]
fn a_command_becomes_events() {
    let mut l = bank(0, 0);
    l.now = 90;
    let got = l.execute(A, id(2), &dep(100));
    check!("applied", matches!(got, Ok(Outcome::Applied(1))), true);
    let line = l.store.read_all(1)[0].clone();
    check!("envelope", (line.cmd, line.ts, line.version, line.print != 0), (id(2), 90, 2, true));
    check!("balance", l.load(A).balance, 100);
}

#[test]
fn sending_a_command_again_does_nothing() {
    let mut l = bank(0, 100);
    check!("again", matches!(l.execute(A, id(2), &dep(100)), Ok(Outcome::Duplicate)), true);
    check!("balance", l.load(A).balance, 100);
}

#[test]
fn an_old_id_cannot_be_used_for_something_else() {
    let mut l = bank(0, 100);
    check!("reused", refusal(l.execute(A, id(2), &spend(1))), Some(DomainError::IdReused));
    check!("lines", l.store.read_all(0).len(), 2);
}

#[test]
fn a_stale_write_is_a_conflict() {
    let mut l = bank(0, 100);
    let stale = l.load(A);
    l.execute(A, id(3), &dep(1)).unwrap();
    check!("stale", is_conflict(&l.try_commit(A, &stale, id(4), &dep(5))), true);
}

#[test]
fn a_stale_no_is_a_conflict_too() {
    let mut l = bank(0, 0);
    let stale = l.load(A);
    l.execute(A, id(3), &dep(100)).unwrap();
    // from the stale account there is no money, from today's there is
    check!("stale refusal", is_conflict(&l.try_commit(A, &stale, id(4), &spend(50))), true);
    let fresh = l.load(A);
    check!("a fresh refusal stands", refusal(l.try_commit(A, &fresh, id(5), &spend(500))), Some(DomainError::Insufficient { have: 100, need: 500 }));
}

#[test]
fn time_travel() {
    let mut l = bank(0, 100);
    l.execute(A, id(3), &spend(30)).unwrap();
    let balances: Vec<i64> = (0..5).map(|v| l.state_at(A, v).balance).collect();
    check!("versions 0 to 4", balances, vec![0, 0, 100, 70, 70]);
}

#[test]
fn a_snapshot_at_each_multiple() {
    let mut l = bank(4, 1);
    l.execute(A, id(3), &dep(1)).unwrap();
    check!("version 3", l.snapshot_version(A), None);
    l.execute(A, id(4), &dep(1)).unwrap();
    check!("version 4", l.snapshot_version(A), Some(4));
    l.execute(A, id(5), &dep(1)).unwrap();
    check!("version 5", l.snapshot_version(A), Some(4));
    check!("same account", l.load(A), l.state_at(A, u64::MAX));
}
