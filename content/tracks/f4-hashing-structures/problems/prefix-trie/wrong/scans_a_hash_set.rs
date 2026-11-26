use std::collections::HashSet;

#[derive(Default)]
pub struct Dictionary {
    words: HashSet<String>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, word: &str) -> bool {
        self.words.insert(word.to_string())
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(word)
    }

    pub fn remove(&mut self, word: &str) -> bool {
        self.words.remove(word)
    }

    pub fn count_prefix(&self, prefix: &str) -> usize {
        self.words.iter().filter(|w| w.starts_with(prefix)).count()
    }

    pub fn complete(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = self
            .words
            .iter()
            .filter(|w| w.starts_with(prefix))
            .cloned()
            .collect();
        out.sort();
        out.truncate(limit);
        out
    }

    pub fn longest_stored_prefix<'t>(&self, text: &'t str) -> Option<&'t str> {
        (0..=text.len())
            .rev()
            .filter(|&i| text.is_char_boundary(i))
            .map(|i| &text[..i])
            .find(|p| self.words.contains(*p))
    }

    pub fn prefix_count(&self) -> usize {
        let all: HashSet<&str> = self
            .words
            .iter()
            .flat_map(|w| w.char_indices().map(move |(i, c)| &w[..i + c.len_utf8()]))
            .collect();
        all.len()
    }
}
