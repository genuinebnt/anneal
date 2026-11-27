use std::collections::HashMap;
use std::sync::Arc;

/// Built once and never changed.
struct Part {
    /// the number of its first message
    first: usize,
    messages: Vec<(u32, Arc<str>)>,
    by_author: HashMap<u32, Vec<usize>>,
    chars: u64,
}

impl Part {
    fn build(first: usize, messages: Vec<(u32, Arc<str>)>) -> Part {
        let mut by_author: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut chars = 0;
        for (i, (author, text)) in messages.iter().enumerate() {
            by_author.entry(*author).or_default().push(first + i);
            chars += text.chars().count() as u64;
        }
        Part {
            first,
            messages,
            by_author,
            chars,
        }
    }

    /// A new part with the messages of `older`, then those of `newer`.
    fn join(older: &Part, newer: &Part) -> Part {
        let messages = [older.messages.as_slice(), newer.messages.as_slice()].concat();
        Part::build(older.first, messages)
    }
}

pub struct History {
    parts: Vec<Arc<Part>>,
}

impl History {
    pub fn new() -> History {
        History { parts: Vec::new() }
    }

    /// Adds a batch of `(author, text)` messages, then joins parts by the rule in the statement.
    pub fn append(&mut self, batch: &[(u32, &str)]) {
        if batch.is_empty() {
            return;
        }
        let messages = batch.iter().map(|&(a, t)| (a, Arc::from(t))).collect();
        let part = Part::build(self.len(), messages);
        self.parts.push(Arc::new(part));
        while let [.., older, newer] = self.parts.as_slice() {
            if older.messages.len() >= 2 * newer.messages.len() {
                break;
            }
            let joined = Part::join(older, newer);
            self.parts.truncate(self.parts.len() - 2);
            self.parts.push(Arc::new(joined));
        }
    }

    /// How many messages each part holds, oldest part first.
    pub fn part_sizes(&self) -> Vec<usize> {
        self.parts.iter().map(|p| p.messages.len()).collect()
    }

    pub fn len(&self) -> usize {
        self.parts.last().map_or(0, |p| p.first + p.messages.len())
    }

    /// The author and text of message `n`.
    pub fn message(&self, n: usize) -> Option<(u32, &str)> {
        let at = self
            .parts
            .partition_point(|p| p.first <= n)
            .checked_sub(1)?;
        let part = &self.parts[at];
        let (author, text) = part.messages.get(n - part.first)?;
        Some((*author, text))
    }

    /// The numbers of this author's messages, rising.
    pub fn by_author(&self, author: u32) -> Vec<usize> {
        self.parts
            .iter()
            .filter_map(|p| p.by_author.get(&author))
            .flatten()
            .copied()
            .collect()
    }

    /// The mean length in characters over all messages.
    pub fn mean_length(&self) -> Option<f64> {
        let chars: u64 = self.parts.iter().map(|p| p.chars).sum();
        (self.len() > 0).then(|| chars as f64 / self.len() as f64)
    }

    /// A history that answers as this one does now and does not see later appends.
    pub fn snapshot(&self) -> History {
        History {
            parts: self.parts.clone(),
        }
    }
}
