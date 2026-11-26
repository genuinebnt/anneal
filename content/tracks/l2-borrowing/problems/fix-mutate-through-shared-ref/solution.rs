#[derive(Debug, PartialEq)]
pub struct Account {
    pub id: u32,
    pub balance: i64,
}

/// Pays `pct` percent interest (rounded toward zero) into every selected account.
pub fn pay_interest(selected: &mut [&mut Account], pct: i64) {
    for a in selected.iter_mut() {
        a.balance += a.balance * pct / 100;
    }
}

/// Points `richest` at whichever of `richest` and `candidate` has the larger balance; a tie keeps `richest`.
pub fn keep_richest<'a>(richest: &mut &'a Account, candidate: &'a Account) {
    if candidate.balance > richest.balance {
        *richest = candidate;
    }
}

/// Moves `amount` from `from` into `to`.
pub fn top_up(to: &mut Account, from: &mut Account, amount: i64) {
    from.balance -= amount;
    to.balance += amount;
}

/// Pays interest to every account with a balance of at least `min`, and returns their ids in order.
pub fn reward(accounts: &mut [Account], min: i64, pct: i64) -> Vec<u32> {
    let mut selected: Vec<&mut Account> =
        accounts.iter_mut().filter(|a| a.balance >= min).collect();
    pay_interest(&mut selected, pct);
    selected.iter().map(|a| a.id).collect()
}

/// The id of the richest account (the first one on a tie), or None if there are none.
pub fn richest_id(accounts: &[Account]) -> Option<u32> {
    let (first, rest) = accounts.split_first()?;
    let mut best = first;
    for a in rest {
        keep_richest(&mut best, a);
    }
    Some(best.id)
}
