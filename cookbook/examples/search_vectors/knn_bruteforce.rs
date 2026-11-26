//! RECIPE 26 (vectors): brute-force cosine kNN with a top-k heap: the baseline every ANN index is measured against.
//! LEVEL: intermediate
//! Use in: vector search roadmap.   Run: cargo run --release --example knn_bruteforce
//! COMMON USES: semantic search, recommendations, dedup: exact kNN is the correctness baseline for every ANN index.
//!
//! Pattern: score every vector, keep only the best k in a MIN-heap of size k (pop the worst when it grows
//! past k). O(n log k), O(k) memory. Recall of any approximate index (HNSW, IVF) is judged against this exact answer.
//! Read: https://ann-benchmarks.com/ and the HNSW paper https://arxiv.org/abs/1603.09320
#![allow(dead_code)]
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

#[derive(PartialEq)]
struct Score(f32);
impl Eq for Score {}
impl PartialOrd for Score {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Score {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0) // f32 is not Ord; total_cmp gives a total order
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

fn top_k(query: &[f32], data: &[Vec<f32>], k: usize) -> Vec<(usize, f32)> {
    let mut heap: BinaryHeap<Reverse<(Score, usize)>> = BinaryHeap::new();
    for (i, v) in data.iter().enumerate() {
        heap.push(Reverse((Score(cosine(query, v)), i)));
        if heap.len() > k {
            heap.pop(); // drop the current worst
        }
    }
    let mut out: Vec<(usize, f32)> = heap.into_iter().map(|Reverse((s, i))| (i, s.0)).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

fn main() {
    let mut s = 0x1234_5678_9ABC_DEF0u64;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s % 2000) as f32 / 1000.0 - 1.0
    };
    let data: Vec<Vec<f32>> = (0..5_000)
        .map(|_| (0..16).map(|_| rnd()).collect())
        .collect();
    let query = data[42].clone();
    let hits = top_k(&query, &data, 3);
    println!("{hits:?}");
    assert_eq!(hits[0].0, 42, "a vector is most similar to itself");
    assert!(hits.windows(2).all(|w| w[0].1 >= w[1].1));
}
