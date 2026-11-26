//! RECIPE 631 (interview): advanced trie interview questions in Rust: Word Search II, XOR tries, Aho-Corasick streams, prefix+suffix search, palindrome pairs, word squares.
//! LEVEL: advanced
//! Use in: coding interviews (LeetCode 212, 421, 1707, 1803, 1032, 745, 472, 642, 588, 336, 425) and system-design follow-ups (autocomplete, filesystems, multi-pattern scanning).   Run: cargo run --release --example tries_advanced_questions
//! COMMON USES: boggle/crossword solvers, XOR maximization, log/keyword scanning (Aho-Corasick), autocomplete state machines, path trees, palindromic word pairs.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233474786/word-search-ii-trie-pattern-recognition/ , https://www.techinterview.org/post/3233463531/trie-interview-patterns-autocomplete/ ,
//! https://rust-unofficial.github.io/too-many-lists/
//! See also: examples/interview/tries_core_questions.rs (the basics: three trie layouts, wildcards, top-k autocomplete, radix tries)
//!
//! QUESTION BANK
//! Q1. Word Search II: find all dictionary words in a letter grid (adjacent cells, no cell reused). Trie + DFS + pruning exhausted branches (LeetCode 212). [Hard] [asked: very common]
//! Q2. Maximum XOR of two numbers in an array with a binary trie (LeetCode 421). [Medium] [asked: very common]
//! Q3. Maximum XOR with an element not exceeding m, for many queries (offline sort + binary trie, LeetCode 1707). [Hard] [asked: common]
//! Q4. Maximum XOR of any non-empty subarray (prefix XORs + binary trie). [Medium] [asked: common]
//! Q5. Count pairs whose XOR lies in [low, high] (binary trie with subtree counts, LeetCode 1803). [Hard] [asked: occasional]
//! Q6. Stream of Characters: after each letter, does some word end the stream? Reversed-word trie (LeetCode 1032). [Hard] [asked: common]
//! Q7. Aho-Corasick: find every occurrence of many patterns in a text in one pass (failure links). [Hard] [asked: common]
//! Q8. Prefix and Suffix Search: the largest index of a word with a given prefix and suffix (LeetCode 745). [Hard] [asked: common]
//! Q9. Concatenated Words: which words are built from at least two other words in the list (LeetCode 472). [Hard] [asked: common]
//! Q10. Design Search Autocomplete System with an input(c) state machine and an end marker '#' (LeetCode 642). [Hard] [asked: very common]
//! Q11. Design an in-memory file system with mkdir, ls, addContentToFile, readContentFromFile (LeetCode 588, a path trie). [Hard] [asked: very common]
//! Q12. Palindrome Pairs: all pairs of words whose concatenation is a palindrome (trie of reversed words, LeetCode 336). [Hard] [asked: common]
//! Q13. Shortest unique prefix for each word (pass counts in the trie). [Medium] [asked: common]
//! Q14. Word Squares: all squares where the k-th row equals the k-th column (backtracking with prefix lookups, LeetCode 425). [Hard] [asked: occasional]
//! Q15. Put an LRU cache of prefix results in front of a trie, and invalidate it correctly on insert. [Medium] [asked: common]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::time::Instant;

// ---------------------------------------------------------------------------------------------------------------------
// Shared plumbing: seeded xorshift, random words, letter index.
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
fn gen_word(r: &mut Rng, max_len: usize, alpha: usize) -> String {
    let n = 1 + r.range(max_len);
    (0..n)
        .map(|_| (b'a' + r.range(alpha) as u8) as char)
        .collect()
}
fn ix(b: u8) -> usize {
    (b - b'a') as usize
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Word Search II (LeetCode 212): given a grid of letters and a word list, return every word that can be traced through
// horizontally/vertically adjacent cells without reusing a cell.
// Approach (naive): run a Word Search I DFS for every word from every cell: O(W * cells * 3^L). Approach (trie): insert all words once, then
// DFS from each cell WALKING THE TRIE: one DFS serves every word that shares the path, and a branch dies as soon as no word continues it.
// The key optimization interviewers wait for is PRUNING: when a node has no word and no children left, delete it from its parent so later
// DFS calls stop at the parent. Time O(cells * 3^L) worst case but far less in practice; space O(total word chars).
// Rust wrinkle: the DFS holds `&mut` to the current trie node and to the board. Take the child with `node.kids[i].as_mut()` (a `let ... else`
// return when missing), `take()` the stored word so it is reported once (no HashSet needed), mark the cell visited by overwriting it with '#'
// and RESTORE it after (no separate visited array), and prune with `node.kids[i] = None` AFTER the last use of `child`: NLL ends the borrow there.
// Interviewer is testing: whether you move from per-word DFS to a trie, restore state on backtrack, dedupe results, and prune.
// Mistakes: forgetting to restore the cell; reporting a word twice (found via two paths); not pruning (TLE on dense boards); allocating a new
// String at every step instead of storing the word at its end node.
// Follow-ups: diagonal moves; return the paths; boards with 10^4 words (iterative DFS to avoid deep recursion); Boggle scoring.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct WNode {
    kids: [Option<Box<WNode>>; 26],
    word: Option<String>,
}
impl WNode {
    fn is_leaf(&self) -> bool {
        self.word.is_none() && self.kids.iter().all(|k| k.is_none())
    }
}
const DIRS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
fn neighbours(b: &[Vec<u8>], r: usize, c: usize) -> impl Iterator<Item = (usize, usize)> {
    let (h, w) = (b.len() as i32, b[0].len() as i32);
    DIRS.iter()
        .map(move |&(dr, dc)| (r as i32 + dr, c as i32 + dc))
        .filter(move |&(nr, nc)| nr >= 0 && nc >= 0 && nr < h && nc < w)
        .map(|(nr, nc)| (nr as usize, nc as usize))
}
fn find_words(board: &[Vec<u8>], words: &[String], calls: &mut u64) -> Vec<String> {
    let mut root = WNode::default();
    for w in words {
        let mut cur = &mut root;
        for b in w.bytes() {
            cur = cur.kids[ix(b)].get_or_insert_with(Default::default);
        }
        cur.word = Some(w.clone());
    }
    fn dfs(
        b: &mut Vec<Vec<u8>>,
        r: usize,
        c: usize,
        node: &mut WNode,
        out: &mut Vec<String>,
        calls: &mut u64,
    ) {
        *calls += 1;
        let ch = b[r][c];
        if ch == b'#' {
            return;
        }
        let i = ix(ch);
        let Some(child) = node.kids[i].as_mut() else {
            return;
        };
        if let Some(w) = child.word.take() {
            out.push(w); // take(): this word can never be reported again
        }
        b[r][c] = b'#';
        let ns: Vec<_> = neighbours(b, r, c).collect();
        for (nr, nc) in ns {
            dfs(b, nr, nc, child, out, calls);
        }
        b[r][c] = ch;
        if child.is_leaf() {
            node.kids[i] = None; // prune the exhausted branch
        }
    }
    let (mut out, mut b) = (Vec::new(), board.to_vec());
    if !b.is_empty() {
        for r in 0..b.len() {
            for c in 0..b[0].len() {
                dfs(&mut b, r, c, &mut root, &mut out, calls);
            }
        }
    }
    out.sort();
    out
}
fn find_words_naive(board: &[Vec<u8>], words: &[String], calls: &mut u64) -> Vec<String> {
    fn exists(b: &mut Vec<Vec<u8>>, r: usize, c: usize, w: &[u8], calls: &mut u64) -> bool {
        *calls += 1;
        if b[r][c] != w[0] {
            return false;
        }
        if w.len() == 1 {
            return true;
        }
        let ch = b[r][c];
        b[r][c] = b'#';
        let ns: Vec<_> = neighbours(b, r, c).collect();
        let found = ns
            .into_iter()
            .any(|(nr, nc)| exists(b, nr, nc, &w[1..], calls));
        b[r][c] = ch;
        found
    }
    let mut b = board.to_vec();
    let mut out: Vec<String> = Vec::new();
    if !b.is_empty() {
        for w in words {
            if (0..b.len())
                .any(|r| (0..b[0].len()).any(|c| exists(&mut b, r, c, w.as_bytes(), calls)))
            {
                out.push(w.clone());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}
fn rand_board(r: &mut Rng, h: usize, w: usize, alpha: usize) -> Vec<Vec<u8>> {
    (0..h)
        .map(|_| (0..w).map(|_| b'a' + r.range(alpha) as u8).collect())
        .collect()
}
fn q1() {
    let board: Vec<Vec<u8>> = ["oaan", "etae", "ihkr", "iflv"]
        .iter()
        .map(|s| s.bytes().collect())
        .collect();
    let words: Vec<String> = ["oath", "pea", "eat", "rain"].map(String::from).to_vec();
    let mut calls = 0;
    assert_eq!(find_words(&board, &words, &mut calls), vec!["eat", "oath"]);
    assert!(find_words(&[], &words, &mut calls).is_empty());
    let mut r = Rng(0x2121_2121_ABCD_0001);
    for _ in 0..2000 {
        let (h, w) = (1 + r.range(4), 1 + r.range(4));
        let board = rand_board(&mut r, h, w, 3);
        let set: BTreeSet<String> = (0..1 + r.range(10))
            .map(|_| gen_word(&mut r, 6, 3))
            .collect();
        let words: Vec<String> = set.into_iter().collect();
        let (mut c1, mut c2) = (0, 0);
        assert_eq!(
            find_words(&board, &words, &mut c1),
            find_words_naive(&board, &words, &mut c2),
            "{board:?} {words:?}"
        );
    }
    // speed: 12x12 board over 4 letters, 400 words
    let big = rand_board(&mut r, 12, 12, 4);
    let set: BTreeSet<String> = (0..400)
        .map(|_| {
            let n = 4 + r.range(5);
            (0..n)
                .map(|_| (b'a' + r.range(4) as u8) as char)
                .collect::<String>()
        })
        .collect();
    let words: Vec<String> = set.into_iter().collect();
    let (mut c_trie, mut c_naive) = (0u64, 0u64);
    let t0 = Instant::now();
    let a = find_words(&big, &words, &mut c_trie);
    let t_trie = t0.elapsed();
    let t0 = Instant::now();
    let b = find_words_naive(&big, &words, &mut c_naive);
    let t_naive = t0.elapsed();
    assert_eq!(a, b);
    println!(
        "    Q1: {} words found; DFS calls trie {c_trie} vs per-word {c_naive} ({:.1}x fewer); time {t_trie:?} vs {t_naive:?}",
        a.len(),
        c_naive as f64 / c_trie as f64
    );
    assert!(c_trie < c_naive);
}

// ---------------------------------------------------------------------------------------------------------------------
// Binary trie shared by Q2-Q5: fixed-width numbers, one level per bit from the most significant, children `[u32; 2]` (0 = none), and a
// pass count per node so Q5 can count how many stored numbers go below a node.
// ---------------------------------------------------------------------------------------------------------------------
struct BitTrie {
    ch: Vec<[u32; 2]>,
    cnt: Vec<u32>,
    bits: u32,
}
impl BitTrie {
    fn new(bits: u32) -> Self {
        BitTrie {
            ch: vec![[0; 2]],
            cnt: vec![0],
            bits,
        }
    }
    fn insert(&mut self, x: u32) {
        let mut cur = 0usize;
        self.cnt[0] += 1;
        for b in (0..self.bits).rev() {
            let bit = ((x >> b) & 1) as usize;
            if self.ch[cur][bit] == 0 {
                self.ch.push([0; 2]);
                self.cnt.push(0);
                let id = (self.ch.len() - 1) as u32;
                self.ch[cur][bit] = id;
            }
            cur = self.ch[cur][bit] as usize;
            self.cnt[cur] += 1;
        }
    }
    /// max over stored `a` of `x ^ a`: at each bit prefer the child whose bit is the OPPOSITE of x's bit. None if empty.
    fn max_xor(&self, x: u32) -> Option<u32> {
        if self.cnt[0] == 0 {
            return None;
        }
        let (mut cur, mut res) = (0usize, 0u32);
        for b in (0..self.bits).rev() {
            let bit = ((x >> b) & 1) as usize;
            if self.ch[cur][bit ^ 1] != 0 {
                res |= 1 << b;
                cur = self.ch[cur][bit ^ 1] as usize;
            } else {
                cur = self.ch[cur][bit] as usize; // must exist: the trie is non-empty and every inner node has a child
            }
        }
        Some(res)
    }
    /// number of stored `a` with `x ^ a < k`.
    fn count_xor_less(&self, x: u32, k: u32) -> u64 {
        if k >> self.bits != 0 {
            return self.cnt[0] as u64; // k exceeds every possible xor
        }
        let (mut cur, mut total) = (0usize, 0u64);
        for b in (0..self.bits).rev() {
            let (xb, kb) = (((x >> b) & 1) as usize, ((k >> b) & 1) as usize);
            if kb == 1 {
                let same = self.ch[cur][xb]; // xor bit would be 0 < 1: the whole subtree is smaller
                if same != 0 {
                    total += self.cnt[same as usize] as u64;
                }
                cur = self.ch[cur][xb ^ 1] as usize; // xor bit 1 == k's bit: keep matching
            } else {
                cur = self.ch[cur][xb] as usize; // xor bit must be 0 to stay <= k's prefix
            }
            if cur == 0 {
                break;
            }
        }
        total // equal-to-k numbers are not counted: strictly less
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. Maximum XOR of two numbers (LeetCode 421).
// Approach A (brute): all pairs, O(n^2). Approach B (binary trie): insert every number's 31 bits; for each number walk greedily choosing the
// opposite bit when it exists (the higher bit dominates everything below it, so greedy is optimal). O(31 n) time, O(31 n) space.
// Approach C (hash set of prefixes): for bit from high to low, grow the answer mask and check whether two prefixes XOR to the candidate. Same O(31 n).
// Rust wrinkle: `[u32; 2]` children in a `Vec` (arena) means no Box and no Option; the greedy step reads the Copy index first, then moves.
// Use `u32` (or `u64`) explicitly: shifting `1i32 << 31` overflows into the sign bit.
// Interviewer is testing: why greedy per bit is correct, bit width choice, handling a single element (answer 0) and equal numbers.
// Mistakes: iterating bits LSB-first; walking only into existing branches of the same bit (the max needs the opposite); width too small.
// Follow-ups: online insertion (the trie already supports it); values up to 10^18 (use 60 bits); minimum XOR pair (sort + adjacent).
// ---------------------------------------------------------------------------------------------------------------------
fn max_xor_pair(nums: &[u32]) -> u32 {
    let mut t = BitTrie::new(31);
    nums.iter().for_each(|&x| t.insert(x));
    nums.iter().filter_map(|&x| t.max_xor(x)).max().unwrap_or(0)
}
fn q2() {
    assert_eq!(max_xor_pair(&[3, 10, 5, 25, 2, 8]), 28);
    assert_eq!(
        max_xor_pair(&[14, 70, 53, 83, 49, 91, 36, 80, 92, 51, 66, 70]),
        127
    );
    assert_eq!(max_xor_pair(&[7]), 0);
    assert_eq!(max_xor_pair(&[]), 0);
    let mut r = Rng(0x2222_FFFF_0000_1111);
    for _ in 0..2000 {
        let hi = [4usize, 64, 1 << 30][r.range(3)];
        let nums: Vec<u32> = (0..r.range(15)).map(|_| r.range(hi) as u32).collect();
        let want = nums
            .iter()
            .flat_map(|&a| nums.iter().map(move |&b| a ^ b))
            .max()
            .unwrap_or(0);
        assert_eq!(max_xor_pair(&nums), want, "{nums:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. Maximum XOR With an Element From Array (LeetCode 1707): for each query (x, m) find max(x ^ a) over array elements a <= m, or -1.
// Approach: OFFLINE. Sort the array; sort the queries by m (keeping original positions); sweep, inserting every array element <= m into the
// trie before answering the query. Each element inserts once: O((n + q) * 31 + sort). Online alternative: store the MIN value under each node and
// refuse to descend into a child whose min > m (no sorting, queries in any order), at the cost of one extra u32 per node.
// Rust wrinkle: `sort_unstable_by_key` on an index vector, then write answers back by original index; a two-pointer over the sorted array.
// Interviewer is testing: spotting that a constraint on `a` (a <= m) is handled by processing queries in a good ORDER, not by rebuilding.
// Mistakes: answering in input order (needs rebuild per query); returning 0 instead of -1 for "no element <= m"; losing the original order.
// Follow-ups: online version with per-node min; also support deletes (counts); range-of-indices constraints (persistent trie).
// ---------------------------------------------------------------------------------------------------------------------
fn max_xor_queries(nums: &[u32], queries: &[(u32, u32)]) -> Vec<i64> {
    let mut sorted = nums.to_vec();
    sorted.sort_unstable();
    let mut order: Vec<usize> = (0..queries.len()).collect();
    order.sort_unstable_by_key(|&i| queries[i].1);
    let (mut t, mut p, mut ans) = (BitTrie::new(31), 0, vec![-1i64; queries.len()]);
    for qi in order {
        let (x, m) = queries[qi];
        while p < sorted.len() && sorted[p] <= m {
            t.insert(sorted[p]);
            p += 1;
        }
        ans[qi] = t.max_xor(x).map_or(-1, |v| v as i64);
    }
    ans
}
fn q3() {
    assert_eq!(
        max_xor_queries(&[0, 1, 2, 3, 4], &[(3, 1), (1, 3), (5, 6)]),
        vec![3, 3, 7]
    );
    assert_eq!(
        max_xor_queries(&[5, 2, 4, 6, 6, 3], &[(12, 4), (8, 1), (6, 3)]),
        vec![15, -1, 5]
    );
    let mut r = Rng(0x3333_0000_AAAA_BBBB);
    for _ in 0..2000 {
        let nums: Vec<u32> = (0..r.range(10)).map(|_| r.range(64) as u32).collect();
        let qs: Vec<(u32, u32)> = (0..1 + r.range(8))
            .map(|_| (r.range(64) as u32, r.range(70) as u32))
            .collect();
        let want: Vec<i64> = qs
            .iter()
            .map(|&(x, m)| {
                nums.iter()
                    .filter(|&&a| a <= m)
                    .map(|&a| (x ^ a) as i64)
                    .max()
                    .unwrap_or(-1)
            })
            .collect();
        assert_eq!(max_xor_queries(&nums, &qs), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. Maximum XOR of any non-empty subarray.
// Approach: with prefix XORs P[0] = 0, P[i] = a[0] ^ ... ^ a[i-1], the XOR of a[l..r] is P[l] ^ P[r]. So the answer is the max XOR of two
// prefix values with l < r: Q2 again. Insert P[0], then for each i query P[i] BEFORE inserting it (so l < r and the subarray is non-empty).
// O(31 n) vs O(n^2) brute force. The trick generalizes to every "XOR of a range" question.
// Rust wrinkle: running prefix kept in one `u32`; no need to materialize the prefix array.
// Interviewer is testing: recognizing the prefix-XOR identity (x ^ x = 0 cancels the shared part) and reducing to the known problem.
// Mistakes: forgetting to insert P[0] = 0 (misses subarrays starting at index 0); querying after inserting (allows the empty subarray).
// Follow-ups: print the subarray bounds (store the index in the leaf); subarrays with XOR exactly k (HashMap of prefix counts, no trie needed).
// ---------------------------------------------------------------------------------------------------------------------
fn max_xor_subarray(a: &[u32]) -> u32 {
    let mut t = BitTrie::new(31);
    t.insert(0);
    let (mut pref, mut best) = (0u32, 0u32);
    for &x in a {
        pref ^= x;
        best = best.max(t.max_xor(pref).unwrap());
        t.insert(pref);
    }
    best
}
fn q4() {
    assert_eq!(max_xor_subarray(&[8, 1, 2, 12]), 15); // 1^2^12 = 15
    assert_eq!(max_xor_subarray(&[4, 6]), 6);
    assert_eq!(max_xor_subarray(&[]), 0);
    let mut r = Rng(0x4444_ABCD_0000_5555);
    for _ in 0..2000 {
        let a: Vec<u32> = (0..r.range(14)).map(|_| r.range(1 << 12) as u32).collect();
        let mut want = 0;
        for l in 0..a.len() {
            let mut x = 0;
            for &v in &a[l..] {
                x ^= v;
                want = want.max(x);
            }
        }
        assert_eq!(max_xor_subarray(&a), want, "{a:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. Count pairs (i < j) whose XOR is in [low, high] (LeetCode 1803).
// Approach: count(high) - count(low - 1) is the standard "prefix of a range" trick; here f(k) = #pairs with xor < k, so the answer is
// f(high + 1) - f(low). f(k) per element x walks the trie by bits of k: wherever k has a 1, every number whose xor bit is 0 there is smaller
// (add its subtree COUNT), then follow the child that makes the xor bit equal to k's bit. O(bits) per query, O(n * bits) total.
// Rust wrinkle: widths: values < 2^14 and high up to 2^15 needs a trie one bit wider than the values, or the early-exit guard `k >> bits != 0`
// (k bigger than any possible XOR) which `count_xor_less` includes. Use u64 for the counts: n^2/2 pairs overflow u32.
// Interviewer is testing: turning a range into two "less than" counts, subtree counts as an aggregate, and the strict/inclusive boundary.
// Mistakes: counting equal-to-k numbers as smaller; counting j < i pairs twice (insert AFTER querying); u32 total overflow.
// Follow-ups: pairs with xor == k (HashMap); k-th smallest XOR pair (binary search on the answer + this count); persistent trie for subarray ranges.
// ---------------------------------------------------------------------------------------------------------------------
fn count_pairs_xor_in_range(a: &[u32], low: u32, high: u32) -> u64 {
    let mut t = BitTrie::new(16);
    let mut total = 0u64;
    for &x in a {
        total += t.count_xor_less(x, high + 1) - t.count_xor_less(x, low);
        t.insert(x);
    }
    total
}
fn q5() {
    assert_eq!(count_pairs_xor_in_range(&[1, 4, 2, 7], 2, 6), 6);
    assert_eq!(count_pairs_xor_in_range(&[9, 8, 4, 2, 1], 5, 14), 8);
    let mut r = Rng(0x5555_0000_C0DE_0005);
    for _ in 0..2000 {
        let a: Vec<u32> = (0..r.range(14)).map(|_| r.range(64) as u32).collect();
        let (low, high) = {
            let l = r.range(70) as u32;
            (l, l + r.range(40) as u32)
        };
        let mut want = 0u64;
        for i in 0..a.len() {
            for j in i + 1..a.len() {
                if (low..=high).contains(&(a[i] ^ a[j])) {
                    want += 1;
                }
            }
        }
        assert_eq!(
            count_pairs_xor_in_range(&a, low, high),
            want,
            "{a:?} [{low},{high}]"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Stream of Characters (LeetCode 1032): `query(c)` appends a letter to a stream and answers whether SOME word is a suffix of the stream so far.
// Approach A (reversed trie): store each word REVERSED; on every query walk the trie from the newest letter backwards through the stream;
// stop at a missing edge (so at most max_word_len steps) and answer true at any end marker. O(max_len) per query. Keep only the last max_len
// letters (a `VecDeque`) in production; the unbounded `Vec` here is for brevity.
// Approach B (Aho-Corasick automaton, see Q7): a single state advances one transition per letter: O(1) per query, no history needed.
// Rust wrinkle: iterate `hist.iter().rev()` and walk an arena `Vec<[u32; 26]>` (0 = none), exiting early with `return`; the AC version
// stores only a `usize` state, which is why it is the "stream" answer: no history, no lifetimes.
// Interviewer is testing: the REVERSE insight (suffix matching of a stream becomes prefix matching read backwards) and knowing AC exists.
// Mistakes: building a forward trie and scanning from the start of the stream (O(n) per query); not breaking on a missing child.
// Follow-ups: memory-bounded history; report WHICH word matched; unbounded alphabets (map children); thousands of patterns (AC).
// ---------------------------------------------------------------------------------------------------------------------
struct RevStream {
    go: Vec<[u32; 26]>,
    end: Vec<bool>,
    hist: Vec<u8>,
}
impl RevStream {
    fn new(words: &[String]) -> Self {
        let (mut go, mut end) = (vec![[0u32; 26]], vec![false]);
        for w in words {
            let mut cur = 0usize;
            for b in w.bytes().rev() {
                if go[cur][ix(b)] == 0 {
                    go.push([0; 26]);
                    end.push(false);
                    let id = (go.len() - 1) as u32;
                    go[cur][ix(b)] = id;
                }
                cur = go[cur][ix(b)] as usize;
            }
            end[cur] = true;
        }
        RevStream {
            go,
            end,
            hist: Vec::new(),
        }
    }
    fn query(&mut self, c: u8) -> bool {
        self.hist.push(c);
        let mut cur = 0usize;
        for &b in self.hist.iter().rev() {
            match self.go[cur][ix(b)] {
                0 => return false,
                n => cur = n as usize,
            }
            if self.end[cur] {
                return true;
            }
        }
        false
    }
}

/// Aho-Corasick automaton (also used by Q7): trie + failure links, turned into a full DFA so a step is one table lookup.
struct Aho {
    go: Vec<[u32; 26]>,
    fail: Vec<u32>,
    out: Vec<Vec<usize>>, // pattern ids ending exactly at this node
    link: Vec<u32>, // nearest proper fail-ancestor that has output (0 = none): "dictionary suffix link"
    any: Vec<bool>, // some pattern ends at this node or on its fail chain
}
impl Aho {
    fn new(pats: &[String]) -> Self {
        let (mut go, mut out) = (vec![[0u32; 26]], vec![Vec::new()]);
        for (id, p) in pats.iter().enumerate() {
            let mut cur = 0usize;
            for b in p.bytes() {
                let c = ix(b);
                if go[cur][c] == 0 {
                    go.push([0; 26]);
                    out.push(Vec::new());
                    let n = (go.len() - 1) as u32;
                    go[cur][c] = n;
                }
                cur = go[cur][c] as usize;
            }
            out[cur].push(id);
        }
        let n = go.len();
        let (mut fail, mut link, mut any) = (vec![0u32; n], vec![0u32; n], vec![false; n]);
        let mut q = VecDeque::new();
        for c in 0..26 {
            let v = go[0][c] as usize;
            if v != 0 {
                any[v] = !out[v].is_empty();
                q.push_back(v);
            }
        }
        while let Some(u) = q.pop_front() {
            for c in 0..26 {
                let v = go[u][c] as usize;
                let f = go[fail[u] as usize][c]; // fail[u] is shallower: its row is already a complete DFA row
                if v != 0 {
                    fail[v] = f;
                    let fv = f as usize;
                    link[v] = if out[fv].is_empty() { link[fv] } else { f };
                    any[v] = !out[v].is_empty() || any[fv];
                    q.push_back(v);
                } else {
                    go[u][c] = f; // missing edge: behave like the failure state
                }
            }
        }
        Aho {
            go,
            fail,
            out,
            link,
            any,
        }
    }
    fn step(&self, state: usize, b: u8) -> usize {
        self.go[state][ix(b)] as usize
    }
    /// All (end index, pattern id) occurrences, sorted.
    fn find_all(&self, text: &str) -> Vec<(usize, usize)> {
        let (mut st, mut res) = (0usize, Vec::new());
        for (i, b) in text.bytes().enumerate() {
            st = self.step(st, b);
            let mut s = if self.out[st].is_empty() {
                self.link[st] as usize
            } else {
                st
            };
            while s != 0 {
                res.extend(self.out[s].iter().map(|&id| (i, id)));
                s = self.link[s] as usize;
            }
        }
        res.sort();
        res
    }
}
fn q6() {
    let words: Vec<String> = ["cd", "f", "kl"].map(String::from).to_vec();
    let mut s = RevStream::new(&words);
    let got: Vec<bool> = "abcdefghijkl".bytes().map(|c| s.query(c)).collect();
    assert_eq!(
        got,
        vec![
            false, false, false, true, false, true, false, false, false, false, false, true
        ]
    );
    let mut r = Rng(0x6666_0606_0606_6666);
    for _ in 0..2000 {
        let words: Vec<String> = (0..1 + r.range(5))
            .map(|_| gen_word(&mut r, 4, 3))
            .collect();
        let (mut rs, aho, mut state, mut stream) = (
            RevStream::new(&words),
            Aho::new(&words),
            0usize,
            String::new(),
        );
        for _ in 0..30 {
            let c = b'a' + r.range(3) as u8;
            stream.push(c as char);
            let want = words.iter().any(|w| stream.ends_with(w.as_str()));
            assert_eq!(rs.query(c), want, "reversed trie {stream}");
            state = aho.step(state, c);
            assert_eq!(aho.any[state], want, "aho-corasick {stream}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. Aho-Corasick: report every occurrence of every pattern in a text in a single pass.
// Approach: build the trie of patterns; BFS to compute a FAILURE link for each node (longest proper suffix of its string that is also a trie
// prefix) and fold it into a complete DFA (`go[u][c] = go[fail[u]][c]` when the edge is missing). Scanning is then one lookup per text byte,
// and matches are reported by following DICTIONARY-SUFFIX links (nodes with output on the fail chain), so work is O(text + matches).
// Build O(total pattern chars * alphabet). Versus naive search of every pattern: O(text * patterns * len).
// Rust wrinkle: the arena (parallel `Vec`s indexed by node id, `u32` links) sidesteps the cyclic-graph problem: failure links form cycles/DAG
// edges that `Box` or `&` cannot express, and `Rc<RefCell>` would be noise. Process BFS order so `go[fail[u]]` is final before it is read.
// Interviewer is testing: explaining WHY the failure link of a child is `go[fail[parent]][c]`, the dictionary-link trick, and complexity.
// Mistakes: forgetting dictionary links (misses patterns that are suffixes of longer ones, like "he" inside "she"); building the DFA in DFS order.
// Follow-ups: overlapping vs leftmost-longest matches; case-insensitive matching; Unicode (byte automaton); the `aho-corasick` crate in production.
// ---------------------------------------------------------------------------------------------------------------------
fn naive_find_all(text: &str, pats: &[String]) -> Vec<(usize, usize)> {
    let mut res = Vec::new();
    for (id, p) in pats.iter().enumerate() {
        for end in p.len()..=text.len() {
            if text[..end].ends_with(p.as_str()) {
                res.push((end - 1, id));
            }
        }
    }
    res.sort();
    res
}
fn q7() {
    let pats: Vec<String> = ["he", "she", "his", "hers"].map(String::from).to_vec();
    let aho = Aho::new(&pats);
    // "ushers": she@3, he@3, hers@5 (end indices)
    assert_eq!(aho.find_all("ushers"), vec![(3, 0), (3, 1), (5, 3)]);
    let mut r = Rng(0x7777_AC0C_0000_0007);
    for _ in 0..2000 {
        let pats: Vec<String> = (0..1 + r.range(6))
            .map(|_| {
                let a = 2 + r.range(2);
                gen_word(&mut r, 4, a)
            })
            .collect(); // duplicates and nested patterns happen
        let text: String = (0..r.range(25))
            .map(|_| (b'a' + r.range(3) as u8) as char)
            .collect();
        assert_eq!(
            Aho::new(&pats).find_all(&text),
            naive_find_all(&text, &pats),
            "{pats:?} in {text}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. Prefix and Suffix Search (LeetCode 745): WordFilter(words); f(prefix, suffix) returns the LARGEST index of a word having both, or -1.
// Approach A (brute): scan words from the end: O(n * L) per query. Approach B (suffix{word trick): for every word insert ALL strings
// `suffix + '{' + word` into one trie ('{' is the char after 'z', a separator) and store the word's index in every node on the path (later
// indices overwrite, giving the max). A query walks `suffix + '{' + prefix`. Insert O(L^2) per word, query O(|prefix| + |suffix|).
// Alternative: two tries (prefix trie, reversed-suffix trie) with index sets intersected: more memory and a set intersection per query.
// Rust wrinkle: 27-way arena nodes `[u32; 27]` (Default works up to 32), `b'{' - b'a' == 26` so the separator maps to slot 26 with the same `ix()`.
// Interviewer is testing: encoding two constraints in ONE path with a separator, and why overwriting index at each node gives "latest".
// Mistakes: no separator (a suffix and prefix could blend); storing indices in a Vec per node (memory blow-up) when only the max is needed.
// Follow-ups: memoize repeated queries (HashMap<(prefix, suffix)>); weights instead of indices; updates (needs per-node multiset).
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct PsNode {
    kids: [u32; 27],
    best: i32,
}
struct WordFilter {
    nodes: Vec<PsNode>,
}
impl WordFilter {
    fn new(words: &[String]) -> Self {
        let mut nodes = vec![PsNode {
            kids: [0; 27],
            best: -1,
        }];
        for (i, w) in words.iter().enumerate() {
            for s in 0..=w.len() {
                let key = format!("{}{{{}", &w[s..], w);
                let mut cur = 0usize;
                nodes[0].best = i as i32;
                for b in key.bytes() {
                    let c = ix(b);
                    if nodes[cur].kids[c] == 0 {
                        nodes.push(PsNode {
                            kids: [0; 27],
                            best: -1,
                        });
                        let id = (nodes.len() - 1) as u32;
                        nodes[cur].kids[c] = id;
                    }
                    cur = nodes[cur].kids[c] as usize;
                    nodes[cur].best = i as i32; // indices arrive in increasing order, so this is a running max
                }
            }
        }
        WordFilter { nodes }
    }
    fn f(&self, prefix: &str, suffix: &str) -> i32 {
        let mut cur = 0usize;
        for b in format!("{suffix}{{{prefix}").bytes() {
            match self.nodes[cur].kids[ix(b)] {
                0 => return -1,
                n => cur = n as usize,
            }
        }
        self.nodes[cur].best
    }
}
fn q8() {
    let wf = WordFilter::new(&["apple".to_string()]);
    assert_eq!(wf.f("a", "e"), 0);
    assert_eq!(wf.f("b", ""), -1);
    let mut r = Rng(0x8888_0F0F_8888_0F0F);
    for _ in 0..2000 {
        let words: Vec<String> = (0..1 + r.range(8))
            .map(|_| gen_word(&mut r, 5, 3))
            .collect(); // duplicates allowed: the later index wins
        let wf = WordFilter::new(&words);
        for _ in 0..10 {
            let (p, s) = (gen_word(&mut r, 3, 3), gen_word(&mut r, 3, 3));
            let (p, s) = (
                if r.range(5) == 0 { String::new() } else { p },
                if r.range(5) == 0 { String::new() } else { s },
            );
            let want = words
                .iter()
                .rposition(|w| w.starts_with(p.as_str()) && w.ends_with(s.as_str()))
                .map_or(-1, |i| i as i32);
            assert_eq!(wf.f(&p, &s), want, "{words:?} {p}/{s}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. Concatenated Words (LeetCode 472): from a list of distinct words, return those that are a concatenation of at least two OTHER words
// from the list.
// Approach: put all words in a trie. For each word run the Word Break DP, walking the trie from every reachable boundary; forbid the single
// piece that covers the whole word (start 0, end at the last letter), so reaching the end requires >= 2 pieces. O(total chars * L) overall.
// Alternative: sort by length and DP each word against a HashSet of the shorter words (substring hashing: O(L^2) per word).
// Rust wrinkle: reuse the same arena trie for every word (build once). Shorter words only can be pieces, but because the whole word is the only
// piece of full length and is excluded, no explicit length ordering is needed.
// Interviewer is testing: reducing to Word Break, the "at least two" subtlety, reuse of one trie for all words, and complexity vs the hash version.
// Mistakes: counting a word as built from itself; including the empty string; recursion without memo (exponential).
// Follow-ups: longest concatenated word; count the number of ways; allow each word to be used once only (bitmask/backtracking).
// ---------------------------------------------------------------------------------------------------------------------
fn concatenated_words(words: &[String]) -> Vec<String> {
    let (mut go, mut end) = (vec![[0u32; 26]], vec![false]);
    for w in words.iter().filter(|w| !w.is_empty()) {
        let mut cur = 0usize;
        for b in w.bytes() {
            if go[cur][ix(b)] == 0 {
                go.push([0; 26]);
                end.push(false);
                let id = (go.len() - 1) as u32;
                go[cur][ix(b)] = id;
            }
            cur = go[cur][ix(b)] as usize;
        }
        end[cur] = true;
    }
    let mut res = Vec::new();
    for w in words.iter().filter(|w| !w.is_empty()) {
        let b = w.as_bytes();
        let mut dp = vec![false; b.len() + 1];
        dp[0] = true;
        for i in 0..b.len() {
            if !dp[i] {
                continue;
            }
            let mut cur = 0usize;
            for j in i..b.len() {
                match go[cur][ix(b[j])] {
                    0 => break,
                    n => cur = n as usize,
                }
                if end[cur] && !(i == 0 && j + 1 == b.len()) {
                    dp[j + 1] = true;
                }
            }
        }
        if dp[b.len()] {
            res.push(w.clone());
        }
    }
    res.sort();
    res
}
fn q9() {
    let d = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        concatenated_words(&d(&[
            "cat",
            "cats",
            "catsdogcats",
            "dog",
            "dogcatsdog",
            "hippopotamuses",
            "rat",
            "ratcatdogcat"
        ])),
        vec!["catsdogcats", "dogcatsdog", "ratcatdogcat"]
    );
    assert_eq!(
        concatenated_words(&d(&["cat", "dog", "catdog"])),
        vec!["catdog"]
    );
    assert!(concatenated_words(&d(&["a"])).is_empty());
    fn brute(s: &str, set: &HashSet<&str>) -> bool {
        s.is_empty() || (1..=s.len()).any(|k| set.contains(&s[..k]) && brute(&s[k..], set))
    }
    let mut r = Rng(0x9999_4747_0000_2222);
    for _ in 0..2000 {
        let set: BTreeSet<String> = (0..1 + r.range(8))
            .map(|_| gen_word(&mut r, 6, 2))
            .collect();
        let words: Vec<String> = set.iter().cloned().collect();
        let want: Vec<String> = words
            .iter()
            .filter(|w| {
                let others: HashSet<&str> =
                    set.iter().filter(|x| x != w).map(|x| x.as_str()).collect();
                brute(w, &others) // distinct words: a split into others needs at least two pieces
            })
            .cloned()
            .collect();
        assert_eq!(concatenated_words(&words), want, "{words:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Design Search Autocomplete System (LeetCode 642): `input(c)` is called per typed character; for a normal char return the top 3
// historical sentences (by hot degree = times typed, ties alphabetical) that start with everything typed so far; '#' ends the sentence:
// record it (count + 1), reset, return [].
// Approach: a trie plus a CURSOR: the state machine remembers the node for the current prefix, so each character costs O(1) to advance (not a
// walk from the root). Once the cursor falls off the trie it stays dead (None) until '#', but the typed text still accumulates so '#' records it.
// Ranking at query time: collect the subtree under the cursor and take the best 3 (or keep a per-node top-3 cache, Q10 of the core file).
// Rust wrinkle: the cursor cannot be a `&Node` stored next to `&mut self` (self-referential borrow), so nodes live in an arena and the cursor is
// an `Option<usize>`. Children are `BTreeMap<char, usize>` because sentences contain spaces and any char, and sorted order helps ties.
// Interviewer is testing: modelling an object with mutable session state, the arena/cursor pattern, the '#' reset, and tie-breaking.
// Mistakes: restarting from the root each call (O(n^2) per sentence); forgetting to reset the cursor and buffer on '#'; recording nothing when the
// cursor died; returning more than 3.
// Follow-ups: per-node top-3 caches; decay of old counts; backspace support (keep a stack of cursors); sharded tries by first letter.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct SNode {
    kids: BTreeMap<char, usize>,
    count: u32,
}
struct AutoSystem {
    nodes: Vec<SNode>,
    cursor: Option<usize>,
    typed: String,
}
fn hot_cmp(a: &(u32, String), b: &(u32, String)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1))
}
impl AutoSystem {
    fn new(sentences: &[(String, u32)]) -> Self {
        let mut s = AutoSystem {
            nodes: vec![SNode::default()],
            cursor: Some(0),
            typed: String::new(),
        };
        for (w, c) in sentences {
            s.record(w, *c);
        }
        s
    }
    fn record(&mut self, w: &str, add: u32) {
        let mut cur = 0usize;
        for c in w.chars() {
            cur = match self.nodes[cur].kids.get(&c) {
                Some(&n) => n,
                None => {
                    self.nodes.push(SNode::default());
                    let id = self.nodes.len() - 1;
                    self.nodes[cur].kids.insert(c, id);
                    id
                }
            };
        }
        self.nodes[cur].count += add;
    }
    fn input(&mut self, c: char) -> Vec<String> {
        if c == '#' {
            let typed = std::mem::take(&mut self.typed);
            self.record(&typed, 1);
            self.cursor = Some(0);
            return Vec::new();
        }
        self.typed.push(c);
        self.cursor = self
            .cursor
            .and_then(|u| self.nodes[u].kids.get(&c).copied()); // O(log k): one edge, not a walk from the root
        let Some(start) = self.cursor else {
            return Vec::new();
        };
        let mut all = Vec::new();
        let mut stack = vec![(start, self.typed.clone())];
        while let Some((u, s)) = stack.pop() {
            if self.nodes[u].count > 0 {
                all.push((self.nodes[u].count, s.clone()));
            }
            for (&ch, &v) in &self.nodes[u].kids {
                let mut t = s.clone();
                t.push(ch);
                stack.push((v, t));
            }
        }
        all.sort_by(hot_cmp);
        all.into_iter().take(3).map(|x| x.1).collect()
    }
}
fn q10() {
    let mut a = AutoSystem::new(&[
        ("i love you".into(), 5),
        ("island".into(), 3),
        ("ironman".into(), 2),
        ("i love leetcode".into(), 2),
    ]);
    assert_eq!(
        a.input('i'),
        vec!["i love you", "island", "i love leetcode"]
    );
    assert_eq!(a.input(' '), vec!["i love you", "i love leetcode"]);
    assert!(a.input('a').is_empty());
    assert!(a.input('#').is_empty());
    assert_eq!(
        a.input('i'),
        vec!["i love you", "island", "i love leetcode"]
    );
    assert_eq!(a.input(' '), vec!["i love you", "i love leetcode", "i a"]);
    let mut r = Rng(0xAA10_0000_0000_0A10);
    let alpha = ['a', 'b', ' '];
    for _ in 0..500 {
        let init: BTreeMap<String, u32> = (0..r.range(6))
            .map(|_| {
                (
                    (0..1 + r.range(4))
                        .map(|_| alpha[r.range(3)])
                        .collect::<String>(),
                    1 + r.range(3) as u32,
                )
            })
            .collect();
        let mut sys = AutoSystem::new(
            &init
                .iter()
                .map(|(s, &c)| (s.clone(), c))
                .collect::<Vec<_>>(),
        );
        let (mut model, mut typed) = (init, String::new());
        for _ in 0..60 {
            let c = if r.range(6) == 0 {
                '#'
            } else {
                alpha[r.range(3)]
            };
            let got = sys.input(c);
            if c == '#' {
                *model.entry(std::mem::take(&mut typed)).or_insert(0) += 1;
                assert!(got.is_empty());
            } else {
                typed.push(c);
                let mut all: Vec<(u32, String)> = model
                    .iter()
                    .filter(|(s, _)| s.starts_with(typed.as_str()))
                    .map(|(s, &c)| (c, s.clone()))
                    .collect();
                all.sort_by(hot_cmp);
                assert_eq!(
                    got,
                    all.into_iter().take(3).map(|x| x.1).collect::<Vec<_>>(),
                    "typed {typed:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Design an in-memory file system (LeetCode 588): ls(path), mkdir(path), addContentToFile(path, text), readContentFromFile(path).
// Approach: a PATH TRIE: each node is a directory or file; children keyed by path SEGMENT (a `String`, not a char). `ls` of a directory lists
// child names sorted (a `BTreeMap` gives that for free); `ls` of a file returns just its name. mkdir creates every missing segment;
// addContentToFile creates the file if needed and appends. All ops O(path length) (plus O(k log k) for ls without BTreeMap).
// Rust wrinkle: walking down while CREATING needs `cur = cur.kids.entry(seg.to_string()).or_default();`: a `&mut` cursor reassigned each step.
// Returning `&mut FsNode` from a helper (`walk_create`) keeps mkdir and add one-liners. Content `Option<String>` doubles as the "is a file" flag.
// Interviewer is testing: the trie generalization (segments instead of chars), sorted listing, the file-vs-directory distinction for `ls`.
// Mistakes: splitting "/" into empty segments (filter them out); overwriting file content instead of appending; unsorted ls output.
// Follow-ups: delete / move (reattach a subtree with `remove` + `insert`); rename; sizes and `du`; permissions; persistence (arena + serialization).
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct FsNode {
    kids: BTreeMap<String, FsNode>,
    content: Option<String>,
}
#[derive(Default)]
struct Fs {
    root: FsNode,
}
fn parts(p: &str) -> impl Iterator<Item = &str> {
    p.split('/').filter(|s| !s.is_empty())
}
impl Fs {
    fn walk_create(&mut self, path: &str) -> &mut FsNode {
        let mut cur = &mut self.root;
        for seg in parts(path) {
            cur = cur.kids.entry(seg.to_string()).or_default();
        }
        cur
    }
    fn walk(&self, path: &str) -> Option<&FsNode> {
        let mut cur = &self.root;
        for seg in parts(path) {
            cur = cur.kids.get(seg)?;
        }
        Some(cur)
    }
    fn mkdir(&mut self, path: &str) {
        self.walk_create(path);
    }
    fn add_content(&mut self, path: &str, text: &str) {
        self.walk_create(path)
            .content
            .get_or_insert_with(String::new)
            .push_str(text);
    }
    fn read(&self, path: &str) -> Option<&str> {
        self.walk(path)?.content.as_deref()
    }
    fn ls(&self, path: &str) -> Vec<String> {
        match self.walk(path) {
            None => vec![],
            Some(n) if n.content.is_some() => vec![parts(path).last().unwrap().to_string()],
            Some(n) => n.kids.keys().cloned().collect(),
        }
    }
}
fn q11() {
    let mut fs = Fs::default();
    assert!(fs.ls("/").is_empty());
    fs.mkdir("/a/b/c");
    fs.add_content("/a/b/c/d", "hel");
    fs.add_content("/a/b/c/d", "lo");
    assert_eq!(fs.ls("/"), vec!["a"]);
    assert_eq!(fs.read("/a/b/c/d"), Some("hello"));
    assert_eq!(fs.ls("/a/b/c/d"), vec!["d"]);
    assert_eq!(fs.ls("/a/b/c"), vec!["d"]);
    assert_eq!(fs.read("/a/b/c"), None);
    // oracle: a flat model of directories (a set of paths) and files (a map path -> content)
    let mut r = Rng(0xF5F5_0000_1111_F5F5);
    for _ in 0..2000 {
        let (mut fs, mut dirs, mut files) = (
            Fs::default(),
            BTreeSet::<String>::new(),
            BTreeMap::<String, String>::new(),
        );
        let dn = ["a", "b", "c"];
        let fnm = ["f", "g"];
        for _ in 0..25 {
            let depth = r.range(3);
            let dir_path: Vec<&str> = (0..depth).map(|_| dn[r.range(3)]).collect();
            let mut add_dirs = |p: &[&str]| {
                for k in 1..=p.len() {
                    dirs.insert(format!("/{}", p[..k].join("/")));
                }
            };
            match r.range(3) {
                0 => {
                    fs.mkdir(&format!("/{}", dir_path.join("/")));
                    add_dirs(&dir_path);
                }
                1 => {
                    let f = fnm[r.range(2)];
                    let path = format!(
                        "/{}",
                        dir_path
                            .iter()
                            .chain(std::iter::once(&f))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("/")
                    );
                    let text = ["x", "yz"][r.range(2)];
                    fs.add_content(&path, text);
                    add_dirs(&dir_path);
                    files.entry(path).or_default().push_str(text);
                }
                _ => {
                    let path = format!("/{}", dir_path.join("/"));
                    let want: Vec<String> = {
                        let prefix = if path == "/" {
                            "/".to_string()
                        } else {
                            format!("{path}/")
                        };
                        let mut names: BTreeSet<String> = BTreeSet::new();
                        for p in dirs.iter().chain(files.keys()) {
                            if let Some(rest) = p.strip_prefix(prefix.as_str()) {
                                names.insert(rest.split('/').next().unwrap().to_string());
                            }
                        }
                        names.into_iter().collect()
                    };
                    assert_eq!(fs.ls(&path), want, "ls {path}");
                }
            }
        }
        for (p, c) in &files {
            assert_eq!(fs.read(p), Some(c.as_str()));
            assert_eq!(fs.ls(p), vec![p.rsplit('/').next().unwrap().to_string()]);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. Palindrome Pairs (LeetCode 336): given distinct words, return all (i, j), i != j, with words[i] + words[j] a palindrome.
// Approach A (brute): all pairs and a palindrome check: O(n^2 * L). Approach B (hash map): for each word try every split (prefix, suffix);
// if prefix is a palindrome look up reverse(suffix), and vice versa. O(n * L^2). Approach C (trie of REVERSED words), below, same bound with
// prefix sharing: insert each word reversed; at every node on its path record the word's id in `pal` when the REMAINING reversed part is a
// palindrome. Then walk each word forward through the trie: (1) whenever the walk passes a node where some word j ENDS and the leftover tail
// of our word is a palindrome, (i, j) is a pair (j shorter or equal); (2) if the whole word is consumed, every id in that node's `pal` list is a
// pair (j longer). The empty word pairs with every palindrome and needs no special case.
// Rust wrinkle: arena nodes with `pal: Vec<usize>`; compute palindromicity with `b.iter().eq(b.iter().rev())` on byte slices (no allocation).
// `nodes.push` while holding an index (not a reference) keeps the borrow checker quiet.
// Interviewer is testing: the case analysis by length (shorter / equal / longer), the empty string, not pairing a word with itself.
// Mistakes: counting the equal-length case twice; forgetting the empty word; pairing i with i for palindromic words; O(n^2) brute force.
// Follow-ups: ordered output; words with duplicates (then dedupe pairs); k-palindromes; longest palindromic concatenation.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct PNode {
    kids: [u32; 26],
    word: Option<usize>,
    pal: Vec<usize>,
}
fn is_pal(b: &[u8]) -> bool {
    b.iter().eq(b.iter().rev())
}
fn palindrome_pairs(words: &[String]) -> Vec<(usize, usize)> {
    let mut nodes = vec![PNode::default()];
    for (j, w) in words.iter().enumerate() {
        let rev: Vec<u8> = w.bytes().rev().collect();
        let mut cur = 0usize;
        for t in 0..=rev.len() {
            if is_pal(&rev[t..]) {
                nodes[cur].pal.push(j);
            }
            if t == rev.len() {
                break;
            }
            let c = ix(rev[t]);
            if nodes[cur].kids[c] == 0 {
                nodes.push(PNode::default());
                let id = (nodes.len() - 1) as u32;
                nodes[cur].kids[c] = id;
            }
            cur = nodes[cur].kids[c] as usize;
        }
        nodes[cur].word = Some(j);
    }
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let b = w.as_bytes();
        let mut cur = 0usize;
        let mut alive = true;
        for t in 0..b.len() {
            if let Some(j) = nodes[cur].word {
                if j != i && is_pal(&b[t..]) {
                    out.push((i, j)); // word j (shorter) ends here and what remains of ours is a palindrome
                }
            }
            match nodes[cur].kids[ix(b[t])] {
                0 => {
                    alive = false;
                    break;
                }
                n => cur = n as usize,
            }
        }
        if alive {
            out.extend(nodes[cur].pal.iter().filter(|&&j| j != i).map(|&j| (i, j))); // our word fully consumed: j equal or longer
        }
    }
    out.sort();
    out
}
fn q12() {
    let d = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        palindrome_pairs(&d(&["abcd", "dcba", "lls", "s", "sssll"])),
        vec![(0, 1), (1, 0), (2, 4), (3, 2)]
    );
    assert_eq!(
        palindrome_pairs(&d(&["bat", "tab", "cat"])),
        vec![(0, 1), (1, 0)]
    );
    assert_eq!(palindrome_pairs(&d(&["a", ""])), vec![(0, 1), (1, 0)]);
    let mut r = Rng(0x1212_AAAA_3434_BBBB);
    for _ in 0..2000 {
        let mut set: BTreeSet<String> = (0..1 + r.range(9))
            .map(|_| gen_word(&mut r, 4, 2))
            .collect();
        if r.range(4) == 0 {
            set.insert(String::new());
        }
        let mut words: Vec<String> = set.into_iter().collect();
        words.reverse();
        let mut want = Vec::new();
        for i in 0..words.len() {
            for j in 0..words.len() {
                if i != j && is_pal(format!("{}{}", words[i], words[j]).as_bytes()) {
                    want.push((i, j));
                }
            }
        }
        assert_eq!(palindrome_pairs(&words), want, "{words:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. Shortest unique prefix for each word (e.g. {"zebra","dog","duck","dove"} -> {"z","dog","du","dov"}).
// Approach: insert all words with a PASS COUNT per node (how many words go through it). For each word, walk down and stop at the first node
// whose count is 1: nobody else shares that prefix. O(total chars). Brute force: for each word try each prefix length against all other
// words: O(n^2 * L). If a word is a prefix of another (e.g. "a" and "ab"), no proper unique prefix exists: report the whole word.
// Rust wrinkle: counts live in the nodes, so one pass over the input builds them and the query pass only reads: build `&mut`, then freeze.
// Interviewer is testing: aggregate-in-node (counts), the prefix-of-another-word edge case, and whether you can state why count == 1 suffices.
// Mistakes: stopping when `end` is set instead of when count is 1; off-by-one in the returned slice; quadratic comparison of all pairs.
// Follow-ups: shortest prefix that is unique AND at least 3 chars (clamp); abbreviations (LeetCode 527); handling duplicates in the input.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct PassNode {
    kids: [Option<Box<PassNode>>; 26],
    pass: usize,
}
fn shortest_unique_prefixes(words: &[String]) -> Vec<String> {
    let mut root = PassNode::default();
    for w in words {
        let mut cur = &mut root;
        for b in w.bytes() {
            cur = cur.kids[ix(b)].get_or_insert_with(Default::default);
            cur.pass += 1;
        }
    }
    words
        .iter()
        .map(|w| {
            let mut cur = &root;
            for (i, b) in w.bytes().enumerate() {
                cur = cur.kids[ix(b)].as_deref().unwrap();
                if cur.pass == 1 {
                    return w[..=i].to_string();
                }
            }
            w.clone() // another word extends this one: no shorter unique prefix
        })
        .collect()
}
fn q13() {
    let d = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        shortest_unique_prefixes(&d(&["zebra", "dog", "duck", "dove"])),
        d(&["z", "dog", "du", "dov"])
    );
    assert_eq!(shortest_unique_prefixes(&d(&["a", "ab"])), d(&["a", "ab"]));
    assert!(shortest_unique_prefixes(&[]).is_empty());
    let mut r = Rng(0x1313_5A5A_0000_1313);
    for _ in 0..2000 {
        let set: BTreeSet<String> = (0..1 + r.range(10))
            .map(|_| gen_word(&mut r, 5, 3))
            .collect();
        let words: Vec<String> = set.into_iter().collect();
        let want: Vec<String> = words
            .iter()
            .map(|w| {
                (1..=w.len())
                    .find(|&k| words.iter().filter(|o| o.starts_with(&w[..k])).count() == 1)
                    .map_or(w.clone(), |k| w[..k].to_string())
            })
            .collect();
        assert_eq!(shortest_unique_prefixes(&words), want, "{words:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. Word Squares (LeetCode 425): given distinct words of equal length n, find all sequences of n words forming a square whose k-th ROW
// reads the same as its k-th COLUMN. A word may be reused.
// Approach: backtracking by rows. After choosing rows 0..k, row k must start with the letters rows[0][k], rows[1][k], ..., rows[k-1][k]: a
// PREFIX. A trie that stores, at every node, the list of word ids sharing that prefix makes "all words with this prefix" an O(prefix) lookup.
// Brute force tries |W|^n combinations. With the trie, branching shrinks quickly because each new row is heavily constrained.
// Rust wrinkle: the trie node holds `Vec<usize>` ids (not Strings): candidates borrow from the arena while the recursion pushes onto a separate
// `rows` Vec: two disjoint things, so no clone is needed except when recording a finished square.
// Interviewer is testing: seeing the prefix constraint (symmetry), pruning early, and the memory trade-off of ids at every node (O(n * L)).
// Mistakes: building the prefix from the wrong index (rows[i][k] vs rows[k][i] are the same thing by symmetry, but be consistent); not
// allowing reuse; forgetting that the prefix for the first row is empty (all words).
// Follow-ups: sorted-array range instead of per-node lists (less memory); count squares only; non-square rectangles.
// ---------------------------------------------------------------------------------------------------------------------
fn word_squares(words: &[String]) -> Vec<Vec<String>> {
    let Some(first) = words.first() else {
        return vec![];
    };
    let n = first.len();
    let mut kids: Vec<[u32; 26]> = vec![[0; 26]];
    let mut ids: Vec<Vec<usize>> = vec![Vec::new()];
    for (i, w) in words.iter().enumerate() {
        let mut cur = 0usize;
        ids[0].push(i);
        for b in w.bytes() {
            if kids[cur][ix(b)] == 0 {
                kids.push([0; 26]);
                ids.push(Vec::new());
                let id = (kids.len() - 1) as u32;
                kids[cur][ix(b)] = id;
            }
            cur = kids[cur][ix(b)] as usize;
            ids[cur].push(i);
        }
    }
    fn go(
        words: &[String],
        kids: &[[u32; 26]],
        ids: &[Vec<usize>],
        n: usize,
        rows: &mut Vec<usize>,
        out: &mut Vec<Vec<String>>,
    ) {
        if rows.len() == n {
            out.push(rows.iter().map(|&i| words[i].clone()).collect());
            return;
        }
        let k = rows.len();
        let mut cur = 0usize;
        for &r in rows.iter() {
            match kids[cur][ix(words[r].as_bytes()[k])] {
                0 => return, // no word has this prefix: prune
                nx => cur = nx as usize,
            }
        }
        for &cand in &ids[cur] {
            rows.push(cand);
            go(words, kids, ids, n, rows, out);
            rows.pop();
        }
    }
    let mut out = Vec::new();
    go(words, &kids, &ids, n, &mut Vec::new(), &mut out);
    out.sort();
    out
}
fn q14() {
    let d = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        word_squares(&d(&["area", "lead", "wall", "lady", "ball"])),
        vec![
            d(&["ball", "area", "lead", "lady"]),
            d(&["wall", "area", "lead", "lady"])
        ]
    );
    assert!(word_squares(&[]).is_empty());
    let mut r = Rng(0x1414_0000_FEED_1414);
    for _ in 0..2000 {
        let n = 1 + r.range(3);
        let set: BTreeSet<String> = (0..1 + r.range(6))
            .map(|_| {
                (0..n)
                    .map(|_| (b'a' + r.range(2) as u8) as char)
                    .collect::<String>()
            })
            .collect();
        let words: Vec<String> = set.into_iter().collect();
        // brute force: every n-tuple of words, check row k == column k
        let mut want = Vec::new();
        let mut idx = vec![0usize; n];
        loop {
            let rows: Vec<&String> = idx.iter().map(|&i| &words[i]).collect();
            if (0..n).all(|i| (0..n).all(|j| rows[i].as_bytes()[j] == rows[j].as_bytes()[i])) {
                want.push(rows.into_iter().cloned().collect::<Vec<_>>());
            }
            let mut p = n;
            while p > 0 && idx[p - 1] + 1 == words.len() {
                idx[p - 1] = 0;
                p -= 1;
            }
            if p == 0 {
                break;
            }
            idx[p - 1] += 1;
        }
        want.sort();
        assert_eq!(word_squares(&words), want, "{words:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. Put an LRU cache of prefix results in front of a trie, and invalidate it correctly.
// Approach: hot prefixes ("a", "th", "app") dominate autocomplete traffic, so cache `prefix -> top-3 words`. Evict the least recently used
// when full. Correctness rule people forget: INSERTING a word changes the answer for EVERY prefix of that word, so those cache entries must be
// removed (|word| + 1 lookups; or version-stamp the cache). LRU here = `HashMap<key, (value, tick)>` + `BTreeMap<tick, key>`: O(log n) per op
// and 100 percent safe Rust (the classic O(1) version is a hash map plus a doubly linked list: see linked_lists_in_rust.rs Q20/Q21).
// Rust wrinkle: updating the recency order needs `&mut self` over two fields at once (`map` and `order`): direct field access in one method
// satisfies the borrow checker, a helper taking `&mut self.map` plus `&mut self.order` also works. `BTreeMap::pop_first` gives the oldest tick.
// Interviewer is testing: cache invalidation (the second hard problem), the eviction data structure, hit/miss accounting, capacity edge cases.
// Mistakes: caching but never invalidating (stale suggestions); invalidating only the exact word; leaking `order` entries when a key is re-put.
// Follow-ups: TTLs; caching misses (negative caching); sharded/concurrent caches (per-shard Mutex); write-through vs write-around.
// ---------------------------------------------------------------------------------------------------------------------
struct Lru<V: Clone> {
    cap: usize,
    tick: u64,
    map: HashMap<String, (V, u64)>,
    order: BTreeMap<u64, String>,
    hits: u64,
    misses: u64,
}
impl<V: Clone> Lru<V> {
    fn new(cap: usize) -> Self {
        Lru {
            cap,
            tick: 0,
            map: HashMap::new(),
            order: BTreeMap::new(),
            hits: 0,
            misses: 0,
        }
    }
    fn get(&mut self, k: &str) -> Option<V> {
        match self.map.get_mut(k) {
            Some((v, t)) => {
                self.order.remove(t);
                self.tick += 1;
                *t = self.tick;
                self.order.insert(self.tick, k.to_string());
                self.hits += 1;
                Some(v.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }
    fn put(&mut self, k: &str, v: V) {
        if self.cap == 0 {
            return;
        }
        if let Some((_, t)) = self.map.remove(k) {
            self.order.remove(&t);
        } else if self.map.len() == self.cap {
            if let Some((_, oldest)) = self.order.pop_first() {
                self.map.remove(&oldest);
            }
        }
        self.tick += 1;
        self.order.insert(self.tick, k.to_string());
        self.map.insert(k.to_string(), (v, self.tick));
    }
    fn invalidate(&mut self, k: &str) {
        if let Some((_, t)) = self.map.remove(k) {
            self.order.remove(&t);
        }
    }
}
#[derive(Default)]
struct DictTrie {
    kids: BTreeMap<char, DictTrie>,
    end: bool,
}
impl DictTrie {
    fn insert(&mut self, w: &str) {
        let mut cur = &mut *self;
        for c in w.chars() {
            cur = cur.kids.entry(c).or_default();
        }
        cur.end = true;
    }
    /// The 3 lexicographically smallest words with this prefix (BTreeMap order makes the first 3 found the smallest).
    fn top3(&self, p: &str) -> Vec<String> {
        fn go(n: &DictTrie, path: &mut String, out: &mut Vec<String>) {
            if out.len() == 3 {
                return;
            }
            if n.end {
                out.push(path.clone());
            }
            for (&c, k) in &n.kids {
                path.push(c);
                go(k, path, out);
                path.pop();
            }
        }
        let mut cur = self;
        for c in p.chars() {
            match cur.kids.get(&c) {
                Some(n) => cur = n,
                None => return vec![],
            }
        }
        let mut out = Vec::new();
        go(cur, &mut p.to_string(), &mut out);
        out
    }
}
struct CachedAutocomplete {
    trie: DictTrie,
    cache: Lru<Vec<String>>,
}
impl CachedAutocomplete {
    fn insert(&mut self, w: &str) {
        self.trie.insert(w);
        for k in 0..=w.len() {
            self.cache.invalidate(&w[..k]); // every prefix of w (including "") may now have a different top 3
        }
    }
    fn suggest(&mut self, p: &str) -> Vec<String> {
        if let Some(v) = self.cache.get(p) {
            return v;
        }
        let v = self.trie.top3(p);
        self.cache.put(p, v.clone());
        v
    }
}
fn q15() {
    let mut l: Lru<i32> = Lru::new(2);
    l.put("a", 1);
    l.put("b", 2);
    assert_eq!(l.get("a"), Some(1)); // a is now most recent
    l.put("c", 3); // evicts b
    assert_eq!(l.get("b"), None);
    assert_eq!((l.get("a"), l.get("c")), (Some(1), Some(3)));
    l.put("a", 10); // re-put must not leak an order entry
    assert_eq!(l.order.len(), l.map.len());
    let mut r = Rng(0x1515_CAFE_0000_1515);
    let mut total_hits = 0;
    for _ in 0..2000 {
        let cap = r.range(5);
        let mut ca = CachedAutocomplete {
            trie: DictTrie::default(),
            cache: Lru::new(cap),
        };
        let mut dict = BTreeSet::<String>::new();
        let mut queries = 0u64;
        for _ in 0..40 {
            if r.range(3) == 0 {
                let w = gen_word(&mut r, 4, 3);
                ca.insert(&w);
                dict.insert(w);
            } else {
                let mut p = gen_word(&mut r, 3, 3);
                if r.range(6) == 0 {
                    p.clear();
                }
                queries += 1;
                let want: Vec<String> = dict
                    .iter()
                    .filter(|w| w.starts_with(p.as_str()))
                    .take(3)
                    .cloned()
                    .collect();
                assert_eq!(ca.suggest(&p), want, "prefix {p:?}");
            }
            assert!(ca.cache.map.len() <= cap && ca.cache.order.len() == ca.cache.map.len());
        }
        assert_eq!(ca.cache.hits + ca.cache.misses, queries);
        total_hits += ca.cache.hits;
    }
    assert!(total_hits > 0, "the cache must actually be used");
    println!("    Q15: cache hits across the random runs: {total_hits}");
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
}
