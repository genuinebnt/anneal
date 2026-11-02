use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A substring-search index over documents, built on trigrams (every window of 3 consecutive `char`s), as in Google Code Search.
pub struct Index {
    docs: BTreeMap<u32, String>,
    grams: HashMap<[char; 3], BTreeSet<u32>>,
}

impl Index {
    pub fn new() -> Index {
        todo!()
    }

    /// Adds a document. Adding an id that already exists **replaces** its text (its old trigrams must stop matching).
    pub fn add(&mut self, id: u32, text: &str) {
        todo!()
    }

    /// Removes a document; true if it existed.
    pub fn remove(&mut self, id: u32) -> bool {
        todo!()
    }

    /// The ids of documents that contain **every trigram** of `query`, sorted: a superset of the real matches, found without
    /// reading any document. `None` if the query has fewer than 3 chars (the index cannot help).
    pub fn candidates(&self, query: &str) -> Option<Vec<u32>> {
        todo!()
    }

    /// The ids, sorted, of documents whose text contains `query` as a substring (case-sensitive). The empty query matches every
    /// document. Use `candidates` to avoid scanning, and then confirm each candidate against the text.
    pub fn search(&self, query: &str) -> Vec<u32> {
        todo!()
    }
}
