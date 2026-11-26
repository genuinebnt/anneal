//! RECIPE 23 (search): trigram inverted index: filter with the index, then verify.
//! LEVEL: advanced
//! Use in: search/lineage roadmap (searching task logs).   Run: cargo run --example trigram_index
//! COMMON USES: log search, code search (ripgrep-style/zoekt), Postgres pg_trgm LIKE acceleration.
//!
//! Pattern: map every 3-byte window to the documents containing it. A query's trigrams must ALL appear in
//! a matching document, so intersect their posting lists to get candidates, then confirm each candidate
//! with a real substring check (trigrams give false positives, never false negatives).
//! Read: https://swtch.com/~rsc/regexp/regexp4.html (Russ Cox) and https://github.com/quickwit-oss/tantivy
#![allow(dead_code)]
use std::collections::{HashMap, HashSet};

struct Index {
    docs: Vec<String>,
    postings: HashMap<[u8; 3], Vec<usize>>, // sorted doc ids
}

fn trigrams(s: &str) -> HashSet<[u8; 3]> {
    s.as_bytes()
        .windows(3)
        .map(|w| [w[0], w[1], w[2]])
        .collect()
}

impl Index {
    fn build(docs: &[&str]) -> Index {
        let docs: Vec<String> = docs.iter().map(|d| d.to_lowercase()).collect();
        let mut postings: HashMap<[u8; 3], Vec<usize>> = HashMap::new();
        for (id, d) in docs.iter().enumerate() {
            for t in trigrams(d) {
                postings.entry(t).or_default().push(id); // ids arrive in increasing order
            }
        }
        Index { docs, postings }
    }

    /// returns (candidates, confirmed matches)
    fn search(&self, query: &str) -> (usize, Vec<usize>) {
        let q = query.to_lowercase();
        let tris = trigrams(&q);
        let mut cands: Vec<usize> = if tris.is_empty() {
            (0..self.docs.len()).collect() // query shorter than 3 bytes: no index help
        } else {
            let mut lists: Vec<&Vec<usize>> = Vec::new();
            for t in &tris {
                match self.postings.get(t) {
                    Some(l) => lists.push(l),
                    None => return (0, vec![]),
                }
            }
            lists.sort_by_key(|l| l.len()); // start from the rarest trigram
            let mut c = lists[0].clone();
            for l in &lists[1..] {
                c.retain(|id| l.binary_search(id).is_ok());
            }
            c
        };
        let n = cands.len();
        cands.retain(|&id| self.docs[id].contains(&q));
        (n, cands)
    }
}

fn main() {
    let idx = Index::build(&[
        "transcode failed: out of memory",
        "upload finished in 300ms",
        "thumbnail generated",
        "transcode finished in 900ms",
        "validate ok",
    ]);
    for q in ["transcode", "finished in", "memory", "xyz", "ok"] {
        let (cands, hits) = idx.search(q);
        println!("{q:>12}: {cands} candidates -> hits {hits:?}");
    }
    assert_eq!(idx.search("transcode").1, vec![0, 3]);
    assert_eq!(idx.search("nothing here").1, Vec::<usize>::new());
}
