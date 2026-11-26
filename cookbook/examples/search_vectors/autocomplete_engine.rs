//! RECIPE 159 (search): type-ahead autocomplete: trie with cached top-k, decayed scores, bounded fuzzy search.
//! LEVEL: advanced
//! Use in: search boxes, IDE completion, command palettes.   Run: cargo run --release --example autocomplete_engine
#![allow(dead_code)]
//!
//! WHAT: given a typed prefix, return the k best completions, ranked by "how popular lately".
//! WHY a cached top-k per trie node: a plain trie must walk the whole subtree under the prefix and sort it
//! (cost grows with the number of matching words, bad for the 1-letter prefix "a"). If every node stores its
//! own best-k word ids, a query is O(prefix length + k): walk down, clone the list.
//! HOW (writes pay, reads are cheap):
//!   * Trie nodes live in a `Vec` arena and refer to children by index (no Box / Rc / lifetimes).
//!   * Score = sum over uses of 2^(t / half_life). A word's score at time `now` is that sum times
//!     2^(-now/half_life); the second factor is IDENTICAL for all words, so the ORDER never changes as time
//!     passes. We store the growing sum and never rescan to "apply decay": the cached top-k stays valid.
//!     (Floats overflow near t/half_life ~ 1000; a real system rebases periodically.)
//!   * `record(word)` only ever INCREASES one word's weight, so each node on its path needs just one fix:
//!     insert/re-sort that word into its small list and truncate to k. Everyone else is unchanged.
//!   * Ordering is total and deterministic: higher weight first, ties broken by the word itself.
//!   * Fuzzy: walk the trie carrying one Levenshtein DP row (dp_edit_distance) per node. A node whose row's last cell is
//!     <= budget means "the typed text is within budget of this path", so its whole subtree matches and we
//!     take that node's cached top-k instead of descending. A node whose row minimum exceeds the budget can
//!     never recover (row minima never decrease along a path), so the whole subtree is pruned.
//!   * A brute-force scan over all words is the oracle for both modes.
//! COMMON USES: search-box suggestions, IDE/code completion, shell and command-palette ranking (frecency),
//!   "did you mean" prefix matching, phone keyboards, address entry. Per-node top-k caches are the standard
//!   trick for fast prefix suggestion; frequency with exponential decay is the usual "frecency" idea.
//! Read: https://en.wikipedia.org/wiki/Trie and https://en.wikipedia.org/wiki/Levenshtein_distance
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Instant;

// ---------- injectable clock: tests control time ----------
trait Clock {
    fn now(&self) -> u64;
}

struct FakeClock(Cell<u64>);

impl FakeClock {
    fn advance(&self, dt: u64) {
        self.0.set(self.0.get() + dt);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> u64 {
        self.0.get()
    }
}

// ---------- engine ----------
struct Entry {
    text: String,
    weight: f64, // sum of 2^(t/half_life); see module notes
}

struct Node {
    kids: Vec<(u8, usize)>, // small alphabets: a linear scan beats a map
    word: Option<usize>,
    top: Vec<usize>, // best word ids in this subtree, best first, len <= k
}

/// total order: best first (higher weight, then smaller text)
fn better(words: &[Entry], a: usize, b: usize) -> Ordering {
    words[b]
        .weight
        .total_cmp(&words[a].weight)
        .then_with(|| words[a].text.cmp(&words[b].text))
}

struct Engine<C: Clock> {
    clock: C,
    half_life: f64,
    k: usize,
    nodes: Vec<Node>,
    words: Vec<Entry>,
    index: HashMap<String, usize>,
}

impl<C: Clock> Engine<C> {
    fn new(clock: C, half_life: f64, k: usize) -> Self {
        let root = Node {
            kids: vec![],
            word: None,
            top: vec![],
        };
        Engine {
            clock,
            half_life,
            k,
            nodes: vec![root],
            words: vec![],
            index: HashMap::new(),
        }
    }

    fn child(&self, node: usize, b: u8) -> Option<usize> {
        self.nodes[node]
            .kids
            .iter()
            .find(|(c, _)| *c == b)
            .map(|(_, n)| *n)
    }

    fn insert_path(&mut self, text: &str) -> Vec<usize> {
        let mut path = vec![0];
        let mut cur = 0;
        for &b in text.as_bytes() {
            cur = match self.child(cur, b) {
                Some(n) => n,
                None => {
                    self.nodes.push(Node {
                        kids: vec![],
                        word: None,
                        top: vec![],
                    });
                    let n = self.nodes.len() - 1;
                    self.nodes[cur].kids.push((b, n));
                    n
                }
            };
            path.push(cur);
        }
        path
    }

    /// one use of `text` happened "now"
    fn record(&mut self, text: &str) {
        let gain = (self.clock.now() as f64 / self.half_life).exp2();
        let path = self.insert_path(text);
        let id = *self.index.entry(text.to_string()).or_insert_with(|| {
            self.words.push(Entry {
                text: text.to_string(),
                weight: 0.0,
            });
            self.words.len() - 1
        });
        self.nodes[*path.last().unwrap()].word = Some(id);
        self.words[id].weight += gain;
        for node in path {
            // O(k log k) per node on the path: tiny k, and far simpler than a position-tracked heap
            let mut top = std::mem::take(&mut self.nodes[node].top);
            if !top.contains(&id) {
                top.push(id);
            }
            top.sort_by(|&a, &b| better(&self.words, a, b));
            top.truncate(self.k);
            self.nodes[node].top = top;
        }
    }

    fn texts(&self, ids: &[usize]) -> Vec<&str> {
        ids.iter().map(|&i| self.words[i].text.as_str()).collect()
    }

    /// O(len(prefix) + k)
    fn complete(&self, prefix: &str) -> Vec<usize> {
        let mut cur = 0;
        for &b in prefix.as_bytes() {
            match self.child(cur, b) {
                Some(n) => cur = n,
                None => return vec![],
            }
        }
        self.nodes[cur].top.clone()
    }

    /// words having a prefix within `budget` edits of `query`
    fn complete_fuzzy(&self, query: &str, budget: usize) -> Vec<usize> {
        let q = query.as_bytes();
        let row: Vec<usize> = (0..=q.len()).collect();
        let mut cands = vec![];
        self.walk(0, &row, q, budget, &mut cands);
        cands.sort_by(|&a, &b| better(&self.words, a, b));
        cands.truncate(self.k);
        cands
    }

    fn walk(&self, node: usize, row: &[usize], q: &[u8], budget: usize, out: &mut Vec<usize>) {
        if row[q.len()] <= budget {
            // whole subtree matches; its cached top-k is all we can need. Paths are disjoint, so no duplicates.
            out.extend(&self.nodes[node].top);
            return;
        }
        for &(b, child) in &self.nodes[node].kids {
            let next = next_row(row, q, b);
            if next.iter().min().is_some_and(|&m| m <= budget) {
                self.walk(child, &next, q, budget, out);
            }
        }
    }

    // ---------- oracles: scan everything ----------
    fn brute_complete(&self, prefix: &str) -> Vec<usize> {
        let mut ids: Vec<usize> = (0..self.words.len())
            .filter(|&i| self.words[i].text.starts_with(prefix))
            .collect();
        ids.sort_by(|&a, &b| better(&self.words, a, b));
        ids.truncate(self.k);
        ids
    }

    fn brute_fuzzy(&self, query: &str, budget: usize) -> Vec<usize> {
        let q = query.as_bytes();
        let mut ids: Vec<usize> = (0..self.words.len())
            .filter(|&i| {
                let mut row: Vec<usize> = (0..=q.len()).collect();
                let mut best = row[q.len()];
                for &b in self.words[i].text.as_bytes() {
                    row = next_row(&row, q, b);
                    best = best.min(row[q.len()]);
                }
                best <= budget
            })
            .collect();
        ids.sort_by(|&a, &b| better(&self.words, a, b));
        ids.truncate(self.k);
        ids
    }
}

/// one Levenshtein DP step: row for (path + byte b) from the row for path
fn next_row(row: &[usize], q: &[u8], b: u8) -> Vec<usize> {
    let mut next = Vec::with_capacity(row.len());
    next.push(row[0] + 1);
    for j in 1..row.len() {
        let sub = row[j - 1] + usize::from(q[j - 1] != b);
        next.push(sub.min(row[j] + 1).min(next[j - 1] + 1));
    }
    next
}

// ---------- seeded randomness ----------
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn random_word(rng: &mut Rng) -> String {
    // small alphabet on purpose: many shared prefixes and many fuzzy hits make the test meaningful
    (0..3 + rng.below(7))
        .map(|_| (b'a' + rng.below(8) as u8) as char)
        .collect()
}

fn typo(rng: &mut Rng, s: &str, n: usize) -> String {
    let mut b: Vec<u8> = s.bytes().collect();
    for _ in 0..n {
        let pos = rng.below(b.len().max(1));
        match rng.below(3) {
            0 if !b.is_empty() => b[pos] = b'a' + rng.below(8) as u8, // substitute
            1 if !b.is_empty() => {
                b.remove(pos); // delete
            }
            _ => b.insert(pos.min(b.len()), b'a' + rng.below(8) as u8), // insert
        }
    }
    String::from_utf8(b).unwrap()
}

fn demo_decay() {
    let mut e = Engine::new(FakeClock(Cell::new(0)), 100.0, 3);
    for _ in 0..5 {
        e.record("alpha");
    }
    e.record("alpine");
    e.record("alpine"); // tie-ish scores resolved below
    println!("t=0   al -> {:?}", e.texts(&e.complete("al")));
    assert_eq!(e.texts(&e.complete("al")), ["alpha", "alpine"]);
    e.clock.advance(500); // five half-lives later, fresh activity outweighs old popularity
    e.record("alps");
    e.record("alps");
    println!("t=500 al -> {:?}", e.texts(&e.complete("al")));
    assert_eq!(e.texts(&e.complete("al"))[0], "alps");
    // tie-break: equal weights sort alphabetically
    let mut t = Engine::new(FakeClock(Cell::new(7)), 100.0, 3);
    for w in ["pear", "peach", "pea"] {
        t.record(w);
    }
    assert_eq!(t.texts(&t.complete("pe")), ["pea", "peach", "pear"]);
    println!(
        "fuzzy 'peqch' (1 typo) -> {:?}",
        t.texts(&t.complete_fuzzy("peqch", 1))
    );
    assert_eq!(t.complete_fuzzy("peqch", 1), t.brute_fuzzy("peqch", 1));
}

fn main() {
    demo_decay();

    let mut rng = Rng::new(159);
    let mut engine = Engine::new(FakeClock(Cell::new(0)), 400.0, 5);
    let mut vocab: Vec<String> = (0..5000).map(|_| random_word(&mut rng)).collect();
    vocab.sort();
    vocab.dedup();
    for w in &vocab {
        engine.record(w); // everyone known once
    }
    for _ in 0..30_000 {
        engine.clock.advance(rng.below(3) as u64);
        let skew = rng.below(vocab.len()) * rng.below(vocab.len()) / vocab.len(); // few hot words
        engine.record(&vocab[skew]);
    }
    println!(
        "{} words, {} trie nodes, clock at {}",
        engine.words.len(),
        engine.nodes.len(),
        engine.clock.now()
    );

    // queries: prefixes of real words, then typo'd prefixes
    let prefixes: Vec<String> = (0..2000)
        .map(|_| {
            let w = &vocab[rng.below(vocab.len())];
            w[..1 + rng.below(w.len().min(4))].to_string()
        })
        .collect();
    let typos: Vec<(String, usize)> = (0..300)
        .map(|i| (typo(&mut rng, &prefixes[i], 1 + i % 2), 1 + i % 2))
        .collect();

    let t = Instant::now();
    let fast: Vec<Vec<usize>> = prefixes.iter().map(|p| engine.complete(p)).collect();
    let exact_time = t.elapsed();
    let t = Instant::now();
    let slow: Vec<Vec<usize>> = prefixes.iter().map(|p| engine.brute_complete(p)).collect();
    let brute_time = t.elapsed();
    assert_eq!(fast, slow, "trie top-k must equal brute-force scan");

    let t = Instant::now();
    let fz: Vec<Vec<usize>> = typos
        .iter()
        .map(|(q, d)| engine.complete_fuzzy(q, *d))
        .collect();
    let fuzzy_time = t.elapsed();
    let t = Instant::now();
    let fz_slow: Vec<Vec<usize>> = typos
        .iter()
        .map(|(q, d)| engine.brute_fuzzy(q, *d))
        .collect();
    let fuzzy_brute_time = t.elapsed();
    assert_eq!(
        fz, fz_slow,
        "bounded-Levenshtein trie walk must equal brute-force scan"
    );

    let non_empty = fz.iter().filter(|r| !r.is_empty()).count();
    assert!(non_empty > 250, "fuzzy test should mostly produce matches");
    println!(
        "exact  : {:>9.2?} for {} queries (trie)   vs {:>9.2?} brute force",
        exact_time,
        prefixes.len(),
        brute_time
    );
    println!(
        "fuzzy  : {:>9.2?} for {} queries (trie)   vs {:>9.2?} brute force",
        fuzzy_time,
        typos.len(),
        fuzzy_brute_time
    );
    let (q, d) = &typos[0];
    println!("example: {q:?} (budget {d}) -> {:?}", engine.texts(&fz[0]));
    println!("all differential checks passed");
}
