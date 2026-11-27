use solution::*;

/// Decides and applies, as the bank does.
#[allow(dead_code)]
fn ask(account: &mut Account, command: Command) -> Result<Vec<Event>, DomainError> {
    let events = decide(account, &command)?;
    for e in &events {
        *account = evolve(std::mem::take(account), e);
    }
    Ok(events)
}

#[allow(dead_code)]
fn opened(balance: i64) -> Account {
    let mut a = Account::default();
    ask(&mut a, Command::Open).unwrap();
    if balance > 0 {
        ask(&mut a, Command::Deposit { amount: balance }).unwrap();
    }
    a
}

#[allow(dead_code)]
fn withdraw(amount: i64) -> Command {
    Command::Withdraw { amount, reason: "rent".into() }
}

#[test]
fn open_deposit_withdraw() {
    let mut a = opened(100);
    check!("withdraw 30", ask(&mut a, withdraw(30)), Ok(vec![Event::Withdrawn { amount: 30, reason: "rent".into() }]));
    check!("version, balance", (a.version, a.balance, a.open), (3, 70, true));
}

#[test]
fn each_refusal() {
    check!("not open", decide(&Account::default(), &withdraw(1)), Err(DomainError::NotOpen));
    let a = opened(50);
    check!("twice", decide(&a, &Command::Open), Err(DomainError::AlreadyOpen));
    check!("zero", decide(&a, &Command::Deposit { amount: 0 }), Err(DomainError::NonPositive));
    check!("negative", decide(&a, &withdraw(-5)), Err(DomainError::NonPositive));
    check!("too much", decide(&a, &withdraw(51)), Err(DomainError::Insufficient { have: 50, need: 51 }));
}

#[test]
fn sending_and_receiving() {
    let mut a = opened(50);
    check!(
        "send 20",
        ask(&mut a, Command::TransferOut { transfer: 7, to: "bo".into(), amount: 20 }),
        Ok(vec![Event::TransferredOut { transfer: 7, to: "bo".into(), amount: 20 }])
    );
    ask(&mut a, Command::ReceiveTransfer { transfer: 9, from: "cy".into(), amount: 5 }).unwrap();
    check!("balance", a.balance, 35);
}

#[test]
fn a_mistake_is_reversed_not_erased() {
    let mut a = opened(100);
    ask(&mut a, withdraw(30)).unwrap(); // version 3
    check!("reverse it", ask(&mut a, Command::Reverse { target: 3 }), Ok(vec![Event::Reversed { target: 3, delta: 30 }]));
    check!("version, balance", (a.version, a.balance), (4, 100));
    check!("again", decide(&a, &Command::Reverse { target: 3 }), Err(DomainError::AlreadyReversed(3)));
}

#[test]
fn only_deposits_and_withdrawals_can_be_reversed() {
    let mut a = opened(100);
    ask(&mut a, Command::TransferOut { transfer: 1, to: "bo".into(), amount: 10 }).unwrap();
    ask(&mut a, Command::Reverse { target: 2 }).unwrap_err();
    for target in [0, 1, 3, 99] {
        check!(target, decide(&a, &Command::Reverse { target }), Err(DomainError::NotReversible(target)));
    }
}

#[test]
fn undoing_a_spent_deposit_is_refused() {
    let mut a = opened(100);
    ask(&mut a, withdraw(80)).unwrap();
    check!("undo the deposit", decide(&a, &Command::Reverse { target: 2 }), Err(DomainError::Insufficient { have: 20, need: 100 }));
}
