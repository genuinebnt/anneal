use std::collections::{BTreeMap, HashMap};

const K1: f64 = 1.2;
const B: f64 = 0.75;

pub struct Index {
    /// term -> (doc -> term frequency)
    postings: HashMap<String, BTreeMap<u32, u32>>,
    /// doc -> its terms (kept so a replaced document can be removed cleanly)
    docs: BTreeMap<u32, Vec<String>>,
    total_len: usize,
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_string()).collect()
}

impl Index {
    pub fn new() -> Index {
        Index { postings: HashMap::new(), docs: BTreeMap::new(), total_len: 0 }
    }

    pub fn add(&mut self, id: u32, text: &str) {
        if let Some(old) = self.docs.remove(&id) {
            self.total_len -= old.len();
            for t in &old {
                if let Some(p) = self.postings.get_mut(t) {
                    p.remove(&id);
                    if p.is_empty() {
                        self.postings.remove(t);
                    }
                }
            }
        }
        let terms = tokenize(text);
        self.total_len += terms.len();
        for t in &terms {
            *self.postings.entry(t.clone()).or_default().entry(id).or_insert(0) += 1;
        }
        self.docs.insert(id, terms);
    }

    pub fn search(&self, query: &str, k: usize) -> Vec<(u32, f64)> {
        let n = self.docs.len() as f64;
        if self.docs.is_empty() {
            return Vec::new();
        }
        let avglen = self.total_len as f64 / n;
        let mut seen = std::collections::HashSet::new();
        let mut scores: HashMap<u32, f64> = HashMap::new();
        for term in tokenize(query) {
            if !seen.insert(term.clone()) {
                continue; // each distinct term counts once
            }
            let Some(posting) = self.postings.get(&term) else {
                continue;
            };
            let nt = posting.len() as f64;
            let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();
            for (&doc, &tf) in posting {
                let len = self.docs[&doc].len() as f64;
                let tf = tf as f64;
                *scores.entry(doc).or_insert(0.0) += idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avglen));
            }
        }
        let mut out: Vec<(u32, f64)> = scores.into_iter().collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        out.truncate(k);
        out
    }
}
