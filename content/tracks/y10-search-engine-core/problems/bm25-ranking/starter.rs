use std::collections::{BTreeMap, HashMap};

/// A ranked full-text index using Okapi BM25 (`k1 = 1.2`, `b = 0.75`).
pub struct Index {
    // your fields
}

/// Splits text into lowercase terms: maximal runs of alphanumeric chars (`char::is_alphanumeric`), lowercased with `to_lowercase`.
pub fn tokenize(text: &str) -> Vec<String> {
    todo!()
}

impl Index {
    pub fn new() -> Index {
        todo!()
    }

    /// Adds a document; an existing id is **replaced**.
    pub fn add(&mut self, id: u32, text: &str) {
        todo!()
    }

    /// The best `k` documents for `query` as `(id, score)`, highest score first, ties by smaller id first. Only documents that
    /// contain at least one query term are returned. Each *distinct* query term counts once.
    ///
    /// score(d) = sum over distinct query terms t of  idf(t) * tf * (k1 + 1) / (tf + k1 * (1 - b + b * len(d) / avglen))
    /// idf(t)   = ln(1 + (N - n_t + 0.5) / (n_t + 0.5))
    /// where N is the number of documents, n_t the number containing t, tf the count of t in d, len(d) the number of terms in d,
    /// and avglen the mean length over all documents.
    pub fn search(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        todo!()
    }
}
