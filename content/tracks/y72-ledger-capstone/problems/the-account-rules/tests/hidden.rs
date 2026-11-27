use solution::*;

#[allow(dead_code)]
fn ask(account: &mut Account, command: Command) -> Result<Vec<Event>, DomainError> {
    let events = decide(account, &command)?;
    for e in &events {
        *account = evolve(std::mem::take(account), e);
    }
    Ok(events)
}

#[allow(dead_code)]
fn with(events: &[Event]) -> Account {
    replay(events)
}

#[allow(dead_code)]
fn dep(amount: i64) -> Event {
    Event::Deposited { amount }
}

#[allow(dead_code)]
fn wd(amount: i64) -> Event {
    Event::Withdrawn { amount, reason: String::new() }
}

#[test]
fn spending_everything_is_allowed() {
    let a = with(&[Event::Opened, dep(5)]);
    check!("withdraw 5", decide(&a, &Command::Withdraw { amount: 5, reason: String::new() }).is_ok(), true);
}

#[test]
fn open_is_judged_before_the_amount() {
    let new = Account::default();
    for c in [Command::Deposit { amount: -1 }, Command::Reverse { target: 1 }, Command::ReceiveTransfer { transfer: 1, from: "a".into(), amount: 0 }] {
        check!(format!("{c:?}"), decide(&new, &c), Err(DomainError::NotOpen));
    }
}

#[test]
fn the_amount_is_judged_before_the_funds() {
    let a = with(&[Event::Opened]);
    check!("send -1", decide(&a, &Command::TransferOut { transfer: 1, to: "b".into(), amount: i64::MIN }), Err(DomainError::NonPositive));
}

#[test]
fn numbers_that_would_not_fit_are_refused() {
    let a = with(&[Event::Opened, dep(i64::MAX - 1)]);
    check!("deposit 2", decide(&a, &Command::Deposit { amount: 2 }), Err(DomainError::Overflow));
    check!("deposit 1", decide(&a, &Command::Deposit { amount: 1 }).is_ok(), true);
    check!("receive", decide(&a, &Command::ReceiveTransfer { transfer: 1, from: "b".into(), amount: i64::MAX }), Err(DomainError::Overflow));
    // undoing a withdrawal gives money back, which may not fit either
    let mut b = with(&[Event::Opened, dep(i64::MAX), wd(10)]);
    ask(&mut b, Command::Deposit { amount: 10 }).unwrap();
    check!("undo the withdrawal", decide(&b, &Command::Reverse { target: 3 }), Err(DomainError::Overflow));
}

#[test]
fn receiving_has_no_business_rule() {
    let a = with(&[Event::Opened]);
    check!("into an empty account", decide(&a, &Command::ReceiveTransfer { transfer: 1, from: "b".into(), amount: 3 }).is_ok(), true);
    check!("a zero amount", decide(&a, &Command::ReceiveTransfer { transfer: 1, from: "b".into(), amount: 0 }), Err(DomainError::NonPositive));
}

#[test]
fn a_reversal_cannot_be_reversed() {
    let a = with(&[Event::Opened, dep(10), Event::Reversed { target: 2, delta: -10 }]);
    check!("target 3", decide(&a, &Command::Reverse { target: 3 }), Err(DomainError::NotReversible(3)));
}

#[test]
fn transfers_cannot_be_reversed() {
    let a = with(&[
        Event::Opened,
        dep(50),
        Event::TransferredOut { transfer: 1, to: "b".into(), amount: 5 },
        Event::TransferredIn { transfer: 2, from: "b".into(), amount: 5 },
    ]);
    for target in [3, 4] {
        check!(target, decide(&a, &Command::Reverse { target }), Err(DomainError::NotReversible(target)));
    }
}

#[test]
fn already_reversed_comes_before_the_funds() {
    let a = with(&[Event::Opened, dep(10), Event::Reversed { target: 2, delta: -10 }]);
    check!("again, with no money", decide(&a, &Command::Reverse { target: 2 }), Err(DomainError::AlreadyReversed(2)));
}

#[test]
fn history_that_the_rules_would_refuse_still_applies() {
    let a = with(&[wd(50), Event::Opened, Event::Opened, Event::Reversed { target: 40, delta: 7 }]);
    check!("version, balance, open", (a.version, a.balance, a.open), (4, -43, true));
}

#[test]
fn applying_never_panics() {
    let extremes = [i64::MIN, -1, 0, 1, i64::MAX];
    for &x in &extremes {
        for &y in &extremes {
            let events = [
                dep(x),
                dep(y),
                wd(x),
                wd(y),
                Event::TransferredOut { transfer: u64::MAX, to: "a".into(), amount: x },
                Event::TransferredIn { transfer: 0, from: "a".into(), amount: y },
                Event::Reversed { target: u64::MAX, delta: x },
            ];
            let mut a = replay(&events);
            check!(format!("{x} {y}"), a.version, events.len() as u64);
            a.open = true;
            for target in 1..=7 {
                let _ = decide(&a, &Command::Reverse { target });
            }
        }
    }
    let mut full = Account::default();
    full.version = u64::MAX;
    full = evolve(full, &Event::Opened);
    check!("version stops at the top", full.version, u64::MAX);
}

#[test]
fn deciding_reads_only() {
    let a = with(&[Event::Opened, dep(10)]);
    let before = a.clone();
    let _ = decide(&a, &Command::Reverse { target: 2 });
    check!("account", a, before);
}

#[test]
fn random_commands_match_a_plain_book() {
    let mut rng = anneal_prelude::Rng::new(7201);
    for _ in 0..400 {
        let mut account = Account::default();
        let mut history: Vec<Event> = Vec::new();
        // the plain book: the balance, and for each version what can still be reversed
        let (mut open, mut balance) = (false, 0i64);
        let mut effects: Vec<(i64, bool, bool)> = Vec::new(); // (effect, reversible, reversed)
        let mut trace = Vec::new();
        for _ in 0..30 {
            let amount = rng.int(-3, 40);
            let target = rng.below(effects.len() + 2) as u64;
            let command = match rng.below(10) {
                0 => Command::Open,
                1..=3 => Command::Deposit { amount },
                4 | 5 => Command::Withdraw { amount, reason: "x".into() },
                6 => Command::TransferOut { transfer: 1, to: "b".into(), amount },
                7 => Command::ReceiveTransfer { transfer: 1, from: "b".into(), amount },
                _ => Command::Reverse { target },
            };
            trace.push(format!("{command:?}"));
            let money = |delta: i64| {
                if amount <= 0 {
                    Err(DomainError::NonPositive)
                } else if balance + delta < 0 {
                    Err(DomainError::Insufficient { have: balance, need: -delta })
                } else {
                    Ok(delta)
                }
            };
            let want: Result<(Event, i64, bool), DomainError> = match &command {
                Command::Open if open => Err(DomainError::AlreadyOpen),
                Command::Open => Ok((Event::Opened, 0, false)),
                _ if !open => Err(DomainError::NotOpen),
                Command::Deposit { .. } => money(amount).map(|d| (Event::Deposited { amount }, d, true)),
                Command::Withdraw { .. } => money(-amount).map(|d| (Event::Withdrawn { amount, reason: "x".into() }, d, true)),
                Command::TransferOut { .. } => money(-amount).map(|d| (Event::TransferredOut { transfer: 1, to: "b".into(), amount }, d, false)),
                Command::ReceiveTransfer { .. } => money(amount).map(|d| (Event::TransferredIn { transfer: 1, from: "b".into(), amount }, d, false)),
                Command::Reverse { .. } => match (target as usize).checked_sub(1).and_then(|i| effects.get(i)) {
                    Some(&(effect, true, false)) if balance - effect >= 0 => Ok((Event::Reversed { target, delta: -effect }, -effect, false)),
                    Some(&(effect, true, false)) => Err(DomainError::Insufficient { have: balance, need: effect }),
                    Some(&(_, true, true)) => Err(DomainError::AlreadyReversed(target)),
                    _ => Err(DomainError::NotReversible(target)),
                },
            };
            let got = ask(&mut account, command);
            check!(trace.join("; "), got, want.clone().map(|w| vec![w.0]));
            if let Ok((event, delta, reversible)) = want {
                if let Event::Reversed { target, .. } = event {
                    effects[target as usize - 1].2 = true;
                }
                open |= event == Event::Opened;
                balance += delta;
                effects.push((delta, reversible, false));
                history.push(event);
            }
            check!(trace.join("; "), (account.open, account.balance, account.version), (open, balance, history.len() as u64));
        }
        check!(trace.join("; "), replay(&history), account);
    }
}
