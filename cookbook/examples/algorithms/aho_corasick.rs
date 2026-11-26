//! RECIPE 139 (strings): Aho-Corasick multi-pattern matching: trie + failure links + dictionary links.
//! LEVEL: advanced
//! Use in: scanning one text for thousands of patterns at once (signatures, banned words, log markers).   Run: cargo run --release --example aho_corasick
#![allow(dead_code)]
//!
//! Problem: find every occurrence (overlaps included) of ANY of k patterns in a text. Running KMP per pattern costs
//! O(k * n). Aho-Corasick merges the patterns into one automaton and scans the text once: O(n + total pattern length
//! + number of matches), independent of k.
//!
//! Construction:
//!   1. Trie in an arena (`Vec<Node>`, children addressed by index; no Rc/Box pointer soup, cache friendly).
//!   2. Failure link of node v (string s) = the node for the LONGEST PROPER SUFFIX of s that is also a trie prefix.
//!      BFS order guarantees a node's fail target is shallower, hence already finished. fail(child of v by c) is found
//!      by walking fail(v), fail(fail(v)), ... until some node has a c-child: exactly KMP's fallback, on a trie.
//!   3. Dictionary (output) link of v = nearest node on its fail chain that ends a pattern. Without it, reporting
//!      matches would have to walk the whole fail chain, including nodes that match nothing.
//! Search: keep a current node; on each byte follow fail links until a c-child exists, then step. Each step down
//! deepens the state by 1 and each fail hop makes it shallower, so total hops <= text length (amortised linear).
//! At every position, report the node's own patterns and then everything reachable by dictionary links.
//! COMMON USES: network intrusion detection and antivirus signature scanning (many byte patterns, one pass), profanity
//! and keyword filters, log scanners that tag lines against hundreds of known error strings, bioinformatics motif
//! search. The Rust crate by Andrew Gallant (BurntSushi) is the production-grade version: DFA/NFA variants,
//! leftmost-first/longest semantics, SIMD prefilters, and it powers multi-literal optimisations in the regex crate.
//! Read: https://github.com/BurntSushi/aho-corasick and https://en.wikipedia.org/wiki/Aho%E2%80%93Corasick_algorithm
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn string(&mut self, min: u64, max: u64, alphabet: u8) -> Vec<u8> {
        let len = min + self.below(max - min);
        (0..len)
            .map(|_| b'a' + self.below(alphabet as u64) as u8)
            .collect()
    }
}

#[derive(Default)]
struct Node {
    next: HashMap<u8, usize>,
    fail: usize,
    dict: Option<usize>, // nearest proper-suffix node that ends a pattern
    ends: Vec<usize>,    // ids of patterns ending exactly here (duplicates allowed)
}

struct Automaton {
    nodes: Vec<Node>,
    lens: Vec<usize>, // pattern id -> length, to turn an end position into a start position
}

/// (start offset, pattern id)
type Match = (usize, usize);

impl Automaton {
    fn new(patterns: &[&[u8]]) -> Automaton {
        let mut nodes = vec![Node::default()]; // node 0 = root = empty string
        for (id, pat) in patterns.iter().enumerate() {
            let mut cur = 0;
            for &b in *pat {
                cur = match nodes[cur].next.get(&b) {
                    Some(&n) => n,
                    None => {
                        nodes.push(Node::default());
                        let n = nodes.len() - 1;
                        nodes[cur].next.insert(b, n);
                        n
                    }
                };
            }
            nodes[cur].ends.push(id);
        }
        // BFS by depth: every node's fail target is strictly shallower, so it is already complete when needed
        let mut queue: VecDeque<usize> = nodes[0].next.values().copied().collect(); // depth 1: fail = root
        while let Some(v) = queue.pop_front() {
            let kids: Vec<(u8, usize)> = nodes[v].next.iter().map(|(&b, &n)| (b, n)).collect();
            for (b, child) in kids {
                let mut f = nodes[v].fail;
                let target = loop {
                    if let Some(&t) = nodes[f].next.get(&b) {
                        break t;
                    }
                    if f == 0 {
                        break 0;
                    }
                    f = nodes[f].fail;
                };
                nodes[child].fail = target;
                nodes[child].dict = if nodes[target].ends.is_empty() {
                    nodes[target].dict
                } else {
                    Some(target)
                };
                queue.push_back(child);
            }
        }
        Automaton {
            nodes,
            lens: patterns.iter().map(|p| p.len()).collect(),
        }
    }

    /// Returns all matches plus the number of fail-link hops taken (to show the linear bound).
    fn search(&self, text: &[u8]) -> (Vec<Match>, usize) {
        let (mut state, mut hops, mut out) = (0, 0, vec![]);
        for (i, &b) in text.iter().enumerate() {
            while state != 0 && !self.nodes[state].next.contains_key(&b) {
                state = self.nodes[state].fail;
                hops += 1;
            }
            state = self.nodes[state].next.get(&b).copied().unwrap_or(0);
            // own patterns first, then the dictionary chain: only nodes that really end a pattern are visited
            let mut v = if self.nodes[state].ends.is_empty() {
                self.nodes[state].dict
            } else {
                Some(state)
            };
            while let Some(n) = v {
                for &id in &self.nodes[n].ends {
                    out.push((i + 1 - self.lens[id], id));
                }
                v = self.nodes[n].dict;
            }
        }
        (out, hops)
    }
}

/// Oracle: scan the text once per pattern.
fn naive(patterns: &[&[u8]], text: &[u8]) -> Vec<Match> {
    let mut out = vec![];
    for (id, p) in patterns.iter().enumerate() {
        if p.len() <= text.len() {
            out.extend(
                (0..=text.len() - p.len())
                    .filter(|&i| &text[i..i + p.len()] == *p)
                    .map(|i| (i, id)),
            );
        }
    }
    out
}

fn as_set(v: &[Match]) -> BTreeSet<Match> {
    v.iter().copied().collect()
}

fn main() {
    // the classic example: "she" contains "he", and "hers" overlaps both
    let pats: [&[u8]; 4] = [b"he", b"she", b"his", b"hers"];
    let ac = Automaton::new(&pats);
    let (found, _) = ac.search(b"ushers");
    println!("'ushers' matches (start, pattern): {:?}", {
        let mut f = found.clone();
        f.sort();
        f
    });
    assert_eq!(as_set(&found), as_set(&[(2, 0), (1, 1), (2, 3)])); // he@2, she@1, hers@2
    assert_eq!(found.len(), 3);

    // differential tests: small alphabets force heavy overlap, nesting and duplicates
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for round in 0..2000 {
        let alphabet = 1 + (round % 3) as u8;
        let k = 1 + rng.below(8) as usize;
        let owned: Vec<Vec<u8>> = (0..k).map(|_| rng.string(1, 6, alphabet)).collect();
        let pats: Vec<&[u8]> = owned.iter().map(|p| p.as_slice()).collect();
        let text = rng.string(0, 80, alphabet);
        let (got, hops) = Automaton::new(&pats).search(&text);
        let want = naive(&pats, &text);
        assert_eq!(got.len(), want.len(), "no duplicate or missing reports");
        assert_eq!(as_set(&got), as_set(&want), "pats {owned:?} text {text:?}");
        assert!(
            hops <= text.len(),
            "amortised bound: fail hops never exceed bytes consumed"
        );
    }

    // scale: 1000 patterns, 200k-byte text
    let owned: Vec<Vec<u8>> = (0..1000).map(|_| rng.string(6, 11, 4)).collect();
    let pats: Vec<&[u8]> = owned.iter().map(|p| p.as_slice()).collect();
    let text = rng.string(200_000, 200_001, 4);
    let t = Instant::now();
    let ac = Automaton::new(&pats);
    let built = t.elapsed();
    let t = Instant::now();
    let (got, hops) = ac.search(&text);
    let scan = t.elapsed();
    let t = Instant::now();
    let want = naive(&pats, &text);
    let slow = t.elapsed();
    assert_eq!(as_set(&got), as_set(&want));
    assert!(hops <= text.len());
    println!(
        "1000 patterns, {} bytes, {} matches: build {built:?}, aho-corasick scan {scan:?} ({hops} fail hops), naive {slow:?}",
        text.len(),
        got.len()
    );

    // doubling the text only doubles the work (hops stay <= n), while naive cost is k * n either way
    let text2: Vec<u8> = text.iter().chain(&text).copied().collect();
    let (_, hops2) = ac.search(&text2);
    assert!(hops2 <= text2.len());
    println!(
        "hops: {hops} for n={}, {hops2} for n={}: linear in the text, not in the pattern count",
        text.len(),
        text2.len()
    );
    println!("all aho-corasick checks passed");
}
