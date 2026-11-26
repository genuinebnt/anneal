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
        todo!()
    }

    /// The documents containing `word`, in increasing order (empty if none).
    pub fn docs(&self, word: &str) -> &[u32] {
        todo!()
    }

    /// The posting list of `word`, created empty if it's missing, for the caller to edit.
    pub fn docs_mut(&mut self, word: &str) -> &mut Vec<u32> {
        todo!()
    }

    /// Counts a lookup of `word` and returns how many times it has been looked up, this one included. Must not
    /// allocate for a word looked up before.
    pub fn hit(&mut self, word: &str) -> u64 {
        todo!()
    }

    /// Removes `doc` from every posting list and drops lists that become empty. Returns how many lists changed.
    pub fn remove_doc(&mut self, doc: u32) -> usize {
        todo!()
    }

    /// How many words have a posting list.
    pub fn words(&self) -> usize {
        self.postings.len()
    }
}
