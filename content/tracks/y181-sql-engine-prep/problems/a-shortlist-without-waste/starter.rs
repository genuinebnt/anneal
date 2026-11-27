/// The outside service said no.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

/// The outside service. Every call costs money.
pub trait Judge {
    fn qualifies(&mut self, entry: u32) -> Result<bool, Refusal>;
    /// Lower is better.
    fn score(&mut self, entry: u32) -> Result<u64, Refusal>;
    fn caption(&mut self, entry: u32) -> Result<String, Refusal>;
}

/// The captions of at most `n` qualifying entries: in the order given, or best first.
pub fn shortlist(
    entries: &[u32],
    n: usize,
    ranked: bool,
    judge: &mut dyn Judge,
) -> Result<Vec<String>, Refusal> {
    let _ = (entries, n, ranked, judge);
    todo!()
}
