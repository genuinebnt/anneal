use std::collections::HashMap;

pub struct Index {
    postings: HashMap<String, Vec<u32>>,
    hits: HashMap<String, u64>,
}

impl Index {
    pub fn new() -> Self {
        Index {
            postings: HashMap::new(),
            hits: HashMap::new(),
        }
    }

    /// Records that document `doc` contains each whitespace-separated word of `text`. A document is listed once
    /// per word, and documents are added in increasing id order. Must not allocate a key for a word the index
    /// already has.
    pub fn add(&mut self, doc: u32, text: &str) {
        for w in text.split_whitespace() {
            match self.postings.get_mut(w) {
                Some(list) => {
                    if list.last() != Some(&doc) {
                        list.push(doc);
                    }
                }
                None => {
                    self.postings.insert(w.to_string(), vec![doc]);
                }
            }
        }
    }

    /// The documents containing `word`, in increasing order (empty if none).
    pub fn docs(&self, word: &str) -> &[u32] {
        self.postings.get(word).map_or(&[], Vec::as_slice)
    }

    /// The posting list of `word`, created empty if it's missing, for the caller to edit.
    pub fn docs_mut(&mut self, word: &str) -> &mut Vec<u32> {
        self.postings.entry(word.to_string()).or_default()
    }

    /// Counts a lookup of `word` and returns how many times it has been looked up, this one included. Must not
    /// allocate for a word looked up before.
    pub fn hit(&mut self, word: &str) -> u64 {
        if let Some(n) = self.hits.get_mut(word) {
            *n += 1;
            return *n;
        }
        self.hits.insert(word.to_string(), 1);
        1
    }

    /// Removes `doc` from every posting list and drops lists that become empty. Returns how many lists changed.
    pub fn remove_doc(&mut self, doc: u32) -> usize {
        let mut changed = 0;
        self.postings.retain(|_, list| {
            if let Ok(i) = list.binary_search(&doc) {
                list.remove(i);
                changed += 1;
            }
            !list.is_empty()
        });
        changed
    }

    /// How many words have a posting list.
    pub fn words(&self) -> usize {
        self.postings.len()
    }
}
