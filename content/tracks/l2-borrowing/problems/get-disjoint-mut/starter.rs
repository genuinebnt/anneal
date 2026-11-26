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
    todo!()
}

/// Moves `amount` (>= 0) from account `from` to account `to`, with the same rules as `settle`.
pub fn transfer(
    balances: &mut [i64],
    from: usize,
    to: usize,
    amount: i64,
) -> Result<(), SettleError> {
    todo!()
}
