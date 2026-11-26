use std::slice::GetDisjointMutError;

#[derive(Debug, PartialEq)]
pub enum SettleError {
    /// An index appears twice.
    SameAccount,
    /// An index is out of bounds.
    NoSuchAccount,
    /// The deltas don't add up to zero.
    Unbalanced,
    /// Some balance would go negative.
    Insufficient,
}

/// Applies `deltas[k]` to account `idx[k]` for every k, all or nothing. On error nothing changes. Index errors
/// are reported as std's `get_disjoint_mut` finds them, then `Unbalanced`, then `Insufficient`.
pub fn settle<const N: usize>(
    balances: &mut [i64],
    idx: [usize; N],
    deltas: [i64; N],
) -> Result<(), SettleError> {
    let accounts = balances.get_disjoint_mut(idx).map_err(|e| match e {
        GetDisjointMutError::IndexOutOfBounds => SettleError::NoSuchAccount,
        GetDisjointMutError::OverlappingIndices => SettleError::SameAccount,
    })?;
    if deltas.iter().sum::<i64>() != 0 {
        return Err(SettleError::Unbalanced);
    }
    if accounts.iter().zip(&deltas).any(|(a, d)| **a + d < 0) {
        return Err(SettleError::Insufficient);
    }
    for (a, d) in accounts.into_iter().zip(deltas) {
        *a += d;
    }
    Ok(())
}

/// Moves `amount` (>= 0) from account `from` to account `to`, with the same rules as `settle`.
pub fn transfer(
    balances: &mut [i64],
    from: usize,
    to: usize,
    amount: i64,
) -> Result<(), SettleError> {
    settle(balances, [from, to], [-amount, amount])
}
