//! RECIPE 630 (interview): trie interview questions in Rust: three ways to build a trie, wildcards, prefix tricks, autocomplete, radix tries, Unicode.
//! LEVEL: intermediate
//! Use in: coding interviews (LeetCode 208, 211, 14, 648, 720, 677, 1268, 139, 642) and any code that does prefix search, autocomplete or routing.   Run: cargo run --release --example tries_core_questions
//! COMMON USES: autocomplete/typeahead, spell checkers, T9 keypads, dictionary lookups, IP longest-prefix routing, search suggestions, word games.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233463531/trie-interview-patterns-autocomplete/ , https://www.techinterview.org/post/3233474786/word-search-ii-trie-pattern-recognition/ ,
//! https://rust-unofficial.github.io/too-many-lists/
//! See also: examples/data_structures/trie_radix.rs, examples/search_vectors/autocomplete_engine.rs, examples/interview/tries_advanced_questions.rs
//!
//! QUESTION BANK
//! Q1. Implement a Trie with insert, search, starts_with, count of words with a prefix, and delete (array children). [Medium] [asked: very common]
//! Q2. Implement the same Trie with HashMap children so it handles any char, not just a-z. [Medium] [asked: very common]
//! Q3. Implement the same Trie as an arena: nodes in a Vec, children are u32 indices. Why would you? [Medium] [asked: common]
//! Q4. Add and search words where '.' matches any single letter (LeetCode 211). [Medium] [asked: very common]
//! Q5. Longest common prefix of a set of words (LeetCode 14): is a trie the right tool? [Easy] [asked: very common]
//! Q6. Replace every word in a sentence with the shortest dictionary root that prefixes it (LeetCode 648). [Medium] [asked: common]
//! Q7. Longest word in a dictionary that can be built one character at a time from other words (LeetCode 720). [Medium] [asked: common]
//! Q8. Map Sum Pairs: insert(key, val) with overwrite, sum(prefix) (LeetCode 677). [Medium] [asked: common]
//! Q9. Count the distinct substrings of a string with a suffix trie; why are a suffix array and a suffix automaton better? [Hard] [asked: occasional]
//! Q10. Autocomplete: top-k most frequent words for a prefix, with a per-node top-k cache versus DFS plus a heap (LeetCode 642 idea). [Hard] [asked: very common]
//! Q11. Search suggestions system: after each typed letter, the 3 lexicographically smallest products with that prefix (LeetCode 1268): sorted array vs trie. [Medium] [asked: very common]
//! Q12. Word break: can a string be split into dictionary words? Use a trie to walk (LeetCode 139). [Medium] [asked: very common]
//! Q13. T9 phone keypad: all dictionary words that a digit sequence can spell. [Medium] [asked: occasional]
//! Q14. Spell checker: all dictionary words within edit distance k of a typo, pruning the trie with a Levenshtein row. [Hard] [asked: occasional]
//! Q15. Compress a trie into a radix (Patricia) tree: split edges on insert, and prove it uses at most 2n nodes. [Hard] [asked: common]
//! Q16. Trie vs hash set vs sorted Vec: which wins for prefix queries, and what does each cost in memory? [Medium] [asked: common]
//! Q17. Unicode: should the trie walk bytes or chars? What breaks with normalization and case folding? [Medium] [asked: common]
//! Q18. Longest-prefix match for IPv4-style routing with a binary trie. [Medium] [asked: common]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};
use std::mem::size_of;

// ---------------------------------------------------------------------------------------------------------------------
// Shared plumbing: seeded xorshift, random words, a common prefix-set trait and a generic oracle checker.
// ---------------------------------------------------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}
/// Lowercase word of length 1..=max_len over the first `alpha` letters (small alphabets force shared prefixes).
fn gen_word(r: &mut Rng, max_len: usize, alpha: usize) -> String {
    let n = 1 + r.range(max_len);
    (0..n)
        .map(|_| (b'a' + r.range(alpha) as u8) as char)
        .collect()
}
fn rand_prefix(r: &mut Rng, w: &str) -> String {
    let n = w.chars().count();
    w.chars().take(r.range(n + 1)).collect()
}

trait PrefixSet {
    fn ins(&mut self, w: &str) -> bool;
    fn has(&self, w: &str) -> bool;
    fn pre(&self, p: &str) -> bool;
    fn cnt(&self, p: &str) -> usize;
}

/// Differential test: 2000 random sets, every operation compared with a `BTreeSet<String>` oracle.
fn check_prefix_set<T: PrefixSet + Default>(seed: u64, mk: &dyn Fn(&mut Rng) -> String) {
    let mut r = Rng(seed);
    for _ in 0..2000 {
        let (mut t, mut o) = (T::default(), BTreeSet::<String>::new());
        for _ in 0..12 {
            let w = mk(&mut r);
            assert_eq!(t.ins(&w), o.insert(w.clone()), "insert {w}");
        }
        let mut queries: Vec<String> = (0..10).map(|_| mk(&mut r)).collect();
        for _ in 0..6 {
            let w = o.iter().nth(r.range(o.len())).unwrap().clone();
            queries.push(w); // known members
        }
        for q in queries {
            let p = rand_prefix(&mut r, &q);
            assert_eq!(t.has(&q), o.contains(&q), "has {q}");
            assert_eq!(t.pre(&p), o.iter().any(|w| w.starts_with(&p)), "pre {p}");
            assert_eq!(
                t.cnt(&p),
                o.iter().filter(|w| w.starts_with(p.as_str())).count(),
                "cnt {p}"
            );
            assert_eq!(
                t.cnt(&q),
                o.iter().filter(|w| w.starts_with(q.as_str())).count(),
                "cnt {q}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Implement a Trie (array children) with insert / search / starts_with / count_prefix / delete.
// Approach: one node per character; `end` marks a word; `pass` counts words through the node, so count_prefix is O(|p|) and delete can
// prune a branch the moment its pass count hits zero. Time O(|word|) per op; space O(total characters * 26 pointers) worst case.
// Rust wrinkle: `[None; 26]` does NOT compile for `Option<Box<Node>>` (the repeat expression needs `Copy`, or a `const` item). Three fixes:
// `std::array::from_fn(|_| None)`, `[const { None }; 26]`, or simply `#[derive(Default)]` (arrays up to 32 implement Default).
// Walking and inserting: `cur = cur.kids[i].get_or_insert_with(Default::default)` reassigns a `&mut` cursor each step, which NLL accepts
// because the old borrow is dead when the new one starts. Reading uses `as_deref()` and `?` to bail out on a missing edge.
// Interviewer is testing: node design, end-of-word flag vs "is a prefix", delete that does not break sibling words.
// Mistakes: deleting by clearing `end` only (leaks nodes); unwrapping a missing child; mixing up starts_with("") (true) and search("") (false).
// Follow-ups: iterative Drop for deep tries (recursive Drop can overflow on a million-char word); case-insensitive; thread-safe version.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct ANode {
    kids: [Option<Box<ANode>>; 26], // derive(Default) works for arrays of length <= 32; `[None; 26]` would not compile
    end: bool,
    pass: usize, // number of stored words that pass through (or end at) this node
}
#[derive(Default)]
struct ArrTrie {
    root: ANode,
}
fn ix(b: u8) -> usize {
    (b - b'a') as usize
}
impl ArrTrie {
    fn find(&self, w: &str) -> Option<&ANode> {
        let mut cur = &self.root;
        for b in w.bytes() {
            cur = cur.kids[ix(b)].as_deref()?;
        }
        Some(cur)
    }
    fn insert(&mut self, w: &str) -> bool {
        if self.search(w) {
            return false;
        }
        let mut cur = &mut self.root;
        cur.pass += 1;
        for b in w.bytes() {
            cur = cur.kids[ix(b)].get_or_insert_with(Default::default);
            cur.pass += 1;
        }
        cur.end = true;
        true
    }
    fn search(&self, w: &str) -> bool {
        self.find(w).is_some_and(|n| n.end)
    }
    fn starts_with(&self, p: &str) -> bool {
        self.find(p).is_some()
    }
    fn count_prefix(&self, p: &str) -> usize {
        self.find(p).map_or(0, |n| n.pass)
    }
    fn delete(&mut self, w: &str) -> bool {
        fn del(node: &mut ANode, w: &[u8]) {
            node.pass -= 1;
            if let Some((&b, rest)) = w.split_first() {
                let c = node.kids[ix(b)].as_mut().unwrap();
                del(c, rest);
                if c.pass == 0 {
                    node.kids[ix(b)] = None; // prune the exhausted branch
                }
            } else {
                node.end = false;
            }
        }
        if !self.search(w) {
            return false;
        }
        del(&mut self.root, w.as_bytes());
        true
    }
    fn nodes(&self) -> usize {
        fn go(n: &ANode) -> usize {
            1 + n.kids.iter().flatten().map(|k| go(k)).sum::<usize>()
        }
        go(&self.root)
    }
}
impl PrefixSet for ArrTrie {
    fn ins(&mut self, w: &str) -> bool {
        self.insert(w)
    }
    fn has(&self, w: &str) -> bool {
        self.search(w)
    }
    fn pre(&self, p: &str) -> bool {
        self.starts_with(p)
    }
    fn cnt(&self, p: &str) -> usize {
        self.count_prefix(p)
    }
}
fn q1() {
    let mut t = ArrTrie::default();
    assert!(t.insert("apple") && !t.insert("apple"));
    assert!(t.search("apple") && !t.search("app") && t.starts_with("app"));
    assert!(t.insert("app") && t.search("app"));
    assert_eq!(t.count_prefix("app"), 2);
    assert_eq!(t.count_prefix(""), 2);
    assert!(!t.search("")); // the empty word was never inserted
    assert!(t.delete("apple") && !t.search("apple") && t.search("app") && !t.delete("apple"));
    assert_eq!(t.nodes(), 4); // root + a,p,p: the "le" tail was pruned
    assert!(t.delete("app") && t.nodes() == 1);
    check_prefix_set::<ArrTrie>(0x1234_5678_9ABC_DEF1, &|r| gen_word(r, 6, 3));
    // delete verified against an oracle too: random insert/delete mix, node count equals a freshly built trie
    let mut r = Rng(0xD1E7_E000_0000_0001);
    for _ in 0..2000 {
        let (mut t, mut o) = (ArrTrie::default(), BTreeSet::<String>::new());
        for _ in 0..30 {
            let w = gen_word(&mut r, 5, 3);
            if r.range(3) == 0 {
                assert_eq!(t.delete(&w), o.remove(&w));
            } else {
                assert_eq!(t.insert(&w), o.insert(w.clone()));
            }
        }
        let mut fresh = ArrTrie::default();
        for w in &o {
            fresh.insert(w);
        }
        assert_eq!(
            t.nodes(),
            fresh.nodes(),
            "delete must prune exactly the dead branches"
        );
        assert_eq!(t.count_prefix(""), o.len());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. The same Trie with HashMap children (any `char`).
// Approach: `children: HashMap<char, Node>`; the node OWNS its children by value (no Box needed, the map is the indirection). Memory is
// proportional to edges actually present, not 26 slots per node; lookups pay a hash. BTreeMap instead gives sorted iteration for free.
// Rust wrinkle: insert is `cur = cur.children.entry(c).or_default();`: entry API creates the child if missing and hands back `&mut Node`,
// the one-liner interviewers love. Read-only walk: `cur = cur.children.get(&c)?;`. A `while let Some(n) = cur.children.get_mut(..)` insert
// loop fights the borrow checker (conditional return of a borrow), which is why `entry` is the idiom.
// Interviewer is testing: choosing a child container by alphabet size, entry API fluency, generics over the key (`K: Hash + Eq`).
// Mistakes: `get(&c).unwrap_or(&Node::default())` (temporary dropped), cloning nodes to dodge the borrow checker.
// Follow-ups: make it generic over `K` so it indexes `Vec<u32>` token sequences or path segments (routers); sorted words via BTreeMap.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct MapTrie {
    kids: HashMap<char, MapTrie>,
    end: bool,
    pass: usize,
}
impl MapTrie {
    fn find(&self, w: &str) -> Option<&MapTrie> {
        let mut cur = self;
        for c in w.chars() {
            cur = cur.kids.get(&c)?;
        }
        Some(cur)
    }
    fn insert(&mut self, w: &str) -> bool {
        if self.find(w).is_some_and(|n| n.end) {
            return false;
        }
        let mut cur = &mut *self;
        cur.pass += 1;
        for c in w.chars() {
            cur = cur.kids.entry(c).or_default();
            cur.pass += 1;
        }
        cur.end = true;
        true
    }
    fn words_with_prefix(&self, p: &str) -> Vec<String> {
        fn go(n: &MapTrie, path: &mut String, out: &mut Vec<String>) {
            if n.end {
                out.push(path.clone());
            }
            for (&c, k) in &n.kids {
                path.push(c);
                go(k, path, out);
                path.pop();
            }
        }
        let mut out = Vec::new();
        if let Some(n) = self.find(p) {
            go(n, &mut p.to_string(), &mut out);
        }
        out.sort(); // HashMap order is arbitrary: sort before comparing or printing
        out
    }
    fn nodes(&self) -> usize {
        1 + self.kids.values().map(|k| k.nodes()).sum::<usize>()
    }
}
impl PrefixSet for MapTrie {
    fn ins(&mut self, w: &str) -> bool {
        self.insert(w)
    }
    fn has(&self, w: &str) -> bool {
        self.find(w).is_some_and(|n| n.end)
    }
    fn pre(&self, p: &str) -> bool {
        self.find(p).is_some()
    }
    fn cnt(&self, p: &str) -> usize {
        self.find(p).map_or(0, |n| n.pass)
    }
}
fn q2() {
    check_prefix_set::<MapTrie>(0xAAAA_BBBB_CCCC_0001, &|r| gen_word(r, 6, 3));
    // arbitrary chars, and the enumeration matches the oracle
    let alpha = ['x', 'é', '日', '🦀', 'Z'];
    let mk = |r: &mut Rng| -> String {
        (0..1 + r.range(4))
            .map(|_| alpha[r.range(alpha.len())])
            .collect()
    };
    check_prefix_set::<MapTrie>(0xAAAA_BBBB_CCCC_0002, &mk);
    let mut r = Rng(0xABCD_EF01_2345_6789);
    for _ in 0..500 {
        let (mut t, mut o) = (MapTrie::default(), BTreeSet::new());
        for _ in 0..15 {
            let w = mk(&mut r);
            t.insert(&w);
            o.insert(w);
        }
        let p = {
            let q = mk(&mut r);
            rand_prefix(&mut r, &q)
        };
        let want: Vec<String> = o.iter().filter(|w| w.starts_with(&p)).cloned().collect();
        assert_eq!(t.words_with_prefix(&p), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. The same Trie as an arena: `Vec<Node>` with u32 child indices.
// Approach: all nodes in one Vec; 0 is the root and never a child, so `0` can mean "no child" and the node is `Copy`-friendly
// (`[0u32; 26]`, no Option, no Box). One allocation, cache-friendly, trivially serializable, cheap to drop (no recursive Drop).
// Cost: no real per-node free (use a free list or tombstone), indices instead of references (a bug is a wrong index, not a compile error).
// Rust wrinkle: `self.nodes[cur].kids[i]` is read first (a Copy u32), THEN `self.nodes.push(..)`: you cannot hold `&mut self.nodes[cur]`
// across a push (it may reallocate). Read the index out, mutate after: this is the whole borrow-checker story of arenas.
// Interviewer is testing: whether you can build it without Box/Rc, and trade-off talk (locality, Drop, serialization, 4 vs 8 byte links).
// Mistakes: holding a reference across `push`; using index 0 as both root and "none" without noticing the root can never be a child.
// Follow-ups: free list for delete; compact to a sorted array of (byte, child) per node for sparse alphabets; mmap-friendly flat layout.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default, Clone, Copy)]
struct ArenaNode {
    kids: [u32; 26], // 0 = no child (the root is index 0 and is never a child)
    end: bool,
    pass: u32,
}
struct ArenaTrie {
    nodes: Vec<ArenaNode>,
}
impl Default for ArenaTrie {
    fn default() -> Self {
        ArenaTrie {
            nodes: vec![ArenaNode::default()],
        }
    }
}
impl ArenaTrie {
    fn find(&self, w: &str) -> Option<usize> {
        let mut cur = 0usize;
        for b in w.bytes() {
            match self.nodes[cur].kids[ix(b)] {
                0 => return None,
                n => cur = n as usize,
            }
        }
        Some(cur)
    }
    fn insert(&mut self, w: &str) -> bool {
        if self.find(w).is_some_and(|n| self.nodes[n].end) {
            return false;
        }
        let mut cur = 0usize;
        self.nodes[0].pass += 1;
        for b in w.bytes() {
            let mut nx = self.nodes[cur].kids[ix(b)] as usize; // read first ...
            if nx == 0 {
                self.nodes.push(ArenaNode::default()); // ... then push (may reallocate)
                nx = self.nodes.len() - 1;
                self.nodes[cur].kids[ix(b)] = nx as u32;
            }
            cur = nx;
            self.nodes[cur].pass += 1;
        }
        self.nodes[cur].end = true;
        true
    }
}
impl PrefixSet for ArenaTrie {
    fn ins(&mut self, w: &str) -> bool {
        self.insert(w)
    }
    fn has(&self, w: &str) -> bool {
        self.find(w).is_some_and(|n| self.nodes[n].end)
    }
    fn pre(&self, p: &str) -> bool {
        self.find(p).is_some()
    }
    fn cnt(&self, p: &str) -> usize {
        self.find(p).map_or(0, |n| self.nodes[n].pass as usize)
    }
}
fn q3() {
    check_prefix_set::<ArenaTrie>(0x3333_4444_5555_0003, &|r| gen_word(r, 6, 3));
    // same structure as the boxed trie: identical node counts
    let mut r = Rng(0x0303_0303_0303_0303);
    let (mut a, mut b) = (ArrTrie::default(), ArenaTrie::default());
    for _ in 0..500 {
        let w = gen_word(&mut r, 7, 4);
        a.insert(&w);
        b.insert(&w);
    }
    assert_eq!(a.nodes(), b.nodes.len());
    println!(
        "    Q3 sizes: boxed node {} B (+ a heap allocation each), arena node {} B (one Vec, u32 links)",
        size_of::<ANode>(),
        size_of::<ArenaNode>()
    );
    assert!(size_of::<ArenaNode>() < size_of::<ANode>());
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. Add and search words with '.' wildcards (LeetCode 211).
// Approach: insert is the plain trie; search recurses: a letter follows one edge, '.' branches into every child. Time O(|w|) without
// wildcards, worst case O(26^k * ...) with k dots (bounded by the trie size). Prune early: a missing edge returns false immediately.
// Rust wrinkle: recursion over `&ANode` with a `&[u8]` pattern slice (split_first) is clean and needs no mutation or clones; `iter().flatten()`
// walks only the present children of an `[Option<Box<_>>; 26]`.
// Interviewer is testing: recursion over a trie, the branching factor of '.', handling a pattern that ends on a '.' (needs `end`).
// Mistakes: returning true when the pattern is exhausted without checking `end`; treating '.' as "skip one level" without trying children.
// Follow-ups: '*' (zero or more) needs DP/memo on (node, pattern index); wildcard-only prefix queries; regex-lite matching.
// ---------------------------------------------------------------------------------------------------------------------
fn search_wild(node: &ANode, pat: &[u8]) -> bool {
    match pat.split_first() {
        None => node.end,
        Some((b'.', rest)) => node.kids.iter().flatten().any(|k| search_wild(k, rest)),
        Some((&b, rest)) => node.kids[ix(b)]
            .as_deref()
            .is_some_and(|k| search_wild(k, rest)),
    }
}
fn wild_match(word: &str, pat: &str) -> bool {
    word.len() == pat.len()
        && word
            .bytes()
            .zip(pat.bytes())
            .all(|(w, p)| p == b'.' || p == w)
}
fn q4() {
    let mut t = ArrTrie::default();
    for w in ["bad", "dad", "mad"] {
        t.insert(w);
    }
    assert!(
        !search_wild(&t.root, b"pad")
            && search_wild(&t.root, b"bad")
            && search_wild(&t.root, b".ad")
    );
    assert!(
        search_wild(&t.root, b"b..")
            && !search_wild(&t.root, b"b.")
            && !search_wild(&t.root, b"..d.")
    );
    let mut r = Rng(0x4444_0000_1111_2222);
    for _ in 0..2000 {
        let mut t = ArrTrie::default();
        let ws: Vec<String> = (0..10).map(|_| gen_word(&mut r, 5, 3)).collect();
        for w in &ws {
            t.insert(w);
        }
        for _ in 0..10 {
            let pat: String = gen_word(&mut r, 5, 3)
                .chars()
                .map(|c| if r.range(3) == 0 { '.' } else { c })
                .collect();
            let want = ws.iter().any(|w| wild_match(w, &pat));
            assert_eq!(search_wild(&t.root, pat.as_bytes()), want, "{pat}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. Longest common prefix of a set of words (LeetCode 14). Is a trie the right tool?
// Approach A (best): vertical scan: compare the first word with each other, shrinking the common length. O(total chars), O(1) extra.
// Approach B: sort, then the LCP of the set is the LCP of the first and last word. O(n log n * L). Approach C (trie): insert all words and
// walk from the root while the node has exactly one child and is not a word end. O(total chars) time but O(total chars) memory.
// Say it plainly in the interview: a trie is overkill here; it only wins if the same word set answers MANY prefix queries.
// Rust wrinkle: slice by BYTE index only at a char boundary: compare `as_bytes()` for the length, and since we stop at the first differing
// byte of equal-prefix strings, the cut is always a char boundary for valid UTF-8 (shared bytes before the first difference).
// Interviewer is testing: choosing the simplest correct tool, empty-list and empty-string edge cases.
// Mistakes: panicking on an empty slice (`words[0]`); building a trie because the topic is tries.
// Follow-ups: LCP of every pair (LCP array of a sorted list); longest common SUFFIX (reverse the words); common prefix of file paths by '/' segments.
// ---------------------------------------------------------------------------------------------------------------------
fn common_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}
fn lcp_vertical(ws: &[String]) -> String {
    let Some(first) = ws.first() else {
        return String::new();
    };
    let end = ws[1..]
        .iter()
        .fold(first.len(), |end, w| end.min(common_len(first, w)));
    first[..end].to_string()
}
fn lcp_sorted(ws: &[String]) -> String {
    if ws.is_empty() {
        return String::new();
    }
    let mut v = ws.to_vec();
    v.sort();
    let (a, b) = (&v[0], &v[v.len() - 1]);
    a[..common_len(a, b)].to_string()
}
fn lcp_trie(ws: &[String]) -> String {
    let mut t = ArrTrie::default();
    for w in ws {
        t.insert(w);
    }
    let (mut cur, mut out) = (&t.root, String::new());
    loop {
        let mut kids = cur
            .kids
            .iter()
            .enumerate()
            .filter_map(|(i, k)| k.as_deref().map(|k| (i, k)));
        match (kids.next(), kids.next()) {
            (Some((i, k)), None) if !cur.end => {
                out.push((b'a' + i as u8) as char);
                cur = k;
            }
            _ => return out,
        }
    }
}
fn q5() {
    for (ws, want) in [
        (vec!["flower", "flow", "flight"], "fl"),
        (vec!["dog", "racecar", "car"], ""),
        (vec![], ""),
        (vec!["solo"], "solo"),
        (vec!["", "a"], ""),
    ] {
        let ws: Vec<String> = ws.into_iter().map(String::from).collect();
        assert_eq!(lcp_vertical(&ws), want);
        assert_eq!(lcp_sorted(&ws), want);
        assert_eq!(lcp_trie(&ws), want);
    }
    let mut r = Rng(0x5555_AAAA_5555_AAAA);
    for _ in 0..2000 {
        let n = r.range(6);
        let ws: Vec<String> = (0..n).map(|_| gen_word(&mut r, 6, 2)).collect();
        // oracle: the shortest of the pairwise common lengths (n >= 2), or the word itself
        let want = match ws.len() {
            0 => String::new(),
            1 => ws[0].clone(),
            _ => {
                let m = (0..ws.len())
                    .flat_map(|i| (i + 1..ws.len()).map(move |j| (i, j)))
                    .map(|(i, j)| common_len(&ws[i], &ws[j]))
                    .min()
                    .unwrap();
                ws[0][..m].to_string()
            }
        };
        assert_eq!(lcp_vertical(&ws), want);
        assert_eq!(lcp_sorted(&ws), want);
        assert_eq!(lcp_trie(&ws), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Replace Words (LeetCode 648): replace each word of a sentence by the shortest dictionary root that is a prefix of it.
// Approach: trie of roots; for each word walk until a node with `end` (that prefix is the shortest root) or the walk dies (keep the word).
// O(total sentence chars + total root chars). Alternative: HashSet of roots and try every prefix length of every word: O(L^2) per word.
// Rust wrinkle: return a `&str` slice of the input word (no allocation): the borrow lives as long as the sentence. Check `end` BEFORE
// consuming the next byte so the shortest root wins.
// Interviewer is testing: early exit at the first end marker, handling words with no root, linear time claim.
// Mistakes: choosing the longest root (continuing past `end`); allocating a String per prefix; forgetting a root equal to the whole word.
// Follow-ups: longest root instead; replace with the root's id; streaming sentence; roots with uppercase.
// ---------------------------------------------------------------------------------------------------------------------
fn replace_words(roots: &[String], sentence: &str) -> String {
    let mut t = ArrTrie::default();
    for r in roots {
        t.insert(r);
    }
    fn root_of<'a>(t: &ArrTrie, w: &'a str) -> &'a str {
        let mut cur = &t.root;
        for (i, b) in w.bytes().enumerate() {
            if cur.end {
                return &w[..i];
            }
            match cur.kids[ix(b)].as_deref() {
                Some(n) => cur = n,
                None => return w,
            }
        }
        w
    }
    sentence
        .split_whitespace()
        .map(|w| root_of(&t, w))
        .collect::<Vec<_>>()
        .join(" ")
}
fn q6() {
    let roots: Vec<String> = ["cat", "bat", "rat"].map(String::from).to_vec();
    assert_eq!(
        replace_words(&roots, "the cattle was rattled by the battery"),
        "the cat was rat by the bat"
    );
    assert_eq!(replace_words(&[], "a b"), "a b");
    let mut r = Rng(0x6666_6666_0000_1111);
    for _ in 0..2000 {
        let roots: Vec<String> = (0..1 + r.range(5))
            .map(|_| gen_word(&mut r, 3, 3))
            .collect();
        let sentence: Vec<String> = (0..1 + r.range(6))
            .map(|_| gen_word(&mut r, 7, 3))
            .collect();
        let want: Vec<&str> = sentence
            .iter()
            .map(|w| {
                roots
                    .iter()
                    .filter(|x| w.starts_with(x.as_str()))
                    .min_by_key(|x| x.len())
                    .map_or(w.as_str(), |x| &w[..x.len()])
            })
            .collect();
        assert_eq!(replace_words(&roots, &sentence.join(" ")), want.join(" "));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. Longest word in dictionary built one character at a time (LeetCode 720). Ties: lexicographically smallest.
// Approach: trie, then DFS that only descends into children that are themselves word ends (every prefix must be a word). Visiting
// children in a..z order and updating only on a STRICTLY longer path makes the first-found word of each length the smallest. O(total chars).
// Alternative: sort the words and keep a HashSet of buildable ones: also O(n log n * L).
// Rust wrinkle: DFS with a shared `Vec<u8>` path (push, recurse, pop) instead of cloning a String per node.
// Interviewer is testing: the "all prefixes must be words" condition (the empty prefix is free) and the tie-break argument.
// Mistakes: updating on `>=` (picks the larger string); descending through non-end nodes; recursing before checking `end`.
// Follow-ups: return ALL longest words; words may be built from the end too; BFS version (last level reached, first in order).
// ---------------------------------------------------------------------------------------------------------------------
fn longest_buildable(words: &[String]) -> String {
    let mut t = ArrTrie::default();
    for w in words {
        t.insert(w);
    }
    fn dfs(n: &ANode, path: &mut Vec<u8>, best: &mut Vec<u8>) {
        if path.len() > best.len() {
            *best = path.clone();
        }
        for (i, k) in n.kids.iter().enumerate() {
            if let Some(k) = k.as_deref().filter(|k| k.end) {
                path.push(b'a' + i as u8);
                dfs(k, path, best);
                path.pop();
            }
        }
    }
    let mut best = Vec::new();
    dfs(&t.root, &mut Vec::new(), &mut best);
    String::from_utf8(best).unwrap()
}
fn q7() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        longest_buildable(&v(&["w", "wo", "wor", "worl", "world"])),
        "world"
    );
    assert_eq!(
        longest_buildable(&v(&["a", "banana", "app", "appl", "ap", "apply", "apple"])),
        "apple"
    );
    assert_eq!(longest_buildable(&v(&["b", "ba", "a", "ab"])), "ab"); // tie between "ab" and "ba"
    assert_eq!(longest_buildable(&v(&["xyz"])), "");
    let mut r = Rng(0x7777_1234_8888_4321);
    for _ in 0..2000 {
        let ws: Vec<String> = (0..2 + r.range(14))
            .map(|_| gen_word(&mut r, 4, 2))
            .collect();
        let set: HashSet<&str> = ws.iter().map(|s| s.as_str()).collect();
        let mut best = String::new();
        for w in &ws {
            let ok = (1..=w.len()).all(|k| set.contains(&w[..k]));
            if ok && (w.len() > best.len() || (w.len() == best.len() && *w < best)) {
                best = w.clone();
            }
        }
        assert_eq!(longest_buildable(&ws), best, "{ws:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. Map Sum Pairs (LeetCode 677): insert(key, val) overwrites; sum(prefix) = sum of values of keys with that prefix.
// Approach: each node stores the sum of values below it. Overwriting must add the DELTA (new - old), so keep a side HashMap key -> value.
// insert O(|key|), sum O(|prefix|). Naive: scan all keys per query, O(n * L).
// Rust wrinkle: `HashMap::insert` returns the old value (`Option<i64>`), which is exactly the delta you need: `val - old.unwrap_or(0)`.
// Interviewer is testing: the overwrite subtlety (adding val twice is the classic bug) and aggregate-in-node as a general pattern.
// Mistakes: adding the full new value on overwrite; not storing old values; summing in the DFS at query time (slow).
// Follow-ups: delete a key (subtract its value); max over a prefix (needs a heap/multiset per node, or recompute); count instead of sum.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct SumNode {
    kids: [Option<Box<SumNode>>; 26],
    sum: i64,
}
#[derive(Default)]
struct MapSum {
    root: SumNode,
    vals: HashMap<String, i64>,
}
impl MapSum {
    fn insert(&mut self, key: &str, val: i64) {
        let delta = val - self.vals.insert(key.to_string(), val).unwrap_or(0);
        let mut cur = &mut self.root;
        cur.sum += delta;
        for b in key.bytes() {
            cur = cur.kids[ix(b)].get_or_insert_with(Default::default);
            cur.sum += delta;
        }
    }
    fn sum(&self, prefix: &str) -> i64 {
        let mut cur = &self.root;
        for b in prefix.bytes() {
            match cur.kids[ix(b)].as_deref() {
                Some(n) => cur = n,
                None => return 0,
            }
        }
        cur.sum
    }
}
fn q8() {
    let mut m = MapSum::default();
    m.insert("apple", 3);
    assert_eq!(m.sum("ap"), 3);
    m.insert("app", 2);
    assert_eq!(m.sum("ap"), 5);
    m.insert("apple", 2); // overwrite: 3 -> 2
    assert_eq!(m.sum("ap"), 4);
    assert_eq!(m.sum("b"), 0);
    let mut r = Rng(0x8888_0808_8888_0808);
    for _ in 0..2000 {
        let (mut m, mut o) = (MapSum::default(), BTreeMap::<String, i64>::new());
        for _ in 0..25 {
            if r.range(2) == 0 {
                let (k, v) = (gen_word(&mut r, 4, 3), r.range(21) as i64 - 10);
                m.insert(&k, v);
                o.insert(k, v);
            } else {
                let p = gen_word(&mut r, 3, 3);
                assert_eq!(
                    m.sum(&p),
                    o.iter()
                        .filter(|(k, _)| k.starts_with(&p))
                        .map(|(_, v)| v)
                        .sum::<i64>()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. Count distinct substrings of a string. Suffix trie vs suffix array vs suffix automaton.
// Approach A (suffix trie): insert every suffix into a trie; every node (except the root) is one distinct substring. O(n^2) time AND space.
// Approach B (suffix array + LCP): sort suffixes; distinct = n(n+1)/2 - sum(lcp of neighbours). Kasai builds the LCP in O(n); the sort here
// is the simple O(n^2 log n) one (SA-IS or doubling gets O(n) / O(n log n)). Approach C (suffix automaton): after construction,
// distinct = sum over states of len[v] - len[link[v]]; O(n * alphabet) time, at most 2n states, online (append a char at a time).
// Rust wrinkle: automaton states are arena indices in parallel `Vec`s; `usize::MAX` marks the root's missing link; cloning a state is
// `next.push(next[q])` because `[usize; 26]` is Copy (the same fact that made `[0u32; 26]` fine in Q3).
// Interviewer is testing: knowing a naive trie is quadratic and naming the better structure; a clean count identity.
// Mistakes: counting nodes including the root; building suffix tries for n = 10^5 (10^10 nodes).
// Follow-ups: k-th smallest distinct substring; longest repeated substring (max LCP); count occurrences of each substring (automaton endpos).
// ---------------------------------------------------------------------------------------------------------------------
fn distinct_suffix_trie(s: &str) -> usize {
    let mut t = ArrTrie::default();
    for i in 0..s.len() {
        t.insert(&s[i..]);
    }
    t.nodes() - 1
}
fn distinct_suffix_array(s: &str) -> usize {
    let b = s.as_bytes();
    let n = b.len();
    let mut sa: Vec<usize> = (0..n).collect();
    sa.sort_by(|&x, &y| b[x..].cmp(&b[y..]));
    let mut rank = vec![0; n];
    for (i, &p) in sa.iter().enumerate() {
        rank[p] = i;
    }
    let (mut lcp_sum, mut h) = (0, 0usize);
    for i in 0..n {
        // Kasai: lcp[rank[i]] >= lcp[rank[i-1]] - 1, so h only ever decreases by one per step: O(n) total
        if rank[i] > 0 {
            let j = sa[rank[i] - 1];
            while i + h < n && j + h < n && b[i + h] == b[j + h] {
                h += 1;
            }
            lcp_sum += h;
            h = h.saturating_sub(1);
        } else {
            h = 0;
        }
    }
    n * (n + 1) / 2 - lcp_sum
}
fn distinct_suffix_automaton(s: &str) -> usize {
    let (mut len, mut link, mut next) = (vec![0usize], vec![usize::MAX], vec![[0usize; 26]]); // transition 0 = none: the root is never a target
    let mut last = 0;
    for b in s.bytes() {
        let c = ix(b);
        let cur = len.len();
        len.push(len[last] + 1);
        link.push(0);
        next.push([0; 26]);
        let mut p = Some(last);
        while let Some(x) = p {
            if next[x][c] != 0 {
                break;
            }
            next[x][c] = cur;
            p = (link[x] != usize::MAX).then(|| link[x]);
        }
        if let Some(x) = p {
            let q = next[x][c];
            if len[x] + 1 == len[q] {
                link[cur] = q;
            } else {
                let clone = len.len();
                len.push(len[x] + 1);
                link.push(link[q]);
                next.push(next[q]);
                let mut p2 = Some(x);
                while let Some(y) = p2 {
                    if next[y][c] != q {
                        break;
                    }
                    next[y][c] = clone;
                    p2 = (link[y] != usize::MAX).then(|| link[y]);
                }
                link[q] = clone;
                link[cur] = clone;
            }
        }
        last = cur;
    }
    (1..len.len()).map(|v| len[v] - len[link[v]]).sum()
}
fn q9() {
    for (s, want) in [("", 0), ("a", 1), ("aaa", 3), ("abab", 7), ("banana", 15)] {
        assert_eq!(distinct_suffix_trie(s), want, "{s}");
        assert_eq!(distinct_suffix_array(s), want, "{s}");
        assert_eq!(distinct_suffix_automaton(s), want, "{s}");
    }
    let mut r = Rng(0x9999_0000_ABCD_EF99);
    for _ in 0..2000 {
        let s = if r.range(10) == 0 {
            String::new()
        } else {
            {
                let a = 2 + r.range(2);
                gen_word(&mut r, 16, a)
            }
        };
        let want = (0..s.len())
            .flat_map(|i| (i + 1..=s.len()).map(move |j| (i, j)))
            .map(|(i, j)| &s[i..j])
            .collect::<HashSet<_>>()
            .len();
        assert_eq!(distinct_suffix_trie(&s), want, "{s}");
        assert_eq!(distinct_suffix_array(&s), want, "{s}");
        assert_eq!(distinct_suffix_automaton(&s), want, "{s}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Autocomplete: the top-k most frequent words for a prefix (ties: alphabetical).
// Approach A (per-node cache): every node keeps its subtree's best k (count, word); insert updates the k-lists along the path; a query is
// O(|prefix| + k). Memory O(nodes * k * word length) and updates cost O(|word| * k log k), so it suits read-heavy, mostly static data.
// Approach B (DFS + heap): find the prefix node, DFS the whole subtree keeping a min-heap of size k. Query O(subtree size * log k), no extra
// memory, simple updates. Production engines mix: cache for short hot prefixes, DFS for long cold ones.
// Rust wrinkle: ranking key `(count, Reverse(word))`: bigger is better, so ties prefer the alphabetically smaller word; wrap in `Reverse` to
// make `BinaryHeap` (a max-heap) behave as a min-heap that evicts the worst; `into_sorted_vec()` of Reverse items comes out best-first.
// Interviewer is testing: the complexity trade-off discussion, correct tie-breaking, heap direction.
// Mistakes: sorting the whole subtree (O(m log m)); max-heap of size k evicting the best; comparing by count only (non-deterministic ties).
// Follow-ups: counts that change online (update lists on every increment), typo tolerance (Q14), personalization, sharding by prefix.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct TopNode {
    kids: [Option<Box<TopNode>>; 26],
    count: Option<u32>,      // Some(freq) if a word ends here
    top: Vec<(u32, String)>, // approach A: best k words in this subtree, best first
}
struct Autocomplete {
    root: TopNode,
    k: usize,
}
fn better(a: &(u32, String), b: &(u32, String)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1))
}
impl Autocomplete {
    fn new(k: usize) -> Self {
        Autocomplete {
            root: TopNode::default(),
            k,
        }
    }
    fn insert(&mut self, w: &str, count: u32) {
        let k = self.k;
        let mut cur = &mut self.root;
        let touch = |n: &mut TopNode| {
            n.top.push((count, w.to_string()));
            n.top.sort_by(better);
            n.top.truncate(k);
        };
        touch(cur);
        for b in w.bytes() {
            cur = cur.kids[ix(b)].get_or_insert_with(Default::default);
            touch(cur);
        }
        cur.count = Some(count);
    }
    fn find(&self, p: &str) -> Option<&TopNode> {
        let mut cur = &self.root;
        for b in p.bytes() {
            cur = cur.kids[ix(b)].as_deref()?;
        }
        Some(cur)
    }
    fn top_cached(&self, p: &str) -> Vec<String> {
        self.find(p)
            .map_or_else(Vec::new, |n| n.top.iter().map(|x| x.1.clone()).collect())
    }
    fn top_dfs(&self, p: &str) -> Vec<String> {
        fn go(
            n: &TopNode,
            path: &mut String,
            heap: &mut BinaryHeap<Reverse<(u32, Reverse<String>)>>,
            k: usize,
        ) {
            if let Some(c) = n.count {
                heap.push(Reverse((c, Reverse(path.clone()))));
                if heap.len() > k {
                    heap.pop(); // evict the current worst
                }
            }
            for (i, kid) in n.kids.iter().enumerate() {
                if let Some(kid) = kid {
                    path.push((b'a' + i as u8) as char);
                    go(kid, path, heap, k);
                    path.pop();
                }
            }
        }
        let mut heap = BinaryHeap::new();
        if let Some(n) = self.find(p) {
            go(n, &mut p.to_string(), &mut heap, self.k);
        }
        heap.into_sorted_vec()
            .into_iter()
            .map(|Reverse((_, Reverse(w)))| w)
            .collect()
    }
}
fn q10() {
    let mut ac = Autocomplete::new(2);
    for (w, c) in [
        ("i love you", 5),
        ("island", 3),
        ("iroman", 2),
        ("i love leetcode", 2),
    ] {
        // spaces are outside a-z: use letters only for the trie
        ac.insert(&w.replace(' ', ""), c);
    }
    assert_eq!(
        ac.top_cached("i"),
        vec!["iloveyou".to_string(), "island".to_string()]
    );
    assert_eq!(ac.top_dfs("i"), ac.top_cached("i"));
    assert!(ac.top_cached("zzz").is_empty() && ac.top_dfs("zzz").is_empty());
    let mut r = Rng(0xA10A_10A1_0A10_A10A);
    for _ in 0..2000 {
        let k = 1 + r.range(4);
        let mut freq = BTreeMap::<String, u32>::new();
        for _ in 0..1 + r.range(14) {
            freq.insert(gen_word(&mut r, 5, 3), 1 + r.range(5) as u32);
        }
        let mut ac = Autocomplete::new(k);
        for (w, &c) in &freq {
            ac.insert(w, c);
        }
        for _ in 0..8 {
            let p = {
                let q = gen_word(&mut r, 5, 3);
                rand_prefix(&mut r, &q)
            };
            let mut all: Vec<(u32, String)> = freq
                .iter()
                .filter(|(w, _)| w.starts_with(&p))
                .map(|(w, &c)| (c, w.clone()))
                .collect();
            all.sort_by(better);
            let want: Vec<String> = all.into_iter().take(k).map(|x| x.1).collect();
            assert_eq!(ac.top_cached(&p), want, "cached {p}");
            assert_eq!(ac.top_dfs(&p), want, "dfs {p}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Search Suggestions System (LeetCode 1268): after each letter of `search`, return up to 3 lexicographically smallest products
// that start with the typed prefix.
// Approach A (sorted array + binary search): sort once; for each typed prefix, `partition_point(|p| p < prefix)` gives the first candidate and the
// next <= 3 items are checked with `starts_with`. Because the answer range only shrinks as the prefix grows, restart each search from the
// previous lower bound. O(n log n + |search| log n). Approach B (trie): insert all, and DFS in a..z order from the prefix node collecting 3.
// O(total chars) build, O(|search| * (3 * L)) query. A usually wins on memory and simplicity; B wins when you also need other trie features.
// Rust wrinkle: `slice.partition_point` is the stable-Rust lower_bound; do not hand-roll the binary search (off-by-one factory).
// Interviewer is testing: reaching for the sorted array (the "trie is not mandatory" insight) and the monotone lower bound.
// Mistakes: forgetting to sort; checking only the first candidate; returning products shorter than the prefix.
// Follow-ups: products with duplicates; updates (a BTreeSet with `range(prefix..)`); return top 3 by popularity (Q10).
// ---------------------------------------------------------------------------------------------------------------------
fn suggest_sorted(products: &[String], search: &str) -> Vec<Vec<String>> {
    let mut sorted = products.to_vec();
    sorted.sort();
    let (mut lo, mut out) = (0usize, Vec::new());
    for i in 1..=search.len() {
        let prefix = &search[..i];
        lo += sorted[lo..].partition_point(|p| p.as_str() < prefix);
        out.push(
            sorted[lo..]
                .iter()
                .take(3)
                .take_while(|p| p.starts_with(prefix))
                .cloned()
                .collect(),
        );
    }
    out
}
fn suggest_trie(products: &[String], search: &str) -> Vec<Vec<String>> {
    let mut t = ArrTrie::default();
    for p in products {
        t.insert(p);
    }
    fn go(n: &ANode, path: &mut String, out: &mut Vec<String>) {
        if out.len() == 3 {
            return;
        }
        if n.end {
            out.push(path.clone());
        }
        for (i, k) in n.kids.iter().enumerate() {
            if let Some(k) = k {
                path.push((b'a' + i as u8) as char);
                go(k, path, out);
                path.pop();
            }
        }
    }
    (1..=search.len())
        .map(|i| {
            let mut out = Vec::new();
            if let Some(n) = t.find(&search[..i]) {
                go(n, &mut search[..i].to_string(), &mut out);
            }
            out
        })
        .collect()
}
fn q11() {
    let ps: Vec<String> = ["mobile", "mouse", "moneypot", "monitor", "mousepad"]
        .map(String::from)
        .to_vec();
    let want: Vec<Vec<&str>> = vec![
        vec!["mobile", "moneypot", "monitor"],
        vec!["mobile", "moneypot", "monitor"],
        vec!["mouse", "mousepad"],
        vec!["mouse", "mousepad"],
        vec!["mouse", "mousepad"],
    ];
    assert_eq!(suggest_sorted(&ps, "mouse"), want);
    assert_eq!(suggest_trie(&ps, "mouse"), want);
    let mut r = Rng(0xB11B_11B1_1B11_B11B);
    for _ in 0..2000 {
        let set: BTreeSet<String> = (0..r.range(12)).map(|_| gen_word(&mut r, 5, 3)).collect(); // LeetCode: products are distinct
        let ps: Vec<String> = set.iter().rev().cloned().collect(); // unsorted input
        let search = gen_word(&mut r, 6, 3);
        let want: Vec<Vec<String>> = (1..=search.len())
            .map(|i| {
                set.iter()
                    .filter(|p| p.starts_with(&search[..i]))
                    .take(3)
                    .cloned()
                    .collect()
            })
            .collect();
        assert_eq!(suggest_sorted(&ps, &search), want);
        assert_eq!(suggest_trie(&ps, &search), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. Word Break (LeetCode 139): can `s` be segmented into dictionary words?
// Approach: dp[i] = prefix of length i is segmentable. From each reachable i, walk the trie along s[i..]; each time the walk hits a word end at
// j, mark dp[j+1]. The walk stops as soon as the trie edge is missing, so total work is O(n * L) where L = longest word, instead of
// O(n^2) substring-hashes. Space O(n + trie).
// Rust wrinkle: iterate `s.as_bytes()` by index and keep the trie cursor as `&ANode`; `let Some(n) = cur.kids[..].as_deref() else { break }`
// is the tidy early-exit. No substring allocation anywhere.
// Interviewer is testing: DP over positions plus pruning with the trie; recognizing plain recursion is exponential without memoization.
// Mistakes: greedy longest-match (wrong: "aaaaa" with {"aaa","aa"} needs 3+2, and greedy takes 3 then fails the rest of other inputs);
// restarting the trie walk from the root at every character instead of from every reachable boundary.
// Follow-ups: Word Break II (return all sentences, backtracking over the dp); count segmentations (dp of counts); minimum words.
// ---------------------------------------------------------------------------------------------------------------------
fn word_break(s: &str, dict: &[String]) -> bool {
    let mut t = ArrTrie::default();
    for w in dict {
        t.insert(w);
    }
    let b = s.as_bytes();
    let mut dp = vec![false; b.len() + 1];
    dp[0] = true;
    for i in 0..b.len() {
        if !dp[i] {
            continue;
        }
        let mut cur = &t.root;
        for j in i..b.len() {
            let Some(n) = cur.kids[ix(b[j])].as_deref() else {
                break;
            };
            cur = n;
            if cur.end {
                dp[j + 1] = true;
            }
        }
    }
    dp[b.len()]
}
fn q12() {
    let d = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(word_break("leetcode", &d(&["leet", "code"])));
    assert!(word_break("applepenapple", &d(&["apple", "pen"])));
    assert!(!word_break(
        "catsandog",
        &d(&["cats", "dog", "sand", "and", "cat"])
    ));
    assert!(word_break("", &d(&["a"])));
    fn brute(s: &str, set: &HashSet<&str>) -> bool {
        s.is_empty() || (1..=s.len()).any(|k| set.contains(&s[..k]) && brute(&s[k..], set))
    }
    let mut r = Rng(0xC12C_12C1_2C12_C12C);
    for _ in 0..2000 {
        let dict: Vec<String> = (0..1 + r.range(4))
            .map(|_| gen_word(&mut r, 4, 2))
            .collect();
        let s = if r.range(8) == 0 {
            String::new()
        } else {
            gen_word(&mut r, 12, 2)
        };
        let set: HashSet<&str> = dict.iter().map(|x| x.as_str()).collect();
        assert_eq!(word_break(&s, &dict), brute(&s, &set), "{s} {dict:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. T9 phone keypad: given digits like "4663", list the dictionary words they can spell ("good", "home", ...).
// Approach: trie over letters; DFS where each digit expands to its 3-4 letters, following only edges that exist. The trie prunes dead
// letter combinations immediately, versus generating all 4^n strings and hashing each. Alternative: precompute word -> digit string and
// group in a HashMap<String, Vec<String>> (O(1) lookups, but no prefix completion as the user types).
// Rust wrinkle: keypad as `const KEYPAD: [&str; 8]` indexed by digit - 2; an out-of-range digit must be handled (return empty), not panic.
// Interviewer is testing: combining letter-combinations backtracking with a trie for pruning, and the trade-off against the hash grouping.
// Mistakes: indexing with the digit char instead of `digit - b'2'`; forgetting the `end` check (returns prefixes, not words).
// Follow-ups: predictive text (words whose digit string STARTS with the input: end check removed, collect subtree); frequency ranking (Q10).
// ---------------------------------------------------------------------------------------------------------------------
const KEYPAD: [&str; 8] = ["abc", "def", "ghi", "jkl", "mno", "pqrs", "tuv", "wxyz"];
fn t9_digit(c: u8) -> u8 {
    b'2' + KEYPAD
        .iter()
        .position(|k| k.as_bytes().contains(&c))
        .unwrap() as u8
}
fn t9_words(t: &ArrTrie, digits: &str) -> Vec<String> {
    fn go(n: &ANode, d: &[u8], path: &mut String, out: &mut Vec<String>) {
        match d.split_first() {
            None => {
                if n.end {
                    out.push(path.clone());
                }
            }
            Some((&dg, rest)) => {
                let Some(letters) = dg.checked_sub(b'2').and_then(|i| KEYPAD.get(i as usize))
                else {
                    return;
                };
                for c in letters.chars() {
                    if let Some(k) = n.kids[ix(c as u8)].as_deref() {
                        path.push(c);
                        go(k, rest, path, out);
                        path.pop();
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    if !digits.is_empty() {
        go(&t.root, digits.as_bytes(), &mut String::new(), &mut out);
    }
    out.sort();
    out
}
fn q13() {
    let mut t = ArrTrie::default();
    for w in ["good", "home", "gone", "hood", "hoof", "gold"] {
        t.insert(w);
    }
    assert_eq!(
        t9_words(&t, "4663"),
        vec!["gone", "good", "home", "hood", "hoof"]
    );
    assert!(t9_words(&t, "0").is_empty() && t9_words(&t, "").is_empty());
    let mut r = Rng(0x1313_1313_9999_7777);
    for _ in 0..2000 {
        let ws: BTreeSet<String> = (0..1 + r.range(40))
            .map(|_| gen_word(&mut r, 4, 9))
            .collect(); // letters a..i: digits 2,3,4
        let mut t = ArrTrie::default();
        for w in &ws {
            t.insert(w);
        }
        for _ in 0..6 {
            let digits: String = if r.range(2) == 0 {
                ws.iter()
                    .nth(r.range(ws.len()))
                    .unwrap()
                    .bytes()
                    .map(|b| t9_digit(b) as char)
                    .collect()
            } else {
                (0..1 + r.range(4))
                    .map(|_| (b'2' + r.range(3) as u8) as char)
                    .collect()
            };
            let want: Vec<String> = ws
                .iter()
                .filter(|w| {
                    w.len() == digits.len()
                        && w.bytes().zip(digits.bytes()).all(|(c, d)| t9_digit(c) == d)
                })
                .cloned()
                .collect();
            assert_eq!(t9_words(&t, &digits), want, "{digits}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. Spell checker: every dictionary word within edit distance k of a (mis)typed word.
// Approach: DFS the trie carrying one row of the Levenshtein DP table per node (row[j] = distance between the path so far and target[..j]).
// Each child costs O(|target|); prune a subtree as soon as `min(row) > k`, since distances never decrease deeper. This shares the work for
// common prefixes, unlike running the full O(|a||b|) DP against every word. Alternatives: BK-tree, deletion-neighbour hashing (SymSpell).
// Rust wrinkle: the row is a fresh `Vec<usize>` per child (small, short-lived) while `path` is the shared `String` push/pop buffer.
// Interviewer is testing: the DP recurrence inside a DFS, the pruning bound, and edge cases (empty typo, word with k >= length).
// Mistakes: pruning on `row.last()` instead of the row minimum; forgetting the dictionary word ends at the node (`end`) before reporting.
// Follow-ups: transposition (Damerau) distance; ranking suggestions by frequency; fuzzy autocomplete (prefix distance: report at any node).
// ---------------------------------------------------------------------------------------------------------------------
fn within_edits(t: &ArrTrie, target: &str, k: usize) -> Vec<String> {
    fn go(
        n: &ANode,
        tg: &[u8],
        prev: &[usize],
        path: &mut String,
        k: usize,
        out: &mut Vec<String>,
    ) {
        if n.end && prev[tg.len()] <= k {
            out.push(path.clone());
        }
        if *prev.iter().min().unwrap() > k {
            return;
        }
        for (i, kid) in n.kids.iter().enumerate() {
            let Some(kid) = kid else { continue };
            let ch = b'a' + i as u8;
            let mut row = vec![prev[0] + 1];
            for j in 1..=tg.len() {
                let cost = usize::from(tg[j - 1] != ch);
                row.push((prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost));
            }
            path.push(ch as char);
            go(kid, tg, &row, path, k, out);
            path.pop();
        }
    }
    let first: Vec<usize> = (0..=target.len()).collect();
    let mut out = Vec::new();
    go(
        &t.root,
        target.as_bytes(),
        &first,
        &mut String::new(),
        k,
        &mut out,
    );
    out.sort();
    out
}
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        d[i][0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
    }
    d[a.len()][b.len()]
}
fn q14() {
    let mut t = ArrTrie::default();
    for w in ["hello", "help", "hell", "shell", "world"] {
        t.insert(w);
    }
    assert_eq!(within_edits(&t, "helo", 1), vec!["hell", "hello", "help"]);
    assert_eq!(within_edits(&t, "hello", 0), vec!["hello"]);
    let mut r = Rng(0x1414_AAAA_1414_BBBB);
    for _ in 0..2000 {
        let ws: BTreeSet<String> = (0..1 + r.range(25))
            .map(|_| gen_word(&mut r, 6, 3))
            .collect();
        let mut t = ArrTrie::default();
        for w in &ws {
            t.insert(w);
        }
        let target = if r.range(10) == 0 {
            String::new()
        } else {
            gen_word(&mut r, 6, 3)
        };
        let k = r.range(3);
        let want: Vec<String> = ws
            .iter()
            .filter(|w| edit_distance(w, &target) <= k)
            .cloned()
            .collect();
        assert_eq!(within_edits(&t, &target, k), want, "{target} k={k}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. Compress a trie into a radix (Patricia) tree.
// Approach: edges carry whole string labels; a node exists only where words end or branch. Insert walks to the edge sharing a first byte,
// takes the common prefix length, and SPLITS the edge when the label only partly matches (new middle node, old child hangs below it).
// Bound: every internal node either ends a word or has >= 2 children, so a radix tree over n words has at most 2n nodes (root included),
// no matter how long the words are, while a plain trie has up to total-characters nodes. Lookups stay O(|word|).
// Rust wrinkle: splitting needs the old (label, child) out of the Vec slot while you build the middle node:
// `std::mem::replace(&mut self.kids[i], placeholder)` is the standard trick (take by value without cloning the subtree).
// This sample is ASCII; for general UTF-8 split only at char boundaries (`is_char_boundary`).
// Interviewer is testing: the split case analysis (full match / partial match / no match), the 2n bound, and prefix queries ending mid-edge.
// Mistakes: matching the whole label or nothing (missing the partial-match split); forgetting that a query prefix can end inside an edge label.
// Follow-ups: delete with merge (a node left with one child and no word merges into its edge); store ranges into one big buffer instead of Strings.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct RNode {
    end: bool,
    kids: Vec<(String, RNode)>, // distinct first bytes
}
impl RNode {
    fn insert(&mut self, w: &str) -> bool {
        let Some(&first) = w.as_bytes().first() else {
            return !std::mem::replace(&mut self.end, true);
        };
        let Some(i) = self.kids.iter().position(|(l, _)| l.as_bytes()[0] == first) else {
            self.kids.push((
                w.to_string(),
                RNode {
                    end: true,
                    kids: vec![],
                },
            ));
            return true;
        };
        let common = common_len(&self.kids[i].0, w);
        if common < self.kids[i].0.len() {
            // partial match: split the edge into label[..common] -> mid -> (label[common..], old child)
            let (label, child) =
                std::mem::replace(&mut self.kids[i], (String::new(), RNode::default()));
            let mid = RNode {
                end: false,
                kids: vec![(label[common..].to_string(), child)],
            };
            self.kids[i] = (label[..common].to_string(), mid);
        }
        self.kids[i].1.insert(&w[common..])
    }
    /// The node at (or just below, if `p` ends mid-edge) the position of prefix `p`.
    fn locate(&self, p: &str) -> Option<&RNode> {
        let Some(&first) = p.as_bytes().first() else {
            return Some(self);
        };
        let (label, child) = self.kids.iter().find(|(l, _)| l.as_bytes()[0] == first)?;
        let common = common_len(label, p);
        if common == p.len() {
            Some(child) // p is a prefix of the label: everything below `child` matches
        } else if common == label.len() {
            child.locate(&p[common..])
        } else {
            None
        }
    }
    fn words(&self) -> usize {
        usize::from(self.end) + self.kids.iter().map(|(_, k)| k.words()).sum::<usize>()
    }
    fn nodes(&self) -> usize {
        1 + self.kids.iter().map(|(_, k)| k.nodes()).sum::<usize>()
    }
    fn has(&self, w: &str) -> bool {
        if w.is_empty() {
            return self.end;
        }
        self.kids
            .iter()
            .find(|(l, _)| w.starts_with(l.as_str()))
            .is_some_and(|(l, k)| k.has(&w[l.len()..]))
    }
}
#[derive(Default)]
struct Radix(RNode);
impl PrefixSet for Radix {
    fn ins(&mut self, w: &str) -> bool {
        self.0.insert(w)
    }
    fn has(&self, w: &str) -> bool {
        self.0.has(w)
    }
    fn pre(&self, p: &str) -> bool {
        self.0.locate(p).is_some()
    }
    fn cnt(&self, p: &str) -> usize {
        self.0.locate(p).map_or(0, |n| n.words())
    }
}
fn q15() {
    let mut t = Radix::default();
    for w in [
        "romane",
        "romanus",
        "romulus",
        "rubens",
        "ruber",
        "rubicon",
        "rubicundus",
    ] {
        t.ins(w);
    }
    assert_eq!(t.0.nodes(), 14); // the classic Wikipedia example: 7 words, 14 nodes (a plain trie needs one node per character)
    assert!(t.pre("rom") && t.pre("rubic") && !t.pre("rubx") && t.has("ruber") && !t.has("rube"));
    check_prefix_set::<Radix>(0x1515_1515_AAAA_0000, &|r| gen_word(r, 7, 3));
    let mut r = Rng(0x1515_DEAD_BEEF_0001);
    for _ in 0..500 {
        let (mut radix, mut plain) = (RNode::default(), ArrTrie::default());
        let mut n = 0;
        for _ in 0..30 {
            let w = gen_word(&mut r, 12, 2);
            if plain.insert(&w) {
                n += 1;
                assert!(radix.insert(&w));
            }
        }
        assert!(
            radix.nodes() <= 2 * n,
            "radix has {} nodes for {n} words",
            radix.nodes()
        );
        assert!(radix.nodes() <= plain.nodes());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q16. Trie vs HashSet vs sorted Vec for prefix queries: which wins and what does it cost?
// HashSet<String>: O(1) exact lookup, but a prefix query must scan everything (O(n * L)): no ordering at all.
// Sorted Vec<String>: `partition_point` finds the first match in O(log n * L); the matches are CONTIGUOUS, so count/enumerate costs O(log n + m).
// Memory is just the strings. Inserts are O(n) (shift); great for static dictionaries.
// Trie: O(|p|) prefix queries independent of n, incremental inserts, shared prefixes; but each node costs a fat child table. With
// `[Option<Box<Node>>; 26]` a node is ~224 bytes plus a heap allocation, far more than the one or two characters it represents.
// Rust wrinkle: `size_of::<Option<Box<T>>>() == size_of::<Box<T>>() == 8` (niche optimization: None is the null pointer), so the 26-slot array
// is 208 bytes. Measure instead of guessing: `size_of` for the node, node counts for the shape.
// Interviewer is testing: "it depends" backed by numbers: static vs dynamic data, memory budget, query mix, locality.
// Mistakes: assuming a trie is always smaller (it is usually bigger for random words); building a trie for exact lookups only.
// Follow-ups: compress to radix (Q15), DAWG/FST (the `fst` crate) for huge static sets, arena with sorted edge lists (Q3).
// ---------------------------------------------------------------------------------------------------------------------
fn sorted_count(v: &[String], p: &str) -> usize {
    let lo = v.partition_point(|w| w.as_str() < p);
    v[lo..].partition_point(|w| w.starts_with(p))
}
fn q16() {
    let mut r = Rng(0x1616_2222_3333_4444);
    for _ in 0..2000 {
        let set: BTreeSet<String> = (0..1 + r.range(40))
            .map(|_| gen_word(&mut r, 6, 3))
            .collect();
        let (mut t, hs) = (
            ArrTrie::default(),
            set.iter().cloned().collect::<HashSet<String>>(),
        );
        let v: Vec<String> = set.iter().cloned().collect();
        for w in &v {
            t.insert(w);
        }
        for _ in 0..10 {
            let p = {
                let q = gen_word(&mut r, 6, 3);
                rand_prefix(&mut r, &q)
            };
            let by_hash = hs.iter().filter(|w| w.starts_with(&p)).count(); // the scan a hash set forces
            assert_eq!(t.count_prefix(&p), by_hash);
            assert_eq!(sorted_count(&v, &p), by_hash);
        }
    }
    // memory: 5000 random lowercase words
    let mut r = Rng(0x1616_5555_6666_7777);
    let words: BTreeSet<String> = (0..5000).map(|_| gen_word(&mut r, 10, 26)).collect();
    let mut t = ArrTrie::default();
    let (mut chars, mut vec_bytes) = (0usize, 0usize);
    for w in &words {
        t.insert(w);
        chars += w.len();
        vec_bytes += size_of::<String>() + w.len();
    }
    let node_bytes = size_of::<ANode>();
    let trie_bytes = t.nodes() * (node_bytes + 16); // + ~16 B malloc overhead per Box
    println!(
        "    Q16: {} words, {chars} chars; trie {} nodes x {node_bytes} B = ~{} KiB; sorted Vec<String> ~{} KiB; Option<Box<_>> is {} B",
        words.len(),
        t.nodes(),
        trie_bytes / 1024,
        vec_bytes / 1024,
        size_of::<Option<Box<ANode>>>()
    );
    assert_eq!(size_of::<Option<Box<ANode>>>(), size_of::<usize>());
    assert!(t.nodes() <= chars + 1);
    assert!(
        trie_bytes > 5 * vec_bytes,
        "random words share few prefixes: the trie is the memory hog"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q17. Unicode: bytes or chars?
// Facts: Rust `str` is UTF-8. A trie over BYTES is correct for any valid `&str` (a valid prefix always ends on a char boundary, and UTF-8
// byte order equals code-point order), with a 256-way or small-map child table, but each non-ASCII char costs 2 to 4 nodes. A trie over
// `char` (Unicode scalar values) is shorter and simpler but needs a map child table (1.1M possible values). Neither handles NORMALIZATION
// ("é" as U+00E9 vs "e" + U+0301 are different strings) or CASE FOLDING ("İ".to_lowercase() is two chars); users see GRAPHEMES. Normalize
// (NFC) and fold at the boundary before inserting and before querying (needs the `unicode-normalization` crate; std alone cannot).
// Rust wrinkle: `&s[..n]` panics off a char boundary; slicing by byte index is only safe where `is_char_boundary(n)`. Iterate `chars()`
// or `char_indices()` rather than `0..s.len()` when words may be non-ASCII. `s.len()` is bytes, `s.chars().count()` is scalars.
// Interviewer is testing: knowing `len()` is not the character count, and choosing the key unit deliberately.
// Mistakes: indexing `s.chars().nth(i)` in a loop (O(n^2)); `as u8` on a char; assuming one char equals one user-visible character.
// Follow-ups: grapheme clusters (unicode-segmentation); collation-aware sorting (ICU); byte trie for arbitrary binary keys.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct ByteTrie {
    kids: BTreeMap<u8, ByteTrie>,
    end: bool,
    pass: usize,
}
impl ByteTrie {
    fn find(&self, w: &str) -> Option<&ByteTrie> {
        let mut cur = self;
        for b in w.bytes() {
            cur = cur.kids.get(&b)?;
        }
        Some(cur)
    }
    fn nodes(&self) -> usize {
        1 + self.kids.values().map(|k| k.nodes()).sum::<usize>()
    }
}
impl PrefixSet for ByteTrie {
    fn ins(&mut self, w: &str) -> bool {
        if self.has(w) {
            return false;
        }
        let mut cur = &mut *self;
        cur.pass += 1;
        for b in w.bytes() {
            cur = cur.kids.entry(b).or_default();
            cur.pass += 1;
        }
        cur.end = true;
        true
    }
    fn has(&self, w: &str) -> bool {
        self.find(w).is_some_and(|n| n.end)
    }
    fn pre(&self, p: &str) -> bool {
        self.find(p).is_some()
    }
    fn cnt(&self, p: &str) -> usize {
        self.find(p).map_or(0, |n| n.pass)
    }
}
fn q17() {
    let alpha = ['a', 'é', 'ß', '日', '🦀'];
    let mk = |r: &mut Rng| -> String {
        (0..1 + r.range(4))
            .map(|_| alpha[r.range(alpha.len())])
            .collect()
    };
    check_prefix_set::<ByteTrie>(0x1717_0000_AAAA_0001, &mk);
    check_prefix_set::<MapTrie>(0x1717_0000_AAAA_0001, &mk); // same seed: both agree with the oracle on the same inputs
    let (mut bt, mut ct) = (ByteTrie::default(), MapTrie::default());
    for w in ["日本", "日本語", "🦀", "é"] {
        bt.ins(w);
        ct.insert(w);
    }
    assert!(
        bt.nodes() > ct.nodes(),
        "byte trie spends 3 nodes per CJK char, 4 per emoji"
    );
    println!(
        "    Q17: nodes for 日本/日本語/🦀/é: byte trie {}, char trie {}",
        bt.nodes(),
        ct.nodes()
    );
    assert_eq!("日本語".len(), 9);
    assert_eq!("日本語".chars().count(), 3);
    assert!(!"日本語".is_char_boundary(1) && "日本語".is_char_boundary(3));
    // normalization: two spellings of "é" are different keys; case folding changes length
    let (nfc, nfd) = ("\u{e9}", "e\u{301}");
    assert_ne!(nfc, nfd);
    assert!(bt.has(nfc) && !bt.has(nfd));
    assert_eq!("İ".to_lowercase().chars().count(), 2);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q18. Longest-prefix match (IPv4-style routing) with a binary trie.
// Approach: one level per BIT. Insert (prefix, len, next hop) walks `len` bits from the most significant end and stores the hop at that node.
// Lookup walks all 32 bits of the address, remembering the last hop seen: the deepest stored prefix on the path is the longest match.
// O(32) per op regardless of table size. Production routers use multibit/stride tries or Patricia tries to cut memory accesses.
// Rust wrinkle: arena `Vec<Node>` with `[u32; 2]` children (0 = none) again; mask the prefix with a `checked_shl`-safe expression because
// `!0u32 << 32` is an overflow (panic in debug): handle len == 0 explicitly.
// Interviewer is testing: reading bits MSB-first, "remember the last match" while descending, the /0 default route and overwrite semantics.
// Mistakes: stopping at the first match (shortest prefix); shifting by 32; not masking host bits of the stored prefix.
// Follow-ups: delete a route; IPv6 (128-bit, use u128); compress with path compression (Q15); concurrent readers with RCU/arc-swap.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default, Clone, Copy)]
struct LNode {
    kids: [u32; 2],
    hop: Option<u32>,
}
struct Lpm {
    nodes: Vec<LNode>,
}
fn mask(prefix: u32, len: u8) -> u32 {
    if len == 0 {
        0
    } else {
        prefix & (!0u32 << (32 - len as u32))
    }
}
impl Lpm {
    fn new() -> Self {
        Lpm {
            nodes: vec![LNode::default()],
        }
    }
    fn insert(&mut self, prefix: u32, len: u8, hop: u32) {
        let mut cur = 0usize;
        for i in 0..len as u32 {
            let bit = ((prefix >> (31 - i)) & 1) as usize;
            if self.nodes[cur].kids[bit] == 0 {
                self.nodes.push(LNode::default());
                let id = (self.nodes.len() - 1) as u32;
                self.nodes[cur].kids[bit] = id;
            }
            cur = self.nodes[cur].kids[bit] as usize;
        }
        self.nodes[cur].hop = Some(hop);
    }
    fn lookup(&self, addr: u32) -> Option<u32> {
        let (mut cur, mut best) = (0usize, self.nodes[0].hop);
        for i in 0..32 {
            match self.nodes[cur].kids[((addr >> (31 - i)) & 1) as usize] {
                0 => break,
                n => cur = n as usize,
            }
            best = self.nodes[cur].hop.or(best);
        }
        best
    }
}
fn q18() {
    let mut t = Lpm::new();
    t.insert(0, 0, 1); // default route
    t.insert(0x0A00_0000, 8, 2); // 10.0.0.0/8
    t.insert(0x0A01_0000, 16, 3); // 10.1.0.0/16
    assert_eq!(t.lookup(0x0A01_0203), Some(3));
    assert_eq!(t.lookup(0x0A02_0203), Some(2));
    assert_eq!(t.lookup(0xC0A8_0001), Some(1));
    assert_eq!(Lpm::new().lookup(5), None);
    let mut r = Rng(0x1818_1818_7777_9999);
    let gen_addr = |r: &mut Rng| {
        ((r.range(8) as u32) << 29) | ((r.range(8) as u32) << 26) | (r.next() as u32 & 0x03FF_FFFF)
    };
    for _ in 0..2000 {
        let (mut t, mut table) = (Lpm::new(), BTreeMap::<(u32, u8), u32>::new());
        for _ in 0..1 + r.range(10) {
            let (len, p) = (r.range(7) as u8, gen_addr(&mut r));
            let hop = r.range(100) as u32;
            t.insert(p, len, hop);
            table.insert((mask(p, len), len), hop);
        }
        for _ in 0..20 {
            let a = gen_addr(&mut r);
            let want = table
                .iter()
                .filter(|((p, l), _)| mask(a, *l) == *p)
                .max_by_key(|((_, l), _)| *l)
                .map(|(_, &h)| h);
            assert_eq!(t.lookup(a), want, "addr {a:#x}");
        }
    }
}

fn main() {
    q1();
    println!("Q1: ok");
    q2();
    println!("Q2: ok");
    q3();
    println!("Q3: ok");
    q4();
    println!("Q4: ok");
    q5();
    println!("Q5: ok");
    q6();
    println!("Q6: ok");
    q7();
    println!("Q7: ok");
    q8();
    println!("Q8: ok");
    q9();
    println!("Q9: ok");
    q10();
    println!("Q10: ok");
    q11();
    println!("Q11: ok");
    q12();
    println!("Q12: ok");
    q13();
    println!("Q13: ok");
    q14();
    println!("Q14: ok");
    q15();
    println!("Q15: ok");
    q16();
    println!("Q16: ok");
    q17();
    println!("Q17: ok");
    q18();
    println!("Q18: ok");
}
