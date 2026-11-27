use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap};
use std::ops::Bound;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub player: u64,
    pub score: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadCursor;

/// A position in board order: best score first, then lowest player.
type Key = (Reverse<u64>, u64);

#[derive(Default)]
pub struct Board {
    order: BTreeSet<Key>,
    scores: HashMap<u64, u64>,
}

fn encode((Reverse(score), player): Key) -> String {
    format!("{score}.{player}")
}

fn decode(cursor: &str) -> Result<Key, BadCursor> {
    let (score, player) = cursor.split_once('.').ok_or(BadCursor)?;
    let number = |s: &str| s.parse::<u64>().map_err(|_| BadCursor);
    Ok((Reverse(number(score)?), number(player)?))
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, player: u64, score: u64) {
        self.scores.insert(player, score);
        self.order.insert((Reverse(score), player));
    }

    pub fn remove(&mut self, player: u64) -> bool {
        match self.scores.remove(&player) {
            Some(score) => self.order.remove(&(Reverse(score), player)),
            None => false,
        }
    }

    pub fn page(&self, cursor: Option<&str>, limit: usize) -> Result<Page, BadCursor> {
        let limit = limit.clamp(1, 100);
        // The cursor names a position, not an entry, so it still works when that entry is gone.
        let start = match cursor {
            Some(c) => Bound::Excluded(decode(c)?),
            None => Bound::Unbounded,
        };
        let mut keys: Vec<Key> = self
            .order
            .range((start, Bound::Unbounded))
            .take(limit + 1)
            .copied()
            .collect();
        // One row more than asked tells whether anything follows.
        let more = keys.len() > limit;
        keys.truncate(limit);
        let next = keys.last().filter(|_| more).map(|k| encode(*k));
        let entries = keys
            .into_iter()
            .map(|(Reverse(score), player)| Entry { player, score })
            .collect();
        Ok(Page { entries, next })
    }
}
