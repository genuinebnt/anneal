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
    let mut shown = Vec::new();
    if n == 0 {
        return Ok(shown);
    }
    if !ranked {
        for &entry in entries {
            if judge.qualifies(entry)? {
                shown.push(judge.caption(entry)?);
                if shown.len() == n {
                    break;
                }
            }
        }
        return Ok(shown);
    }
    // the best can be anywhere, so every entry has to be looked at before the first caption
    let mut scored = Vec::new();
    for &entry in entries {
        if judge.qualifies(entry)? {
            scored.push((judge.score(entry)?, entry));
        }
    }
    scored.sort_by_key(|&(score, _)| score); // stable: ties keep their order
    for &(_, entry) in &scored {
        shown.push(judge.caption(entry)?);
    }
    shown.truncate(n);
    Ok(shown)
}
