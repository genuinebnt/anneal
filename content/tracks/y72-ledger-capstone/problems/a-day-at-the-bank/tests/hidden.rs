use solution::*;

use std::io;

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

/// A file in memory that can be made to fail.
#[allow(dead_code)]
#[derive(Default)]
struct Mem {
    bytes: Vec<u8>,
    appends: u32,
    fail_at: Option<u32>,
}

impl Device for Mem {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.appends += 1;
        if self.fail_at == Some(self.appends) {
            return Err(io::Error::other("disk"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_refusal_waits_for_a_current_account() {
    let mut l = bank(&[("ann", 0)]);
    let scripts = vec![vec![spend("ann", 5)], vec![dep("ann", 10)]];
    let mut asked = Vec::new();
    // courier; writer 0 loads an empty account; writer 1 loads and commits; writer 0 tries
    let report = run_day(&mut l, &scripts, by_list(&[2, 0, 1, 1, 0], &mut asked)).unwrap();
    check!("conflicts, applied", (report.conflicts, report.applied), (1, 2));
    check!("refused", report.refused, vec![]);
    check!("balance", l.load("ann").balance, 5);
}

#[test]
fn finished_writers_leave_the_list() {
    let mut l = bank(&[("a", 1)]);
    let scripts = vec![vec![], vec![dep("a", 1)], vec![dep("a", 1), dep("a", 1)]];
    let mut asked = Vec::new();
    run_day(&mut l, &scripts, by_list(&[2], &mut asked)).unwrap();
    // courier, then writer 1 twice, then writer 2 four times, with the courier after each new line when picked last
    check!("first choice: writers 1 and 2 and the courier", asked[0], 3);
    check!("then no courier", asked[1], 2);
    check!("all done", l.load("a").balance, 4);
}

#[test]
fn the_courier_is_listed_last() {
    let mut l = bank(&[("a", 1), ("b", 0)]);
    let scripts = vec![vec![send("a", "b", 1)], vec![dep("b", 5)]];
    // always pick the last one listed
    let report = run_day(&mut l, &scripts, |n| n - 1).unwrap();
    let order: Vec<CmdId> = l.store.read_all(3).iter().map(|s| s.cmd).collect();
    // writer 1 goes first (the courier only for the setup), then writer 0, then the courier delivers
    check!("order of lines", order, vec![command_id(1, 0), command_id(0, 0), CmdId::Reaction(4)]);
    check!("received", report.received, 1);
}

#[test]
fn ids() {
    check!("writer 0, command 0", command_id(0, 0), CmdId::Client(1 << 32));
    check!("writer 2, command 7", command_id(2, 7), CmdId::Client((3 << 32) | 7));
}

#[test]
fn a_store_failure_ends_the_day_with_an_error() {
    let mut l = Ledger::new(FileStore::over(MemStore::new(), Mem::default()), 0);
    open(&mut l, "a", 5);
    l.store.device.fail_at = Some(4);
    let scripts = vec![vec![dep("a", 1), dep("a", 1), dep("a", 1)]];
    let got = run_day(&mut l, &scripts, |_| 0);
    check!("error", matches!(got, Err(StoreError::Io(_))), true);
    check!("what was saved stays", l.load("a").balance, 6);
}

#[allow(dead_code)]
fn random_scripts(rng: &mut anneal_prelude::Rng) -> Vec<Script> {
    let names = ["a", "b", "c", "ghost"];
    (0..1 + rng.below(4))
        .map(|_| {
            (0..rng.below(7))
                .map(|_| {
                    let who = *rng.pick(&names[..3]);
                    let amount = 1 + rng.below(80) as i64;
                    match rng.below(6) {
                        0 => dep(who, amount),
                        1 => spend(who, amount),
                        2 | 3 => send(who, *rng.pick(&names), amount),
                        4 => (who.to_string(), Command::Reverse { target: 1 + rng.below(8) as u64 }),
                        _ => (who.to_string(), Command::Open),
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn random_days_account_for_every_command_and_every_coin() {
    let mut rng = anneal_prelude::Rng::new(7207);
    let mut all_conflicts = 0;
    for case in 0..200 {
        let scripts = random_scripts(&mut rng);
        let seed = rng.next_u64();
        let input = format!("case {case}: {scripts:?}");

        let run = |scripts: &[Script]| {
            let mut dice = anneal_prelude::Rng::new(seed);
            let mut l = bank(&[("a", 100), ("b", 100), ("c", 100)]);
            let report = run_day(&mut l, scripts, |n| dice.below(n)).unwrap();
            (l, report)
        };
        let (l, report) = run(&scripts);
        let (again, report_again) = run(&scripts);
        check!(format!("the same choices give the same day: {input}"), again.store.read_all(0), l.store.read_all(0));
        check!(input.clone(), &report_again, &report);

        let commands: u64 = scripts.iter().map(|s| s.len() as u64).sum();
        check!(format!("each command ends once: {input}"), report.applied + report.duplicates + report.refused.len() as u64, commands);
        check!(input.clone(), report.duplicates, 0);
        all_conflicts += report.conflicts;

        let lines = l.store.read_all(0);
        let mut money = 0i64;
        let mut accounts = std::collections::BTreeMap::<String, Account>::new();
        let (mut sent, mut received, mut bounced) = (0, 0, 0);
        for line in lines {
            let event = decode(line).unwrap();
            match &event {
                Event::Deposited { amount } => money += amount,
                Event::Withdrawn { amount, .. } => money -= amount,
                Event::Reversed { delta, .. } => money += delta,
                Event::TransferredOut { .. } => sent += 1,
                Event::TransferredIn { .. } => match line.cmd {
                    CmdId::Reaction(_) => received += 1,
                    CmdId::Bounce(_) => bounced += 1,
                    CmdId::Client(_) => {}
                },
                Event::Opened => {}
            }
            let account = accounts.remove(&line.stream).unwrap_or_default();
            let next = evolve(account, &event);
            check!(format!("never below zero: {input}"), next.balance >= 0, true);
            accounts.insert(line.stream.clone(), next);
        }
        check!(format!("the day ends with nothing on the way: {input}"), received + bounced, sent);
        check!(input.clone(), (report.received, report.bounced), (received, bounced));
        let total: i64 = accounts.values().map(|a| a.balance).sum();
        check!(format!("money: {input}"), total, money);

        // the same day on a file gives the same journal, byte for byte after reopening
        let mut dice = anneal_prelude::Rng::new(seed);
        let mut f = Ledger::new(FileStore::over(MemStore::new(), Mem::default()), 4);
        for name in ["a", "b", "c"] {
            open(&mut f, name, 100);
        }
        run_day(&mut f, &scripts, |n| dice.below(n)).unwrap();
        let found = recover(&f.store.device.bytes).unwrap();
        check!(format!("file and memory agree: {input}"), found.store.read_all(0), l.store.read_all(0));
    }
    check!("the days really did collide", all_conflicts > 50, true);
}

#[test]
fn a_command_that_already_ran_is_counted_as_a_duplicate() {
    let mut l = bank(&[("a", 0)]);
    // yesterday this exact command, with this id, was done
    l.execute("a", command_id(0, 0), &Command::Deposit { amount: 3 }).unwrap();
    let scripts = vec![vec![dep("a", 3), dep("a", 4)]];
    let report = run_day(&mut l, &scripts, |_| 0).unwrap();
    check!("duplicates, applied", (report.duplicates, report.applied), (1, 1));
    check!("balance", l.load("a").balance, 7);
}

#[test]
fn money_to_nobody_comes_back_during_the_day() {
    let mut l = bank(&[("a", 10)]);
    let scripts = vec![vec![send("a", "ghost", 10)]];
    let report = run_day(&mut l, &scripts, |_| 0).unwrap();
    check!("received, bounced", (report.received, report.bounced), (0, 1));
    check!("balance", l.load("a").balance, 10);
}
