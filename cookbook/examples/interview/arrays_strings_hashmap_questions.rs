//! RECIPE 620 (interview): arrays, strings, hash maps and bit tricks: the most-asked coding questions, answered in Rust.
//! LEVEL: intermediate
//! Use in: preparing for coding interviews; recognising the pattern (two pointers, sliding window, prefix sums, ...) behind a question.   Run: cargo run --release --example arrays_strings_hashmap_questions
//! COMMON USES: phone screens and on-sites (NeetCode / Blind 75 / Grind 75 style), log and text processing, interval merging, windowed statistics.
#![allow(dead_code)]
//! Read: https://www.techinterviewhandbook.org/grind75 , https://neetcode.io/practice , https://interviewing.io/rust-interview-questions , https://cp-algorithms.com/
//!
//! QUESTION BANK (numbering restarts in each interview file; a generator reads these lines)
//! Q1. Two Sum: return indices of two numbers adding to a target (unsorted input, one pass with a hash map). [Easy] [asked: very common]
//! Q2. Two Sum II (sorted input, two pointers) and Two Sum IV (find a pair in a BST). [Easy] [asked: very common]
//! Q3. 3Sum and 4Sum: all unique triplets/quadruplets summing to a target (sort + two pointers + dedupe). [Medium] [asked: very common]
//! Q4. Contains Duplicate, Valid Anagram and Group Anagrams. [Easy] [asked: very common]
//! Q5. Top K Frequent Elements (hash map + bucket sort). [Medium] [asked: very common]
//! Q6. Product of Array Except Self without division. [Medium] [asked: very common]
//! Q7. Maximum Subarray (Kadane), with indices and the circular follow-up. [Medium] [asked: very common]
//! Q8. Maximum Product Subarray. [Medium] [asked: common]
//! Q9. Best Time to Buy and Sell Stock I, II, III (and IV with k transactions). [Easy] [asked: very common]
//! Q10. Merge Intervals, Insert Interval and Non-overlapping Intervals. [Medium] [asked: very common]
//! Q11. Rotate Array (three reversals), Move Zeroes and Remove Duplicates in place. [Easy] [asked: very common]
//! Q12. Container With Most Water and Trapping Rain Water (two pointers). [Hard] [asked: very common]
//! Q13. Subarray Sum Equals K (prefix sums + hash map). [Medium] [asked: very common]
//! Q14. Longest Consecutive Sequence in O(n) with a hash set. [Medium] [asked: very common]
//! Q15. Longest Substring Without Repeating Characters (sliding window). [Medium] [asked: very common]
//! Q16. Minimum Window Substring (sliding window with a need counter). [Hard] [asked: very common]
//! Q17. Longest Repeating Character Replacement and Permutation in String. [Medium] [asked: common]
//! Q18. Longest Palindromic Substring (expand around centre and Manacher) and Palindromic Substrings count. [Medium] [asked: very common]
//! Q19. Valid Palindrome and String to Integer (atoi) with every edge case. [Medium] [asked: very common]
//! Q20. Integer to Roman and Roman to Integer. [Medium] [asked: common]
//! Q21. Longest Common Prefix, implement strStr (KMP), Reverse Words in a String, Encode and Decode Strings. [Medium] [asked: common]
//! Q22. Spiral Matrix, Rotate Image and Set Matrix Zeroes. [Medium] [asked: very common]
//! Q23. Search in Rotated Sorted Array and Find Minimum in Rotated Sorted Array. [Medium] [asked: very common]
//! Q24. First Missing Positive, Majority Element (Boyer-Moore) and Single Number I/II. [Hard] [asked: common]
//! Q25. Count Bits, Reverse Bits, Power of Two, Sum of Two Integers without `+`. [Easy] [asked: common]
//! Q26. 3Sum Closest and Next Permutation. [Medium] [asked: common]
//! Q27. Binary search variants: lower/upper bound, search insert position, sqrt(x), Koko Eating Bananas (search on the answer). [Medium] [asked: very common]
//!
//! Every question: statement, approach (brute force then optimal, complexity, pattern), Rust wrinkle, the solution,
//! a brute-force oracle with 2000 seeded xorshift cases plus edge cases, and what the interviewer is really testing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ------------------------------------------------------------------------------------------------
// test harness: deterministic xorshift64 so every run is identical and needs no `rand` crate
// ------------------------------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn vec(&mut self, n: usize, lo: i64, hi: i64) -> Vec<i64> {
        (0..n).map(|_| self.int(lo, hi)).collect()
    }
    fn string(&mut self, alphabet: &str, max_len: usize) -> String {
        let a: Vec<char> = alphabet.chars().collect();
        let n = self.below(max_len + 1);
        (0..n).map(|_| a[self.below(a.len())]).collect()
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}
const CASES: usize = 2000;

// ================================================================================================
/// Q1. TWO SUM. Given `nums` and `target`, return indices `i < j` with `nums[i] + nums[j] == target`.
/// Exactly one solution is promised in the original; here we return `None` when there is none.
///   nums = [2, 7, 11, 15], target = 9  ->  (0, 1)
///
/// APPROACH. Brute force: all pairs, O(n^2) time, O(1) space. Optimal: ONE PASS with a hash map from value to
/// index. For each x ask "have I already seen target - x?" and only AFTER the lookup insert x, so an element can
/// never pair with itself. O(n) time, O(n) space. Pattern: complement lookup.
/// RUST. `HashMap<i64, usize>`; use `get` then `insert` (the entry API is not needed here). Subtraction on `i64`
/// is safe for small inputs; for full-range `i32` inputs widen to `i64` first to avoid overflow in `target - x`.
/// INTERVIEWER IS TESTING: do you volunteer the brute force, move to a hash map, and handle duplicates
/// ([3,3] target 6) and "same element twice" correctly. Follow-up: sorted input -> two pointers, O(1) space (Q2).
fn two_sum(nums: &[i64], target: i64) -> Option<(usize, usize)> {
    let mut seen: HashMap<i64, usize> = HashMap::new();
    for (j, &x) in nums.iter().enumerate() {
        if let Some(&i) = seen.get(&(target - x)) {
            return Some((i, j));
        }
        seen.insert(x, j);
    }
    None
}
fn two_sum_exists_brute(nums: &[i64], target: i64) -> bool {
    (0..nums.len()).any(|i| (i + 1..nums.len()).any(|j| nums[i] + nums[j] == target))
}
fn q1() {
    let mut rng = Rng::new(1);
    for _ in 0..CASES {
        let n = rng.below(12);
        let nums = rng.vec(n, -10, 10);
        let t = rng.int(-12, 12);
        let got = two_sum(&nums, t);
        assert_eq!(
            got.is_some(),
            two_sum_exists_brute(&nums, t),
            "{nums:?} {t}"
        );
        if let Some((i, j)) = got {
            assert!(i < j && nums[i] + nums[j] == t);
        }
    }
    assert_eq!(two_sum(&[2, 7, 11, 15], 9), Some((0, 1)));
    assert_eq!(two_sum(&[3, 3], 6), Some((0, 1)));
    assert_eq!(two_sum(&[3], 6), None); // 3 must not pair with itself
    assert_eq!(two_sum(&[], 0), None);
}

// ================================================================================================
/// Q2. TWO SUM II (sorted array) and TWO SUM IV (is there a pair in a BST summing to k?).
///   numbers = [2, 7, 11, 15], target = 9 -> (0, 1);  BST [5,3,6,2,4,null,7], k = 9 -> true
///
/// APPROACH. Sorted: two pointers from both ends; sum too small -> move left up, too large -> move right down.
/// O(n) time, O(1) space. Correct because moving the pointer at the larger end can never help once the sum is
/// already too big. BST: in-order traversal is sorted, so either (a) collect and run the same two pointers
/// (O(n) space) or (b) run a forward and a backward in-order ITERATOR with explicit stacks: O(h) space.
/// RUST. Keep `right` EXCLUSIVE (`r = len`) so an empty slice does not underflow `len - 1` on `usize`.
/// Trees are `Option<Box<Node>>`; iterate with `as_deref()` to get `Option<&Node>` without moving.
/// INTERVIEWER IS TESTING: do you exploit sortedness (O(1) space), and can you turn recursion into iterators.
struct Node {
    val: i64,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}
fn two_sum_sorted(nums: &[i64], target: i64) -> Option<(usize, usize)> {
    let (mut l, mut r) = (0, nums.len()); // r exclusive
    while l + 1 < r {
        let s = nums[l] + nums[r - 1];
        match s.cmp(&target) {
            std::cmp::Ordering::Less => l += 1,
            std::cmp::Ordering::Greater => r -= 1,
            std::cmp::Ordering::Equal => return Some((l, r - 1)),
        }
    }
    None
}
fn build_bst(sorted: &[i64]) -> Option<Box<Node>> {
    if sorted.is_empty() {
        return None;
    }
    let mid = sorted.len() / 2;
    Some(Box::new(Node {
        val: sorted[mid],
        left: build_bst(&sorted[..mid]),
        right: build_bst(&sorted[mid + 1..]),
    }))
}
struct BstIter<'a> {
    stack: Vec<&'a Node>,
    forward: bool,
}
impl<'a> BstIter<'a> {
    fn new(root: &'a Option<Box<Node>>, forward: bool) -> Self {
        let mut it = BstIter {
            stack: vec![],
            forward,
        };
        it.push_path(root.as_deref());
        it
    }
    fn push_path(&mut self, mut cur: Option<&'a Node>) {
        while let Some(n) = cur {
            self.stack.push(n);
            cur = if self.forward {
                n.left.as_deref()
            } else {
                n.right.as_deref()
            };
        }
    }
    fn next(&mut self) -> Option<i64> {
        let n = self.stack.pop()?;
        let nxt = if self.forward {
            n.right.as_deref()
        } else {
            n.left.as_deref()
        };
        self.push_path(nxt);
        Some(n.val)
    }
}
fn find_target_bst(root: &Option<Box<Node>>, k: i64) -> bool {
    let (mut a, mut b) = (BstIter::new(root, true), BstIter::new(root, false));
    let (mut lo, mut hi) = (a.next(), b.next());
    while let (Some(x), Some(y)) = (lo, hi) {
        if x >= y {
            return false; // iterators met: never pair a node with itself
        }
        match (x + y).cmp(&k) {
            std::cmp::Ordering::Equal => return true,
            std::cmp::Ordering::Less => lo = a.next(),
            std::cmp::Ordering::Greater => hi = b.next(),
        }
    }
    false
}
fn q2() {
    let mut rng = Rng::new(2);
    for _ in 0..CASES {
        let n = rng.below(10);
        let mut nums = rng.vec(n, -8, 8);
        nums.sort_unstable();
        let t = rng.int(-14, 14);
        let got = two_sum_sorted(&nums, t);
        assert_eq!(got.is_some(), two_sum_exists_brute(&nums, t));
        if let Some((i, j)) = got {
            assert!(i < j && nums[i] + nums[j] == t);
        }
        let mut distinct: Vec<i64> = nums.clone();
        distinct.dedup();
        let tree = build_bst(&distinct);
        assert_eq!(
            find_target_bst(&tree, t),
            two_sum_exists_brute(&distinct, t),
            "{distinct:?} {t}"
        );
    }
    assert_eq!(two_sum_sorted(&[], 1), None);
    assert!(!find_target_bst(&build_bst(&[4]), 8)); // single node must not pair with itself
}

// ================================================================================================
/// Q3. 3SUM / 4SUM. Return all UNIQUE triplets (quadruplets) that sum to the target.
///   [-1, 0, 1, 2, -1, -4], target 0 -> [[-1,-1,2], [-1,0,1]]
///
/// APPROACH. Brute force O(n^3) (O(n^4)) with a set to dedupe. Optimal: SORT, fix the first element(s) with a loop
/// and finish with the two-pointer pair scan: O(n^2) (O(n^3)) time, O(1) extra. Dedupe by skipping equal
/// neighbours at every level (after the first choice at a level) rather than by hashing. Generalises to kSum by
/// recursion down to the k = 2 base case, with early exits when the remaining slice is too short.
/// RUST. Take a sorted copy (`to_vec` + `sort_unstable`); recursion carries `cur: &mut Vec<i64>` and uses
/// push/pop (choose/unchoose). Keep `r` exclusive to avoid `usize` underflow.
/// INTERVIEWER IS TESTING: duplicate handling (the classic bug is skipping the first occurrence instead of the
/// repeats) and whether you can state the complexity as O(n^(k-1)).
fn k_sum(nums: &[i64], target: i64, k: usize) -> Vec<Vec<i64>> {
    let mut v = nums.to_vec();
    v.sort_unstable();
    let (mut out, mut cur) = (vec![], vec![]);
    k_sum_rec(&v, 0, target, k, &mut cur, &mut out);
    out
}
fn k_sum_rec(
    v: &[i64],
    start: usize,
    target: i64,
    k: usize,
    cur: &mut Vec<i64>,
    out: &mut Vec<Vec<i64>>,
) {
    if k == 2 {
        let (mut l, mut r) = (start, v.len()); // r exclusive
        while l + 1 < r {
            let s = v[l] + v[r - 1];
            if s < target {
                l += 1;
            } else if s > target {
                r -= 1;
            } else {
                cur.extend([v[l], v[r - 1]]);
                out.push(cur.clone());
                cur.truncate(cur.len() - 2);
                l += 1;
                r -= 1;
                while l < r && v[l] == v[l - 1] {
                    l += 1;
                }
                while l < r && v[r - 1] == v[r] {
                    r -= 1;
                }
            }
        }
        return;
    }
    for i in start..v.len() {
        if v.len() - i < k {
            break; // not enough elements left
        }
        if i > start && v[i] == v[i - 1] {
            continue; // same value already tried at this depth
        }
        cur.push(v[i]);
        k_sum_rec(v, i + 1, target - v[i], k - 1, cur, out);
        cur.pop();
    }
}
fn k_sum_brute(nums: &[i64], target: i64, k: usize) -> BTreeSet<Vec<i64>> {
    fn go(
        nums: &[i64],
        start: usize,
        target: i64,
        k: usize,
        cur: &mut Vec<i64>,
        out: &mut BTreeSet<Vec<i64>>,
    ) {
        if k == 0 {
            if target == 0 {
                let mut c = cur.clone();
                c.sort_unstable();
                out.insert(c);
            }
            return;
        }
        for i in start..nums.len() {
            cur.push(nums[i]);
            go(nums, i + 1, target - nums[i], k - 1, cur, out);
            cur.pop();
        }
    }
    let mut out = BTreeSet::new();
    go(nums, 0, target, k, &mut vec![], &mut out);
    out
}
fn q3() {
    let mut rng = Rng::new(3);
    for _ in 0..CASES {
        let n = rng.below(11);
        let nums = rng.vec(n, -4, 4);
        let t = rng.int(-5, 5);
        for k in [3usize, 4] {
            let got = k_sum(&nums, t, k);
            let set: BTreeSet<Vec<i64>> = got.iter().cloned().collect();
            assert_eq!(set.len(), got.len(), "duplicates returned for {nums:?}");
            assert_eq!(set, k_sum_brute(&nums, t, k), "k={k} {nums:?} {t}");
        }
    }
    assert_eq!(
        k_sum(&[-1, 0, 1, 2, -1, -4], 0, 3),
        vec![vec![-1, -1, 2], vec![-1, 0, 1]]
    );
    assert_eq!(k_sum(&[0, 0, 0, 0], 0, 3), vec![vec![0, 0, 0]]);
}

// ================================================================================================
/// Q4. CONTAINS DUPLICATE; VALID ANAGRAM; GROUP ANAGRAMS.
///   [1,2,3,1] -> true;  "anagram" vs "nagaram" -> true;  ["eat","tea","tan","ate","nat","bat"] -> 3 groups
///
/// APPROACH. Duplicate: sort and compare neighbours O(n log n), or a `HashSet` O(n) (`insert` returns false when
/// present). Anagram: sort both (O(n log n)) or count letters in a `[i32; 26]` (O(n), O(1) space). Group anagrams:
/// key each word by its letter-count signature `[u8; 26]` (O(total chars)) instead of sorting each word
/// (O(total * log len)); both are accepted, the count key is the optimal one.
/// RUST. Arrays implement `Hash + Eq`, so `[u8; 26]` is a legal `HashMap` key. `entry(key).or_default().push(w)`
/// is THE idiom. For Unicode input, count in a `HashMap<char, i32>` instead of assuming `a..z`.
/// INTERVIEWER IS TESTING: choosing the cheaper key, asking about the alphabet, and not forgetting length checks.
fn contains_duplicate(nums: &[i64]) -> bool {
    let mut seen = HashSet::new();
    nums.iter().any(|&x| !seen.insert(x))
}
fn is_anagram(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut cnt = [0i32; 26];
    for (x, y) in a.bytes().zip(b.bytes()) {
        cnt[(x - b'a') as usize] += 1;
        cnt[(y - b'a') as usize] -= 1;
    }
    cnt.iter().all(|&c| c == 0)
}
fn group_anagrams(words: &[String]) -> Vec<Vec<String>> {
    let mut groups: HashMap<[u8; 26], Vec<String>> = HashMap::new();
    for w in words {
        let mut key = [0u8; 26];
        for b in w.bytes() {
            key[(b - b'a') as usize] += 1;
        }
        groups.entry(key).or_default().push(w.clone());
    }
    let mut out: Vec<Vec<String>> = groups.into_values().collect();
    out.iter_mut().for_each(|g| g.sort());
    out.sort(); // HashMap order is random; normalise before comparing
    out
}
fn q4() {
    let mut rng = Rng::new(4);
    for _ in 0..CASES {
        let n = rng.below(8);
        let nums = rng.vec(n, 0, 9);
        let mut s = nums.clone();
        s.sort_unstable();
        s.dedup();
        assert_eq!(contains_duplicate(&nums), s.len() != nums.len());
        let (a, b) = (rng.string("abc", 5), rng.string("abc", 5));
        let (mut sa, mut sb): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        sa.sort_unstable();
        sb.sort_unstable();
        assert_eq!(is_anagram(&a, &b), sa == sb);
        let words: Vec<String> = (0..rng.below(8)).map(|_| rng.string("abc", 4)).collect();
        let mut oracle: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for w in &words {
            let mut k: Vec<char> = w.chars().collect();
            k.sort_unstable();
            oracle
                .entry(k.into_iter().collect())
                .or_default()
                .push(w.clone());
        }
        let mut want: Vec<Vec<String>> = oracle.into_values().collect();
        want.iter_mut().for_each(|g| g.sort());
        want.sort();
        assert_eq!(group_anagrams(&words), want);
    }
    let w: Vec<String> = ["eat", "tea", "tan", "ate", "nat", "bat"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(group_anagrams(&w).len(), 3);
}

// ================================================================================================
/// Q5. TOP K FREQUENT ELEMENTS. Return the k most frequent values (any order among ties).
///   [1,1,1,2,2,3], k = 2 -> [1, 2]
///
/// APPROACH. Count with a hash map O(n). Then (a) sort the distinct values by count O(d log d); (b) a size-k
/// min-heap O(d log k); (c) BUCKET SORT by frequency: bucket[c] holds the values that occur c times, c <= n, so
/// walk buckets from high to low: O(n) time and space. Pattern: counting + bucket by frequency.
/// RUST. `*count.entry(x).or_insert(0) += 1` is the counting idiom. For heaps use `BinaryHeap<Reverse<(usize, i64)>>`.
/// `sort_unstable_by_key(|&(v, c)| (Reverse(c), v))` gives a deterministic order when you need one.
/// INTERVIEWER IS TESTING: that you beat O(n log n) when asked, and that you can name the heap alternative.
fn top_k_frequent(nums: &[i64], k: usize) -> Vec<i64> {
    let mut count: HashMap<i64, usize> = HashMap::new();
    for &x in nums {
        *count.entry(x).or_insert(0) += 1;
    }
    let mut buckets: Vec<Vec<i64>> = vec![vec![]; nums.len() + 1];
    for (&v, &c) in &count {
        buckets[c].push(v);
    }
    buckets.into_iter().rev().flatten().take(k).collect()
}
fn q5() {
    let mut rng = Rng::new(5);
    for _ in 0..CASES {
        let n = 1 + rng.below(14);
        let nums = rng.vec(n, 0, 6);
        let mut count: HashMap<i64, usize> = HashMap::new();
        for &x in &nums {
            *count.entry(x).or_insert(0) += 1;
        }
        let k = 1 + rng.below(count.len());
        let got = top_k_frequent(&nums, k);
        let distinct: HashSet<_> = got.iter().collect();
        assert_eq!(got.len(), k);
        assert_eq!(distinct.len(), k);
        let mut got_counts: Vec<usize> = got.iter().map(|v| count[v]).collect();
        got_counts.sort_unstable_by(|a, b| b.cmp(a));
        let mut all: Vec<usize> = count.values().copied().collect();
        all.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(got_counts, all[..k].to_vec(), "{nums:?} k={k}");
    }
    assert_eq!(top_k_frequent(&[1, 1, 1, 2, 2, 3], 2), vec![1, 2]);
}

// ================================================================================================
/// Q6. PRODUCT OF ARRAY EXCEPT SELF. `out[i]` = product of all elements but `nums[i]`, WITHOUT division, O(n).
///   [1,2,3,4] -> [24,12,8,6]
///
/// APPROACH. Brute force O(n^2). Division fails on zeros. Optimal: prefix products pass left-to-right filling
/// `out[i]` with the product of everything before i; then a right-to-left pass multiplies by a running suffix
/// product. O(n) time, O(1) extra space beyond the output. Pattern: prefix/suffix aggregation.
/// RUST. Mutate `out` in place by index; use `i64` (or `checked_mul`) because products overflow quickly; say so.
/// INTERVIEWER IS TESTING: zeros, the O(1) extra-space follow-up, and overflow awareness.
fn product_except_self(nums: &[i64]) -> Vec<i64> {
    let n = nums.len();
    let mut out = vec![1i64; n];
    let mut prefix = 1;
    for i in 0..n {
        out[i] = prefix;
        prefix *= nums[i];
    }
    let mut suffix = 1;
    for i in (0..n).rev() {
        out[i] *= suffix;
        suffix *= nums[i];
    }
    out
}
fn q6() {
    let mut rng = Rng::new(6);
    for _ in 0..CASES {
        let n = rng.below(9);
        let nums = rng.vec(n, -3, 3);
        let want: Vec<i64> = (0..n)
            .map(|i| (0..n).filter(|&j| j != i).map(|j| nums[j]).product())
            .collect();
        assert_eq!(product_except_self(&nums), want, "{nums:?}");
    }
    assert_eq!(product_except_self(&[1, 2, 3, 4]), vec![24, 12, 8, 6]);
    assert_eq!(product_except_self(&[0, 0, 2]), vec![0, 0, 0]);
}

// ================================================================================================
/// Q7. MAXIMUM SUBARRAY (KADANE). Largest sum of a non-empty contiguous subarray. Follow-ups: return the
/// indices; CIRCULAR array (subarray may wrap around the end).
///   [-2,1,-3,4,-1,2,1,-5,4] -> 6 (the subarray [4,-1,2,1])
///
/// APPROACH. Brute force O(n^2) over all (start, end) with prefix sums. Kadane: `cur = max(x, cur + x)`,
/// `best = max(best, cur)`: a negative running sum can only hurt, so restart. O(n) time, O(1) space. DP state:
/// best sum of a subarray ENDING at i. Circular: answer = max(plain Kadane, total - minimum subarray), except when
/// every element is negative (then total - min = 0 would mean an empty subarray) -> use the plain answer.
/// RUST. Initialise from `nums[0]`, not `0` or `i64::MIN` + addition (overflow). Use half-open `[l, r)` indices.
/// INTERVIEWER IS TESTING: all-negative input, the restart rule for indices, and the circular all-negative trap.
fn kadane(nums: &[i64]) -> (i64, usize, usize) {
    let (mut best, mut best_l, mut best_r) = (nums[0], 0, 1);
    let (mut cur, mut cur_l) = (nums[0], 0);
    for i in 1..nums.len() {
        if cur < 0 {
            cur = nums[i];
            cur_l = i;
        } else {
            cur += nums[i];
        }
        if cur > best {
            (best, best_l, best_r) = (cur, cur_l, i + 1);
        }
    }
    (best, best_l, best_r)
}
fn max_circular(nums: &[i64]) -> i64 {
    let total: i64 = nums.iter().sum();
    let max_plain = kadane(nums).0;
    let negated: Vec<i64> = nums.iter().map(|x| -x).collect();
    let min_sub = -kadane(&negated).0;
    if max_plain < 0 {
        max_plain
    } else {
        max_plain.max(total - min_sub)
    }
}
fn q7() {
    let mut rng = Rng::new(7);
    for _ in 0..CASES {
        let n = 1 + rng.below(10);
        let nums = rng.vec(n, -6, 6);
        let mut want = i64::MIN;
        for l in 0..n {
            for r in l + 1..=n {
                want = want.max(nums[l..r].iter().sum());
            }
        }
        let (b, l, r) = kadane(&nums);
        assert_eq!(b, want, "{nums:?}");
        assert_eq!(nums[l..r].iter().sum::<i64>(), b);
        let mut circ = i64::MIN;
        for s in 0..n {
            let mut acc = 0;
            for len in 1..=n {
                acc += nums[(s + len - 1) % n];
                circ = circ.max(acc);
            }
        }
        assert_eq!(max_circular(&nums), circ, "{nums:?}");
    }
    assert_eq!(kadane(&[-2, 1, -3, 4, -1, 2, 1, -5, 4]), (6, 3, 7));
    assert_eq!(max_circular(&[-3, -2, -3]), -2);
    assert_eq!(max_circular(&[5, -3, 5]), 10);
}

// ================================================================================================
/// Q8. MAXIMUM PRODUCT SUBARRAY. Largest product of a non-empty contiguous subarray (values may be negative/zero).
///   [2,3,-2,4] -> 6;  [-2,0,-1] -> 0
///
/// APPROACH. Brute force O(n^2). Track BOTH the max and min product ending at i: a negative number swaps their
/// roles, so the smallest (most negative) product may become the largest. `hi = max(x, hi*x, lo*x)`,
/// `lo = min(x, hi*x, lo*x)` using the OLD values. O(n) time, O(1) space. DP with two states.
/// RUST. Compute both new values from copies of the old ones (a classic bug is updating `hi` first and then
/// using it for `lo`). `i64` here; real inputs can overflow, mention `checked_mul`.
/// INTERVIEWER IS TESTING: why keeping only the max fails, zeros resetting the run, and a single negative.
fn max_product_subarray(nums: &[i64]) -> i64 {
    let (mut hi, mut lo, mut best) = (nums[0], nums[0], nums[0]);
    for &x in &nums[1..] {
        let (a, b) = (hi * x, lo * x);
        hi = x.max(a).max(b);
        lo = x.min(a).min(b);
        best = best.max(hi);
    }
    best
}
fn q8() {
    let mut rng = Rng::new(8);
    for _ in 0..CASES {
        let n = 1 + rng.below(9);
        let nums = rng.vec(n, -3, 3);
        let mut want = i64::MIN;
        for l in 0..n {
            for r in l + 1..=n {
                want = want.max(nums[l..r].iter().product());
            }
        }
        assert_eq!(max_product_subarray(&nums), want, "{nums:?}");
    }
    assert_eq!(max_product_subarray(&[2, 3, -2, 4]), 6);
    assert_eq!(max_product_subarray(&[-2, 0, -1]), 0);
    assert_eq!(max_product_subarray(&[-2]), -2);
}

// ================================================================================================
/// Q9. BEST TIME TO BUY AND SELL STOCK. I: one transaction. II: unlimited transactions. III: at most two.
/// IV: at most k. (You must sell before buying again; the cooldown variant lives in the DP file.)
///   [7,1,5,3,6,4] -> I: 5, II: 7, III: 7
///
/// APPROACH. I: track the minimum price so far, best = max(best, p - min): O(n). II: sum every positive day-to-day
/// rise (greedy equals the optimum because profit decomposes into rises). III/IV: state machine DP with
/// `buy[j]` (best cash while holding during the j-th transaction) and `sell[j]` (best cash after j sells):
/// O(n*k) time, O(k) space; when `k >= n/2` it collapses to II. Brute force: exhaustive recursion over
/// (day, holding, transactions left).
/// RUST. Use `i64` and a large negative sentinel such as `i64::MIN / 2` so adding a price cannot overflow.
/// INTERVIEWER IS TESTING: that you derive III from a state machine, not special-case two passes.
fn stock_one(p: &[i64]) -> i64 {
    let mut min = i64::MAX;
    let mut best = 0;
    for &x in p {
        min = min.min(x);
        best = best.max(x - min);
    }
    best
}
fn stock_two(p: &[i64]) -> i64 {
    p.windows(2).map(|w| (w[1] - w[0]).max(0)).sum()
}
fn stock_k(p: &[i64], k: usize) -> i64 {
    if k >= p.len() / 2 {
        return stock_two(p);
    }
    let neg = i64::MIN / 2;
    let (mut buy, mut sell) = (vec![neg; k + 1], vec![0i64; k + 1]);
    for &x in p {
        for j in 1..=k {
            buy[j] = buy[j].max(sell[j - 1] - x);
            sell[j] = sell[j].max(buy[j] + x);
        }
    }
    sell[k]
}
fn stock_brute(p: &[i64], i: usize, holding: bool, left: usize) -> i64 {
    if i == p.len() {
        return 0;
    }
    let skip = stock_brute(p, i + 1, holding, left);
    if holding {
        skip.max(p[i] + stock_brute(p, i + 1, false, left))
    } else if left > 0 {
        skip.max(-p[i] + stock_brute(p, i + 1, true, left - 1))
    } else {
        skip
    }
}
fn q9() {
    let mut rng = Rng::new(9);
    for _ in 0..CASES {
        let n = rng.below(10);
        let p = rng.vec(n, 0, 9);
        assert_eq!(stock_one(&p), stock_brute(&p, 0, false, 1), "{p:?}");
        assert_eq!(
            stock_two(&p),
            stock_brute(&p, 0, false, usize::MAX),
            "{p:?}"
        );
        for k in [2usize, 3] {
            assert_eq!(stock_k(&p, k), stock_brute(&p, 0, false, k), "k={k} {p:?}");
        }
    }
    let p = [7, 1, 5, 3, 6, 4];
    assert_eq!((stock_one(&p), stock_two(&p), stock_k(&p, 2)), (5, 7, 7));
    assert_eq!(stock_one(&[7, 6, 4, 3, 1]), 0);
}

// ================================================================================================
/// Q10. MERGE INTERVALS; INSERT INTERVAL; NON-OVERLAPPING INTERVALS.
///   merge [[1,3],[2,6],[8,10],[15,18]] -> [[1,6],[8,10],[15,18]]
///   insert [[1,3],[6,9]] + [2,5] -> [[1,5],[6,9]];  erase-overlap [[1,2],[2,3],[3,4],[1,3]] -> remove 1
///
/// APPROACH. Merge: sort by start, sweep keeping the last output interval; if `start <= last.end` extend,
/// else push. O(n log n). Insert (already sorted and disjoint): one linear pass in three phases (before / overlapping
/// / after), O(n), no sort. Min removals so the rest do not overlap: the interval-scheduling greedy: sort by END,
/// keep an interval iff its start >= last kept end; answer = n - kept. Pattern: sort + sweep / greedy by earliest finish.
/// RUST. `sort_unstable_by_key(|&(s, _)| s)`; `out.last_mut()` gives `Option<&mut (i64, i64)>` to extend in place.
/// Touching intervals ([1,2],[2,3]) merge in Merge Intervals but do NOT overlap in Erase Overlap: read the statement.
/// INTERVIEWER IS TESTING: the sort key, the `<=` vs `<` boundary, and proving the earliest-end greedy.
type Iv = (i64, i64);
fn merge_intervals(mut v: Vec<Iv>) -> Vec<Iv> {
    v.sort_unstable_by_key(|&(s, _)| s);
    let mut out: Vec<Iv> = Vec::new();
    for (s, e) in v {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}
fn insert_interval(v: &[Iv], new: Iv) -> Vec<Iv> {
    let (mut out, mut cur, mut i) = (vec![], new, 0);
    while i < v.len() && v[i].1 < cur.0 {
        out.push(v[i]);
        i += 1;
    }
    while i < v.len() && v[i].0 <= cur.1 {
        cur = (cur.0.min(v[i].0), cur.1.max(v[i].1));
        i += 1;
    }
    out.push(cur);
    out.extend_from_slice(&v[i..]);
    out
}
fn erase_overlap_intervals(mut v: Vec<Iv>) -> usize {
    v.sort_unstable_by_key(|&(_, e)| e);
    let (mut kept, mut end) = (0, i64::MIN);
    for (s, e) in v.iter().copied() {
        if s >= end {
            kept += 1;
            end = e;
        }
    }
    v.len() - kept
}
fn merge_oracle(mut v: Vec<Iv>) -> Vec<Iv> {
    // repeat "merge any overlapping pair" to a fixpoint; no sorting trick involved
    loop {
        let mut changed = false;
        'scan: for i in 0..v.len() {
            for j in i + 1..v.len() {
                if v[i].0.max(v[j].0) <= v[i].1.min(v[j].1) {
                    v[i] = (v[i].0.min(v[j].0), v[i].1.max(v[j].1));
                    v.remove(j);
                    changed = true;
                    break 'scan;
                }
            }
        }
        if !changed {
            v.sort_unstable();
            return v;
        }
    }
}
fn q10() {
    let mut rng = Rng::new(10);
    let gen_iv = |rng: &mut Rng| -> Iv {
        let s = rng.int(0, 15);
        (s, s + rng.int(1, 4)) // non-empty intervals
    };
    for _ in 0..CASES {
        let n = rng.below(8);
        let v: Vec<Iv> = (0..n).map(|_| gen_iv(&mut rng)).collect();
        assert_eq!(merge_intervals(v.clone()), merge_oracle(v.clone()), "{v:?}");
        let base = merge_oracle(v.clone());
        let new = gen_iv(&mut rng);
        let mut with_new = base.clone();
        with_new.push(new);
        assert_eq!(
            insert_interval(&base, new),
            merge_oracle(with_new),
            "{base:?} + {new:?}"
        );
        // erase-overlap oracle: best subset with pairwise s2 >= e1 (touching is fine); n <= 8 so 2^n subsets
        let mut best_kept = 0;
        for mask in 0u32..(1 << n) {
            let mut chosen: Vec<Iv> = (0..n)
                .filter(|&i| mask >> i & 1 == 1)
                .map(|i| v[i])
                .collect();
            chosen.sort_unstable_by_key(|&(s, e)| (e, s));
            let ok = chosen.windows(2).all(|w| w[1].0 >= w[0].1);
            if ok {
                best_kept = best_kept.max(chosen.len());
            }
        }
        assert_eq!(erase_overlap_intervals(v.clone()), n - best_kept, "{v:?}");
    }
    assert_eq!(
        merge_intervals(vec![(1, 3), (2, 6), (8, 10), (15, 18)]),
        vec![(1, 6), (8, 10), (15, 18)]
    );
    assert_eq!(
        insert_interval(&[(1, 3), (6, 9)], (2, 5)),
        vec![(1, 5), (6, 9)]
    );
    assert_eq!(
        erase_overlap_intervals(vec![(1, 2), (2, 3), (3, 4), (1, 3)]),
        1
    );
}

// ================================================================================================
/// Q11. ROTATE ARRAY (right by k, in place, O(1) space); MOVE ZEROES (stable, in place); REMOVE DUPLICATES from a
/// sorted array in place (keep each value at most once; follow-up: at most twice) and return the new length.
///   rotate [1..7] k=3 -> [5,6,7,1,2,3,4];  move [0,1,0,3,12] -> [1,3,12,0,0];  dedupe [0,0,1,1,1,2] -> 3
///
/// APPROACH. Rotate: extra array O(n) space, or THREE REVERSALS: reverse all, reverse the first k, reverse the
/// rest. O(n) time, O(1) space. Move zeroes and dedupe use the READ/WRITE (slow/fast) two-pointer pattern: `write`
/// marks the end of the kept prefix, `read` scans. Allow-at-most-m duplicates generalises by comparing
/// `nums[read]` with `nums[write - m]`.
/// RUST. `k %= n` first (and guard n == 0: `%` by zero panics). `slice.reverse()`, `nums[..k].reverse()`,
/// `nums.swap(i, j)`. Know that std has `rotate_right`, but the interviewer wants the reversal trick.
/// INTERVIEWER IS TESTING: k > n, empty input, and minimising writes in Move Zeroes.
fn rotate_right(nums: &mut [i64], k: usize) {
    let n = nums.len();
    if n == 0 {
        return;
    }
    let k = k % n;
    nums.reverse();
    nums[..k].reverse();
    nums[k..].reverse();
}
fn move_zeroes(nums: &mut [i64]) {
    let mut write = 0;
    for read in 0..nums.len() {
        if nums[read] != 0 {
            nums.swap(write, read);
            write += 1;
        }
    }
}
fn dedup_at_most(nums: &mut [i64], m: usize) -> usize {
    let mut write = 0;
    for read in 0..nums.len() {
        if write < m || nums[read] != nums[write - m] {
            nums[write] = nums[read];
            write += 1;
        }
    }
    write
}
fn q11() {
    let mut rng = Rng::new(11);
    for _ in 0..CASES {
        let n = rng.below(10);
        let v = rng.vec(n, 0, 3);
        let k = rng.below(25);
        let mut a = v.clone();
        rotate_right(&mut a, k);
        let mut want = vec![0; n];
        for i in 0..n {
            want[(i + k) % n] = v[i];
        }
        assert_eq!(a, want, "{v:?} k={k}");
        let mut z = v.clone();
        move_zeroes(&mut z);
        let mut want: Vec<i64> = v.iter().copied().filter(|&x| x != 0).collect();
        want.resize(n, 0);
        assert_eq!(z, want);
        let mut s = v.clone();
        s.sort_unstable();
        for m in [1usize, 2] {
            let mut t = s.clone();
            let len = dedup_at_most(&mut t, m);
            let mut want = vec![];
            for &x in &s {
                if want.iter().filter(|&&y| y == x).count() < m {
                    want.push(x);
                }
            }
            assert_eq!(&t[..len], &want[..], "m={m} {s:?}");
        }
    }
    let mut a: Vec<i64> = (1..=7).collect();
    rotate_right(&mut a, 3);
    assert_eq!(a, vec![5, 6, 7, 1, 2, 3, 4]);
    rotate_right(&mut [], 5);
}

// ================================================================================================
/// Q12. CONTAINER WITH MOST WATER and TRAPPING RAIN WATER (both: heights array, two pointers).
///   container [1,8,6,2,5,4,8,3,7] -> 49;  trap [0,1,0,2,1,0,1,3,2,1,2,1] -> 6
///
/// APPROACH. Container: brute force all pairs O(n^2). Two pointers from the ends: area = width * min(h[l], h[r]);
/// move the SHORTER side inward (moving the taller can only shrink width and not raise the min). O(n), O(1).
/// Trap: brute force per bar: water = min(max-left, max-right) - h, O(n^2) or O(n) with prefix/suffix max arrays
/// (O(n) space). Optimal two pointers: keep `left_max`, `right_max`; the side with the smaller current wall is
/// bounded by its own running max, so settle that side and advance. O(n), O(1). (A monotonic stack also works.)
/// RUST. Use `usize` heights; `lm - h[l]` never underflows because `lm >= h[l]` after the `max` update.
/// Return 0 for len < 3 before computing `len - 1`.
/// INTERVIEWER IS TESTING: can you justify "move the shorter pointer", and can you explain trap without memorising.
fn max_area(h: &[usize]) -> usize {
    if h.len() < 2 {
        return 0;
    }
    let (mut l, mut r, mut best) = (0, h.len() - 1, 0);
    while l < r {
        best = best.max((r - l) * h[l].min(h[r]));
        if h[l] < h[r] { l += 1 } else { r -= 1 }
    }
    best
}
fn trap(h: &[usize]) -> usize {
    if h.len() < 3 {
        return 0;
    }
    let (mut l, mut r) = (0, h.len() - 1);
    let (mut lm, mut rm, mut water) = (0, 0, 0);
    while l < r {
        if h[l] < h[r] {
            lm = lm.max(h[l]);
            water += lm - h[l];
            l += 1;
        } else {
            rm = rm.max(h[r]);
            water += rm - h[r];
            r -= 1;
        }
    }
    water
}
fn q12() {
    let mut rng = Rng::new(12);
    for _ in 0..CASES {
        let n = rng.below(12);
        let h: Vec<usize> = (0..n).map(|_| rng.below(7)).collect();
        let mut want = 0;
        for i in 0..n {
            for j in i + 1..n {
                want = want.max((j - i) * h[i].min(h[j]));
            }
        }
        assert_eq!(max_area(&h), want, "{h:?}");
        let mut water = 0;
        for i in 0..n {
            let left = h[..=i].iter().max().unwrap();
            let right = h[i..].iter().max().unwrap();
            water += left.min(right) - h[i];
        }
        assert_eq!(trap(&h), water, "{h:?}");
    }
    assert_eq!(max_area(&[1, 8, 6, 2, 5, 4, 8, 3, 7]), 49);
    assert_eq!(trap(&[0, 1, 0, 2, 1, 0, 1, 3, 2, 1, 2, 1]), 6);
}

// ================================================================================================
/// Q13. SUBARRAY SUM EQUALS K. Count contiguous subarrays whose sum is exactly k (values may be negative).
///   [1,1,1], k = 2 -> 2;  [1,2,3], k = 3 -> 2
///
/// APPROACH. Brute force O(n^2) (O(n^3) naive). Negatives break the sliding window, so use PREFIX SUMS: a
/// subarray (j, i] sums to k iff `prefix[i] - prefix[j] == k`. Keep a hash map from prefix value to how many times
/// it has occurred; at each i add `count[prefix - k]`. Seed the map with `{0: 1}` for subarrays starting at index 0.
/// O(n) time and space. Same trick: contiguous subarray sums divisible by k (key = prefix mod k).
/// RUST. `counts.get(&(p - k)).copied().unwrap_or(0)`; insert the current prefix AFTER the lookup (k = 0 would
/// otherwise count the empty subarray). For `% k` with negatives use `rem_euclid`.
/// INTERVIEWER IS TESTING: why the window fails with negatives and why `{0:1}` is seeded.
fn subarray_sum_k(nums: &[i64], k: i64) -> usize {
    let mut counts: HashMap<i64, usize> = HashMap::from([(0, 1)]);
    let (mut prefix, mut ans) = (0, 0);
    for &x in nums {
        prefix += x;
        ans += counts.get(&(prefix - k)).copied().unwrap_or(0);
        *counts.entry(prefix).or_insert(0) += 1;
    }
    ans
}
fn q13() {
    let mut rng = Rng::new(13);
    for _ in 0..CASES {
        let n = rng.below(12);
        let v = rng.vec(n, -3, 3);
        let k = rng.int(-4, 4);
        let mut want = 0;
        for l in 0..n {
            for r in l + 1..=n {
                if v[l..r].iter().sum::<i64>() == k {
                    want += 1;
                }
            }
        }
        assert_eq!(subarray_sum_k(&v, k), want, "{v:?} k={k}");
    }
    assert_eq!(subarray_sum_k(&[1, 1, 1], 2), 2);
    assert_eq!(subarray_sum_k(&[0, 0, 0], 0), 6);
}

// ================================================================================================
/// Q14. LONGEST CONSECUTIVE SEQUENCE. Length of the longest run of consecutive integers (any order, duplicates
/// allowed) in O(n).   [100,4,200,1,3,2] -> 4 (1,2,3,4)
///
/// APPROACH. Sort, dedupe and scan runs: O(n log n). Required O(n): put everything in a `HashSet`; only START
/// counting at a value `x` whose predecessor `x - 1` is absent (a run's first element), then walk upward. Each
/// element is visited at most twice overall, so O(n) time, O(n) space. Alternative: union-find / hash of run ends.
/// RUST. Iterate the SET (not the Vec) so duplicates do not cause repeated work; the `x - 1` check is the
/// whole trick. Use `i64` so `x - 1` cannot underflow at `i32::MIN`-like inputs.
/// INTERVIEWER IS TESTING: do you see why the nested loop is not O(n^2), and can you say it aloud.
fn longest_consecutive(nums: &[i64]) -> usize {
    let set: HashSet<i64> = nums.iter().copied().collect();
    let mut best = 0;
    for &x in &set {
        if !set.contains(&(x - 1)) {
            let mut y = x;
            while set.contains(&(y + 1)) {
                y += 1;
            }
            best = best.max((y - x + 1) as usize);
        }
    }
    best
}
fn q14() {
    let mut rng = Rng::new(14);
    for _ in 0..CASES {
        let n = rng.below(14);
        let v = rng.vec(n, -6, 6);
        let mut s = v.clone();
        s.sort_unstable();
        s.dedup();
        let mut want = 0;
        let mut run = 0;
        for i in 0..s.len() {
            run = if i > 0 && s[i] == s[i - 1] + 1 {
                run + 1
            } else {
                1
            };
            want = want.max(run);
        }
        assert_eq!(longest_consecutive(&v), want, "{v:?}");
    }
    assert_eq!(longest_consecutive(&[100, 4, 200, 1, 3, 2]), 4);
    assert_eq!(longest_consecutive(&[]), 0);
}

// ================================================================================================
/// Q15. LONGEST SUBSTRING WITHOUT REPEATING CHARACTERS.   "abcabcbb" -> 3;  "bbbbb" -> 1;  "pwwkew" -> 3
///
/// APPROACH. Brute force: every substring with a set, O(n^2)-O(n^3). SLIDING WINDOW: keep `start` of the window;
/// remember the last index of each byte; when `s[i]` was last seen at or after `start`, jump `start` to
/// `last + 1`. O(n) time, O(alphabet) space. Pattern: variable-size sliding window with a jump.
/// RUST. WRINKLE: strings are UTF-8. `s.as_bytes()` is right for ASCII; for Unicode iterate `s.chars()` (or grapheme
/// clusters, which need a crate) and key a `HashMap<char, usize>`. Never `s[i]` index a `String`. Store last-seen
/// as `Option<usize>` (or index + 1) so index 0 is not confused with "unseen".
/// INTERVIEWER IS TESTING: the `last >= start` guard (forgetting it lets old positions shrink the window), and
/// byte-vs-char awareness.
fn longest_unique_substring(s: &str) -> usize {
    let mut last: [Option<usize>; 256] = [None; 256];
    let (mut start, mut best) = (0, 0);
    for (i, &b) in s.as_bytes().iter().enumerate() {
        if let Some(p) = last[b as usize] {
            if p >= start {
                start = p + 1;
            }
        }
        last[b as usize] = Some(i);
        best = best.max(i + 1 - start);
    }
    best
}
fn longest_unique_substring_chars(s: &str) -> usize {
    let mut last: HashMap<char, usize> = HashMap::new();
    let (mut start, mut best) = (0, 0);
    for (i, c) in s.chars().enumerate() {
        if let Some(&p) = last.get(&c) {
            start = start.max(p + 1);
        }
        last.insert(c, i);
        best = best.max(i + 1 - start);
    }
    best
}
fn q15() {
    let mut rng = Rng::new(15);
    for _ in 0..CASES {
        let s = rng.string("abcd", 12);
        let b = s.as_bytes();
        let mut want = 0;
        for l in 0..b.len() {
            for r in l..b.len() {
                let uniq: HashSet<u8> = b[l..=r].iter().copied().collect();
                if uniq.len() == r - l + 1 {
                    want = want.max(r - l + 1);
                }
            }
        }
        assert_eq!(longest_unique_substring(&s), want, "{s:?}");
        assert_eq!(longest_unique_substring_chars(&s), want);
    }
    assert_eq!(longest_unique_substring("pwwkew"), 3);
    assert_eq!(longest_unique_substring("abba"), 2); // the guard case: 'a' last seen BEFORE start
    assert_eq!(longest_unique_substring_chars("aé€éb"), 3); // "é€" then "€éb"
}

// ================================================================================================
/// Q16. MINIMUM WINDOW SUBSTRING. Smallest substring of `s` containing every character of `t` with multiplicity;
/// "" if none.   s = "ADOBECODEBANC", t = "ABC" -> "BANC"
///
/// APPROACH. Brute force: all substrings, check coverage, O(n^2 * m). SLIDING WINDOW: `need[c]` counts how many
/// more copies of c are required (can go negative = surplus) and `missing` = total still required. Extend `right`
/// (decrement need, and missing if it was positive), then while `missing == 0` record the window and shrink `left`
/// (increment need; if it becomes positive, missing rises). O(n + m), O(alphabet).
/// RUST. Return `Option<&str>` borrowing from `s` (zero copies); byte-slice `&s[l..r]` is safe here only because
/// the alphabet is ASCII (`is_ascii` guard in real code). `t` empty -> return the empty window up front.
/// INTERVIEWER IS TESTING: expand/shrink loop structure, handling duplicates in t, and leftmost tie-breaking.
fn min_window<'a>(s: &'a str, t: &str) -> Option<&'a str> {
    if t.is_empty() {
        return Some(&s[..0]);
    }
    let sb = s.as_bytes();
    let mut need = [0i32; 256];
    for b in t.bytes() {
        need[b as usize] += 1;
    }
    let (mut missing, mut l) = (t.len(), 0);
    let mut best: Option<(usize, usize)> = None;
    for r in 0..sb.len() {
        if need[sb[r] as usize] > 0 {
            missing -= 1;
        }
        need[sb[r] as usize] -= 1;
        while missing == 0 {
            if best.is_none_or(|(bl, br)| r + 1 - l < br - bl) {
                best = Some((l, r + 1));
            }
            need[sb[l] as usize] += 1;
            if need[sb[l] as usize] > 0 {
                missing += 1;
            }
            l += 1;
        }
    }
    best.map(|(l, r)| &s[l..r])
}
fn min_window_brute<'a>(s: &'a str, t: &str) -> Option<&'a str> {
    for len in t.len()..=s.len() {
        for st in 0..=s.len() - len {
            let w = &s[st..st + len];
            let mut cnt: HashMap<u8, i32> = HashMap::new();
            for b in w.bytes() {
                *cnt.entry(b).or_insert(0) += 1;
            }
            if t.bytes().all(|b| {
                let e = cnt.entry(b).or_insert(0);
                *e -= 1;
                *e >= 0
            }) {
                return Some(w);
            }
        }
    }
    None
}
fn q16() {
    let mut rng = Rng::new(16);
    for _ in 0..CASES {
        let s = rng.string("abc", 10);
        let t = rng.string("abc", 4);
        assert_eq!(min_window(&s, &t), min_window_brute(&s, &t), "{s:?} {t:?}");
    }
    assert_eq!(min_window("ADOBECODEBANC", "ABC"), Some("BANC"));
    assert_eq!(min_window("a", "aa"), None);
}

// ================================================================================================
/// Q17. LONGEST REPEATING CHARACTER REPLACEMENT (replace at most k letters to get the longest run of one letter)
/// and PERMUTATION IN STRING (does `s2` contain a permutation of `s1`?).
///   "AABABBA", k = 1 -> 4;   s1 = "ab", s2 = "eidbaooo" -> true
///
/// APPROACH. Replacement: a window is valid when `len - max_freq <= k`. Slide `right`; if invalid, advance `left`
/// by ONE (the window never shrinks, it just slides), which is correct because `max_freq` only needs to be
/// historic: the answer only grows when a truly better frequency appears. O(n), O(26). Permutation: FIXED-size
/// window of `s1.len()`, maintain counts difference; compare in O(26) per step (or track a `matches` counter).
/// Brute: every window sorted and compared, O(n * m log m).
/// RUST. Counts as `[usize; 26]`; compare arrays directly with `==`. `(r + 1 - l) - maxf` cannot underflow
/// because the window length is never smaller than any historic max frequency.
/// INTERVIEWER IS TESTING: the non-shrinking window argument; the fixed window for permutations.
fn character_replacement(s: &str, k: usize) -> usize {
    let b = s.as_bytes();
    let (mut cnt, mut l, mut maxf, mut best) = ([0usize; 26], 0, 0, 0);
    for r in 0..b.len() {
        let i = (b[r] - b'A') as usize;
        cnt[i] += 1;
        maxf = maxf.max(cnt[i]);
        if r + 1 - l - maxf > k {
            cnt[(b[l] - b'A') as usize] -= 1;
            l += 1;
        }
        best = best.max(r + 1 - l);
    }
    best
}
fn check_inclusion(s1: &str, s2: &str) -> bool {
    let (a, b) = (s1.as_bytes(), s2.as_bytes());
    if a.len() > b.len() {
        return false;
    }
    let (mut ca, mut cb) = ([0i32; 26], [0i32; 26]);
    for i in 0..a.len() {
        ca[(a[i] - b'a') as usize] += 1;
        cb[(b[i] - b'a') as usize] += 1;
    }
    if ca == cb {
        return true;
    }
    for i in a.len()..b.len() {
        cb[(b[i] - b'a') as usize] += 1;
        cb[(b[i - a.len()] - b'a') as usize] -= 1;
        if ca == cb {
            return true;
        }
    }
    false
}
fn q17() {
    let mut rng = Rng::new(17);
    for _ in 0..CASES {
        let s = rng.string("ABC", 11);
        let k = rng.below(4);
        let b = s.as_bytes();
        let mut want = 0;
        for l in 0..b.len() {
            for r in l..b.len() {
                let mut c = [0usize; 3];
                b[l..=r].iter().for_each(|&x| c[(x - b'A') as usize] += 1);
                if r - l + 1 - c.iter().max().unwrap() <= k {
                    want = want.max(r - l + 1);
                }
            }
        }
        assert_eq!(character_replacement(&s, k), want, "{s:?} k={k}");
        let (p, t) = (rng.string("abc", 3), rng.string("abc", 9));
        let mut ps: Vec<u8> = p.bytes().collect();
        ps.sort_unstable();
        let want = t.len() >= p.len()
            && (0..=t.len() - p.len()).any(|i| {
                let mut w: Vec<u8> = t.as_bytes()[i..i + p.len()].to_vec();
                w.sort_unstable();
                w == ps
            });
        assert_eq!(check_inclusion(&p, &t), want, "{p:?} in {t:?}");
    }
    assert_eq!(character_replacement("AABABBA", 1), 4);
    assert!(check_inclusion("ab", "eidbaooo"));
    assert!(!check_inclusion("ab", "eidboaoo"));
}

// ================================================================================================
/// Q18. LONGEST PALINDROMIC SUBSTRING, and COUNT PALINDROMIC SUBSTRINGS.   "babad" -> "bab"/"aba";  "aaa" -> 6 substrings
///
/// APPROACH. Brute force O(n^3) (check every substring). DP table O(n^2) time and space. EXPAND AROUND CENTRE:
/// 2n-1 centres (n odd, n-1 even), expand while the ends match: O(n^2) time, O(1) space; the count question
/// just adds every successful expansion. MANACHER: reuse mirrored radii inside the rightmost palindrome so the
/// total work is linear, O(n) time, O(n) space; `d1[i]` = number of odd palindromes centred at i, `d2[i]` = number
/// of even palindromes centred between i-1 and i. Then count = sum(d1) + sum(d2).
/// RUST. Expanding downwards with `usize` underflows at 0: use `isize` for the left cursor (or `checked_sub`).
/// Work on `&[u8]` for ASCII; for Unicode collect `Vec<char>` first. Return `(start, len)` and slice once.
/// INTERVIEWER IS TESTING: do you check both odd and even centres; do you know Manacher exists (rarely required).
fn longest_pal_expand(s: &[u8]) -> (usize, usize) {
    let n = s.len() as isize;
    let (mut best_start, mut best_len) = (0, 0);
    for c in 0..n {
        for (mut l, mut r) in [(c, c), (c, c + 1)] {
            while l >= 0 && r < n && s[l as usize] == s[r as usize] {
                l -= 1;
                r += 1;
            }
            let len = (r - l - 1) as usize;
            if len > best_len {
                (best_start, best_len) = ((l + 1) as usize, len);
            }
        }
    }
    (best_start, best_len)
}
fn count_palindromes(s: &[u8]) -> usize {
    let n = s.len() as isize;
    let mut total = 0;
    for c in 0..n {
        for (mut l, mut r) in [(c, c), (c, c + 1)] {
            while l >= 0 && r < n && s[l as usize] == s[r as usize] {
                total += 1;
                l -= 1;
                r += 1;
            }
        }
    }
    total
}
fn manacher(s: &[u8]) -> (Vec<usize>, Vec<usize>) {
    let n = s.len() as i64;
    let (mut d1, mut d2) = (vec![0usize; s.len()], vec![0usize; s.len()]);
    let (mut l, mut r) = (0i64, -1i64);
    for i in 0..n {
        let mut k = if i > r {
            1
        } else {
            (d1[(l + r - i) as usize] as i64).min(r - i + 1)
        };
        while i + k < n && i - k >= 0 && s[(i + k) as usize] == s[(i - k) as usize] {
            k += 1;
        }
        d1[i as usize] = k as usize;
        if i + k - 1 > r {
            (l, r) = (i - k + 1, i + k - 1);
        }
    }
    let (mut l, mut r) = (0i64, -1i64);
    for i in 0..n {
        let mut k = if i > r {
            0
        } else {
            (d2[(l + r - i + 1) as usize] as i64).min(r - i + 1)
        };
        while i + k < n && i - k - 1 >= 0 && s[(i + k) as usize] == s[(i - k - 1) as usize] {
            k += 1;
        }
        d2[i as usize] = k as usize;
        if i + k - 1 > r {
            (l, r) = (i - k, i + k - 1);
        }
    }
    (d1, d2)
}
fn longest_pal_manacher(s: &[u8]) -> (usize, usize) {
    let (d1, d2) = manacher(s);
    let (mut start, mut len) = (0, 0);
    for i in 0..s.len() {
        if 2 * d1[i] - 1 > len {
            (start, len) = (i + 1 - d1[i], 2 * d1[i] - 1);
        }
        if 2 * d2[i] > len {
            (start, len) = (i - d2[i], 2 * d2[i]);
        }
    }
    (start, len)
}
fn is_pal(s: &[u8]) -> bool {
    s.iter().eq(s.iter().rev())
}
fn q18() {
    let mut rng = Rng::new(18);
    for _ in 0..CASES {
        let s = rng.string("ab", 14);
        let b = s.as_bytes();
        let (mut want_len, mut want_cnt) = (0, 0);
        for l in 0..b.len() {
            for r in l..b.len() {
                if is_pal(&b[l..=r]) {
                    want_cnt += 1;
                    want_len = want_len.max(r - l + 1);
                }
            }
        }
        for (st, len) in [longest_pal_expand(b), longest_pal_manacher(b)] {
            assert_eq!(len, want_len, "{s:?}");
            assert!(is_pal(&b[st..st + len]));
        }
        assert_eq!(count_palindromes(b), want_cnt);
        let (d1, d2) = manacher(b);
        assert_eq!(
            d1.iter().sum::<usize>() + d2.iter().sum::<usize>(),
            want_cnt
        );
    }
    let (st, len) = longest_pal_expand(b"babad");
    assert_eq!(&b"babad"[st..st + len], b"bab");
    assert_eq!(count_palindromes(b"aaa"), 6);
}

// ================================================================================================
/// Q19. VALID PALINDROME (ignore case and non-alphanumerics) and STRING TO INTEGER (atoi).
///   "A man, a plan, a canal: Panama" -> true;   atoi: "   -042" -> -42, "4193 with words" -> 4193,
///   "words and 987" -> 0, "-91283472332" -> -2147483648 (clamped), "+-12" -> 0
///
/// APPROACH. Palindrome: two pointers from both ends, skipping non-alphanumerics, comparing case-insensitively;
/// O(n) time O(1) space (versus filter + reverse, O(n) space). atoi: (1) skip leading spaces, (2) optional single
/// sign, (3) read digits until the first non-digit, (4) clamp to i32 on overflow, never wrap. Detect overflow
/// BEFORE it happens: accumulate in the sign's direction with `checked_mul`/`checked_add`/`checked_sub` so
/// `i32::MIN` (whose magnitude does not fit in positive i32) works without special cases.
/// RUST. `char::is_alphanumeric` is Unicode-aware, `u8::is_ascii_alphanumeric` is not: say which you mean. Keep the
/// right cursor exclusive to avoid `usize` underflow. Do not use `s.parse::<i32>()` for atoi: it rejects trailing text.
/// INTERVIEWER IS TESTING: the full edge-case list above, and overflow handled without a bigger integer type.
fn is_palindrome_ascii(s: &str) -> bool {
    let b = s.as_bytes();
    let (mut l, mut r) = (0, b.len());
    while l < r {
        if !b[l].is_ascii_alphanumeric() {
            l += 1;
        } else if !b[r - 1].is_ascii_alphanumeric() {
            r -= 1;
        } else if !b[l].eq_ignore_ascii_case(&b[r - 1]) {
            return false;
        } else {
            l += 1;
            r -= 1;
        }
    }
    true
}
fn is_palindrome_unicode(s: &str) -> bool {
    let v: Vec<char> = s
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    v.iter().eq(v.iter().rev())
}
fn my_atoi(s: &str) -> i32 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut acc: i32 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        let d = (b[i] - b'0') as i32;
        let next = acc.checked_mul(10).and_then(|v| {
            if neg {
                v.checked_sub(d)
            } else {
                v.checked_add(d)
            }
        });
        match next {
            Some(v) => acc = v,
            None => return if neg { i32::MIN } else { i32::MAX },
        }
        i += 1;
    }
    acc
}
fn atoi_oracle(s: &str) -> i32 {
    let t = s.trim_start_matches(' ');
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return 0;
    }
    let v: i128 = if digits.len() > 30 {
        i128::MAX / 2
    } else {
        digits.parse().unwrap()
    };
    (if neg { -v } else { v }).clamp(i32::MIN as i128, i32::MAX as i128) as i32
}
fn q19() {
    let mut rng = Rng::new(19);
    for _ in 0..CASES {
        let s = rng.string("aAb1 ,:", 9);
        let cleaned: Vec<u8> = s
            .bytes()
            .filter(|b| b.is_ascii_alphanumeric())
            .map(|b| b.to_ascii_lowercase())
            .collect();
        assert_eq!(is_palindrome_ascii(&s), is_pal(&cleaned), "{s:?}");
        assert_eq!(is_palindrome_unicode(&s), is_pal(&cleaned));
        let t = rng.string("  +-0123456789a", 16);
        assert_eq!(my_atoi(&t), atoi_oracle(&t), "{t:?}");
    }
    assert!(is_palindrome_ascii("A man, a plan, a canal: Panama"));
    assert!(!is_palindrome_ascii("race a car"));
    assert!(is_palindrome_ascii(" ")); // empty after filtering
    assert!(is_palindrome_unicode("Éa, aé"));
    for (s, want) in [
        ("   -042", -42),
        ("4193 with words", 4193),
        ("words and 987", 0),
        ("-91283472332", i32::MIN),
        ("91283472332", i32::MAX),
        ("2147483647", i32::MAX),
        ("2147483648", i32::MAX),
        ("-2147483648", i32::MIN),
        ("-2147483649", i32::MIN),
        ("+-12", 0),
        ("00000-42a1", 0),
        ("", 0),
        ("+", 0),
        ("  +0 123", 0),
    ] {
        assert_eq!(my_atoi(s), want, "{s:?}");
    }
}

// ================================================================================================
/// Q20. INTEGER TO ROMAN and ROMAN TO INTEGER (1..=3999).   1994 <-> "MCMXCIV";  58 <-> "LVIII"
///
/// APPROACH. Int to roman: GREEDY over a descending table that INCLUDES the subtractive pairs
/// (900 CM, 400 CD, 90 XC, 40 XL, 9 IX, 4 IV); repeatedly take the largest value that fits. O(1) (bounded
/// output). Roman to int: scan left to right; if the current symbol is smaller than the NEXT one, subtract it,
/// else add it. O(n). Pattern: greedy with a table, local comparison rule.
/// RUST. Table as `[(u32, &str); 13]` const; build with `String::push_str`. Bounds-check `i + 1 < n` before peeking.
/// INTERVIEWER IS TESTING: that you put subtractive pairs IN the table (avoids special-case code) and the look-ahead.
const ROMAN: [(u32, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];
fn int_to_roman(mut n: u32) -> String {
    let mut s = String::new();
    for &(v, sym) in &ROMAN {
        while n >= v {
            s.push_str(sym);
            n -= v;
        }
    }
    s
}
fn roman_to_int(s: &str) -> i32 {
    let val = |c: u8| match c {
        b'I' => 1,
        b'V' => 5,
        b'X' => 10,
        b'L' => 50,
        b'C' => 100,
        b'D' => 500,
        b'M' => 1000,
        _ => 0,
    };
    let b = s.as_bytes();
    let mut total = 0;
    for i in 0..b.len() {
        let v = val(b[i]);
        if i + 1 < b.len() && v < val(b[i + 1]) {
            total -= v
        } else {
            total += v
        }
    }
    total
}
fn roman_oracle(s: &str) -> i32 {
    // independent method: rewrite subtractive pairs additively, then just sum the symbols
    let t = s
        .replace("CM", "DCCCC")
        .replace("CD", "CCCC")
        .replace("XC", "LXXXX")
        .replace("XL", "XXXX")
        .replace("IX", "VIIII")
        .replace("IV", "IIII");
    t.bytes()
        .map(|c| {
            ROMAN
                .iter()
                .find(|(_, sym)| sym.len() == 1 && sym.as_bytes()[0] == c)
                .unwrap()
                .0 as i32
        })
        .sum()
}
fn q20() {
    for n in 1..=3999u32 {
        let r = int_to_roman(n);
        assert_eq!(roman_to_int(&r), n as i32, "{r}");
        assert_eq!(roman_oracle(&r), n as i32);
    }
    assert_eq!(int_to_roman(1994), "MCMXCIV");
    assert_eq!(int_to_roman(58), "LVIII");
    assert_eq!(roman_to_int("MCMXCIV"), 1994);
}

// ================================================================================================
/// Q21. STRING UTILITIES: LONGEST COMMON PREFIX; implement strStr (KMP); REVERSE WORDS IN A STRING (trim and
/// collapse spaces); ENCODE AND DECODE STRINGS (a list of strings <-> one string, any content).
///   ["flower","flow","flight"] -> "fl";  strStr("sadbutsad","sad") -> 0;  "  hello   world " -> "world hello"
///
/// APPROACH. LCP: take the first string as the candidate and shorten it against each other string,
/// O(total chars); horizontal vs vertical scan are both fine. strStr: brute force O(n*m); KMP computes a
/// failure (prefix) function so the text pointer never moves backwards: O(n + m). Reverse words: split on
/// whitespace and reverse (O(n) extra) or in place: collapse spaces, reverse the whole thing, then reverse each
/// word. Encode/decode: LENGTH-PREFIX each string (`5#hello`), never rely on a delimiter that can occur in the data.
/// RUST. `split_whitespace().rev()` handles every whitespace run for free. Lengths are BYTE lengths (`str::len`), so
/// slicing at the decoded offsets is always on a char boundary even for non-ASCII payloads. LCP must cut at a
/// char boundary: sum `len_utf8` of the matched chars rather than counting chars.
/// INTERVIEWER IS TESTING: empty list vs a list with one empty string, delimiter in payload, KMP failure-function logic.
fn longest_common_prefix(strs: &[&str]) -> String {
    let Some(first) = strs.first() else {
        return String::new();
    };
    let mut prefix: &str = first;
    for s in &strs[1..] {
        let common: usize = prefix
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        prefix = &prefix[..common];
    }
    prefix.to_string()
}
fn prefix_function(p: &[u8]) -> Vec<usize> {
    let mut pi = vec![0; p.len()];
    for i in 1..p.len() {
        let mut k = pi[i - 1];
        while k > 0 && p[i] != p[k] {
            k = pi[k - 1];
        }
        if p[i] == p[k] {
            k += 1;
        }
        pi[i] = k;
    }
    pi
}
fn str_str(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() {
        return Some(0);
    }
    let pi = prefix_function(n);
    let mut k = 0;
    for (i, &c) in h.iter().enumerate() {
        while k > 0 && c != n[k] {
            k = pi[k - 1];
        }
        if c == n[k] {
            k += 1;
        }
        if k == n.len() {
            return Some(i + 1 - k);
        }
    }
    None
}
fn reverse_words_inplace(s: &str) -> String {
    let mut b: Vec<u8> = Vec::with_capacity(s.len());
    for &c in s.as_bytes() {
        // collapse runs of spaces and drop leading ones while copying
        if c != b' ' || b.last().is_some_and(|&p| p != b' ') {
            b.push(c);
        }
    }
    if b.last() == Some(&b' ') {
        b.pop();
    }
    b.reverse();
    let mut start = 0;
    for i in 0..=b.len() {
        if i == b.len() || b[i] == b' ' {
            b[start..i].reverse();
            start = i + 1;
        }
    }
    String::from_utf8(b).unwrap()
}
fn encode(strs: &[String]) -> String {
    let mut out = String::new();
    for s in strs {
        out.push_str(&s.len().to_string());
        out.push('#');
        out.push_str(s);
    }
    out
}
fn decode(s: &str) -> Vec<String> {
    let (b, mut i, mut out) = (s.as_bytes(), 0, vec![]);
    while i < b.len() {
        let hash = i + b[i..].iter().position(|&c| c == b'#').unwrap();
        let len: usize = s[i..hash].parse().unwrap();
        out.push(s[hash + 1..hash + 1 + len].to_string());
        i = hash + 1 + len;
    }
    out
}
fn q21() {
    let mut rng = Rng::new(21);
    for _ in 0..CASES {
        let words: Vec<String> = (0..rng.below(4)).map(|_| rng.string("ab", 5)).collect();
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        let want = if refs.is_empty() {
            String::new()
        } else {
            (0..=refs[0].len())
                .rev()
                .map(|l| &refs[0][..l])
                .find(|p| refs.iter().all(|w| w.starts_with(p)))
                .unwrap()
                .to_string()
        };
        assert_eq!(longest_common_prefix(&refs), want);
        let (h, n) = (rng.string("ab", 14), rng.string("ab", 5));
        assert_eq!(str_str(&h, &n), h.find(&n), "{h:?} {n:?}");
        let sentence = rng.string("ab  ", 14);
        assert_eq!(
            reverse_words_inplace(&sentence),
            sentence
                .split_whitespace()
                .rev()
                .collect::<Vec<_>>()
                .join(" "),
            "{sentence:?}"
        );
        let list: Vec<String> = (0..rng.below(5)).map(|_| rng.string("#1a é", 6)).collect();
        assert_eq!(decode(&encode(&list)), list, "{list:?}");
    }
    assert_eq!(longest_common_prefix(&["flower", "flow", "flight"]), "fl");
    assert_eq!(longest_common_prefix(&["dog", "car"]), "");
    assert_eq!(longest_common_prefix(&["é1", "é2"]), "é");
    assert_eq!(decode(&encode(&[String::new()])), vec![String::new()]);
    assert_eq!(decode(&encode(&[])), Vec::<String>::new());
    assert_eq!(prefix_function(b"aabaaab"), vec![0, 1, 0, 1, 2, 2, 3]);
}

// ================================================================================================
/// Q22. MATRIX: SPIRAL MATRIX (return elements in spiral order), ROTATE IMAGE (n x n, 90 degrees clockwise, in
/// place), SET MATRIX ZEROES (if a cell is 0 set its whole row and column to 0, O(1) extra space).
///   [[1,2,3],[4,5,6],[7,8,9]] -> spiral [1,2,3,6,9,8,7,4,5];  rotated [[7,4,1],[8,5,2],[9,6,3]]
///
/// APPROACH. Spiral: shrink four boundaries (top, bottom, left, right); after each side re-check that the
/// remaining rectangle is non-empty (or the single row/column gets visited twice). O(mn). Rotate: TRANSPOSE then
/// reverse each row (counter-clockwise: reverse rows first or transpose along the anti-diagonal), O(n^2), O(1).
/// Set zeroes: use the first row and first column as the marker arrays, remembering separately whether they
/// themselves contained a zero: O(mn) time, O(1) space (the O(m+n) version uses two bool vectors).
/// RUST. Boundaries go negative (right - 1 at 0), so use `i32`/`isize` and cast at indexing, or `checked_sub`.
/// Swapping `m[i][j]` with `m[j][i]` needs two `&mut` into the same Vec: copy through a temp (cells are `Copy`) or
/// use `split_at_mut`; `std::mem::swap(&mut m[i][j], &mut m[j][i])` does not compile.
/// INTERVIEWER IS TESTING: non-square shapes, single row/column, and the first-row/column flag subtlety.
fn spiral_order(m: &[Vec<i32>]) -> Vec<i32> {
    if m.is_empty() {
        return vec![];
    }
    let (mut top, mut bot) = (0i32, m.len() as i32 - 1);
    let (mut left, mut right) = (0i32, m[0].len() as i32 - 1);
    let mut out = vec![];
    while top <= bot && left <= right {
        for c in left..=right {
            out.push(m[top as usize][c as usize]);
        }
        top += 1;
        for r in top..=bot {
            out.push(m[r as usize][right as usize]);
        }
        right -= 1;
        if top <= bot {
            for c in (left..=right).rev() {
                out.push(m[bot as usize][c as usize]);
            }
            bot -= 1;
        }
        if left <= right {
            for r in (top..=bot).rev() {
                out.push(m[r as usize][left as usize]);
            }
            left += 1;
        }
    }
    out
}
fn spiral_oracle(m: &[Vec<i32>]) -> Vec<i32> {
    let (rows, cols) = (m.len(), m[0].len());
    let mut seen = vec![vec![false; cols]; rows];
    let dirs: [(isize, isize); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
    let (mut r, mut c, mut d, mut out) = (0usize, 0usize, 0, vec![]);
    for _ in 0..rows * cols {
        out.push(m[r][c]);
        seen[r][c] = true;
        let step = |d: usize, r: usize, c: usize| -> Option<(usize, usize)> {
            let nr = r.checked_add_signed(dirs[d].0)?;
            let nc = c.checked_add_signed(dirs[d].1)?;
            (nr < rows && nc < cols && !seen[nr][nc]).then_some((nr, nc))
        };
        if step(d, r, c).is_none() {
            d = (d + 1) % 4;
        }
        if let Some((nr, nc)) = step(d, r, c) {
            (r, c) = (nr, nc);
        }
    }
    out
}
fn rotate_image(m: &mut [Vec<i32>]) {
    let n = m.len();
    for i in 0..n {
        for j in i + 1..n {
            let t = m[i][j];
            m[i][j] = m[j][i];
            m[j][i] = t;
        }
    }
    m.iter_mut().for_each(|row| row.reverse());
}
fn set_zeroes(m: &mut [Vec<i32>]) {
    let (rows, cols) = (m.len(), m[0].len());
    let first_row_zero = m[0].contains(&0);
    let first_col_zero = m.iter().any(|r| r[0] == 0);
    for i in 1..rows {
        for j in 1..cols {
            if m[i][j] == 0 {
                m[i][0] = 0;
                m[0][j] = 0;
            }
        }
    }
    for i in 1..rows {
        for j in 1..cols {
            if m[i][0] == 0 || m[0][j] == 0 {
                m[i][j] = 0;
            }
        }
    }
    if first_row_zero {
        m[0].iter_mut().for_each(|x| *x = 0);
    }
    if first_col_zero {
        m.iter_mut().for_each(|r| r[0] = 0);
    }
}
fn rand_matrix(rng: &mut Rng, rows: usize, cols: usize, zero_chance: usize) -> Vec<Vec<i32>> {
    (0..rows)
        .map(|_| {
            (0..cols)
                .map(|_| {
                    if rng.below(zero_chance) == 0 {
                        0
                    } else {
                        1 + rng.below(9) as i32
                    }
                })
                .collect()
        })
        .collect()
}
fn q22() {
    let mut rng = Rng::new(22);
    for _ in 0..CASES {
        let (r, c) = (1 + rng.below(5), 1 + rng.below(5));
        let m = rand_matrix(&mut rng, r, c, 6);
        assert_eq!(spiral_order(&m), spiral_oracle(&m), "{m:?}");
        let mut z = m.clone();
        set_zeroes(&mut z);
        let want: Vec<Vec<i32>> = (0..r)
            .map(|i| {
                (0..c)
                    .map(|j| {
                        if (0..c).any(|jj| m[i][jj] == 0) || (0..r).any(|ii| m[ii][j] == 0) {
                            0
                        } else {
                            m[i][j]
                        }
                    })
                    .collect()
            })
            .collect();
        assert_eq!(z, want, "{m:?}");
        let n = 1 + rng.below(5);
        let sq = rand_matrix(&mut rng, n, n, 100);
        let mut rot = sq.clone();
        rotate_image(&mut rot);
        for i in 0..n {
            for j in 0..n {
                assert_eq!(rot[j][n - 1 - i], sq[i][j]);
            }
        }
    }
    let m = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
    assert_eq!(spiral_order(&m), vec![1, 2, 3, 6, 9, 8, 7, 4, 5]);
    let mut r = m.clone();
    rotate_image(&mut r);
    assert_eq!(r, vec![vec![7, 4, 1], vec![8, 5, 2], vec![9, 6, 3]]);
}

// ================================================================================================
/// Q23. SEARCH IN ROTATED SORTED ARRAY and FIND MINIMUM IN ROTATED SORTED ARRAY.
///   [4,5,6,7,0,1,2], target 0 -> index 4;  min -> 0.  Follow-up: duplicates allowed (min only).
///
/// APPROACH. Linear scan O(n). Binary search O(log n): at every step at least one half is sorted. Search: decide
/// which half is sorted (`nums[lo] <= nums[mid]` means the left is), test whether the target lies inside the sorted
/// half's range, and go there, else go to the other half. Min: compare `nums[mid]` with `nums[hi]`: if mid is
/// greater, the minimum is right of mid; if smaller, mid could be the minimum (keep it). With duplicates and
/// `nums[mid] == nums[hi]` we can only discard `hi` (worst case O(n)).
/// RUST. Use half-open `[lo, hi)` and `mid = lo + (hi - lo) / 2` (never `(lo + hi) / 2`, which can overflow for
/// big indices). Return `Option<usize>`.
/// INTERVIEWER IS TESTING: the strict-vs-non-strict comparisons at the boundaries and the duplicates discussion.
fn search_rotated(nums: &[i64], target: i64) -> Option<usize> {
    let (mut lo, mut hi) = (0, nums.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if nums[mid] == target {
            return Some(mid);
        }
        if nums[lo] <= nums[mid] {
            if nums[lo] <= target && target < nums[mid] {
                hi = mid
            } else {
                lo = mid + 1
            }
        } else if nums[mid] < target && target <= nums[hi - 1] {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    None
}
fn find_min_rotated(nums: &[i64]) -> i64 {
    let (mut lo, mut hi) = (0, nums.len() - 1);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if nums[mid] > nums[hi] {
            lo = mid + 1;
        } else if nums[mid] < nums[hi] {
            hi = mid;
        } else {
            hi -= 1; // duplicates: cannot tell which side; safe to drop hi
        }
    }
    nums[lo]
}
fn q23() {
    let mut rng = Rng::new(23);
    for _ in 0..CASES {
        let n = 1 + rng.below(10);
        let mut v = Vec::with_capacity(n);
        let mut x = rng.int(-5, 5);
        for _ in 0..n {
            v.push(x);
            x += rng.int(1, 3);
        }
        v.rotate_left(rng.below(n));
        let t = rng.int(-6, 35);
        assert_eq!(
            search_rotated(&v, t),
            v.iter().position(|&y| y == t),
            "{v:?} {t}"
        );
        assert_eq!(find_min_rotated(&v), *v.iter().min().unwrap());
        // duplicates (non-decreasing then rotated) for the minimum
        let mut d: Vec<i64> = (0..n).map(|_| rng.int(0, 3)).collect();
        d.sort_unstable();
        d.rotate_left(rng.below(n));
        assert_eq!(find_min_rotated(&d), *d.iter().min().unwrap(), "{d:?}");
    }
    assert_eq!(search_rotated(&[4, 5, 6, 7, 0, 1, 2], 0), Some(4));
    assert_eq!(search_rotated(&[1], 0), None);
    assert_eq!(find_min_rotated(&[2, 2, 2, 0, 1, 2]), 0);
}

// ================================================================================================
/// Q24. FIRST MISSING POSITIVE (O(n) time, O(1) space); MAJORITY ELEMENT (> n/2, Boyer-Moore); SINGLE NUMBER I
/// (everything twice but one) and II (everything three times but one).
///   [3,4,-1,1] -> 2;  [2,2,1,1,1,2,2] -> 2;  [4,1,2,1,2] -> 4;  [2,2,3,2] -> 3
///
/// APPROACH. First missing positive: the answer is in 1..=n+1, so use the array itself as a hash set: CYCLIC
/// SORT, swapping each value v in 1..=n to index v-1 until the slot holds its value or the value is out of range;
/// then the first index with `nums[i] != i + 1` gives the answer. O(n) because every swap places one value for
/// good. Majority: sort-and-middle O(n log n), hash counts O(n) space, or BOYER-MOORE VOTING: keep a candidate and a
/// counter, O(n) time O(1) space (valid because a majority survives pairing-off). Single I: XOR everything
/// (x ^ x = 0). Single II: count each bit position modulo 3.
/// RUST. `nums.swap(i, j)` sidesteps the double-borrow; compute the target index in a local before swapping.
/// Cast `i32 -> usize` only AFTER checking `> 0`. For single II build the answer in `u32` then cast back to `i32`.
/// INTERVIEWER IS TESTING: duplicates in cyclic sort (the `nums[target] != nums[i]` guard prevents an infinite loop).
fn first_missing_positive(nums: &mut [i32]) -> i32 {
    let n = nums.len();
    for i in 0..n {
        while nums[i] > 0 && (nums[i] as usize) <= n && nums[nums[i] as usize - 1] != nums[i] {
            let j = nums[i] as usize - 1;
            nums.swap(i, j);
        }
    }
    (0..n)
        .find(|&i| nums[i] != i as i32 + 1)
        .map_or(n as i32 + 1, |i| i as i32 + 1)
}
fn majority_element(nums: &[i64]) -> i64 {
    let (mut cand, mut count) = (nums[0], 0);
    for &x in nums {
        if count == 0 {
            cand = x;
        }
        count += if x == cand { 1 } else { -1 };
    }
    cand
}
fn single_number(nums: &[i64]) -> i64 {
    nums.iter().fold(0, |a, &x| a ^ x)
}
fn single_number_ii(nums: &[i32]) -> i32 {
    let mut res = 0u32;
    for bit in 0..32 {
        let ones = nums.iter().filter(|&&x| (x >> bit) & 1 == 1).count();
        if ones % 3 != 0 {
            res |= 1 << bit;
        }
    }
    res as i32
}
fn q24() {
    let mut rng = Rng::new(24);
    for _ in 0..CASES {
        let n = rng.below(10);
        let v: Vec<i32> = (0..n).map(|_| rng.int(-2, 11) as i32).collect();
        let set: HashSet<i32> = v.iter().copied().collect();
        let want = (1..).find(|x| !set.contains(x)).unwrap();
        let mut w = v.clone();
        assert_eq!(first_missing_positive(&mut w), want, "{v:?}");
        // majority: construct an array in which one value fills more than half
        let m = 1 + rng.below(9);
        let maj = rng.int(-3, 3);
        let mut a: Vec<i64> = vec![maj; m / 2 + 1];
        a.extend((0..m - m / 2 - 1).map(|_| rng.int(-3, 3)));
        rng.shuffle(&mut a);
        assert_eq!(majority_element(&a), maj, "{a:?}");
        // single number I and II
        let (s, k) = (rng.int(-20, 20), 1 + rng.below(4));
        let mut pairs: Vec<i64> = vec![s];
        let mut triples: Vec<i32> = vec![s as i32];
        for i in 0..k {
            let x = 100 + i as i64 * 7 + rng.int(0, 5);
            pairs.extend([x, x]);
            triples.extend([x as i32; 3]);
        }
        rng.shuffle(&mut pairs);
        rng.shuffle(&mut triples);
        assert_eq!(single_number(&pairs), s);
        assert_eq!(single_number_ii(&triples), s as i32);
    }
    assert_eq!(first_missing_positive(&mut [3, 4, -1, 1]), 2);
    assert_eq!(first_missing_positive(&mut [1, 1]), 2); // duplicate guard avoids an infinite swap loop
    assert_eq!(first_missing_positive(&mut [7, 8, 9]), 1);
    assert_eq!(single_number_ii(&[-5, 2, 2, 2]), -5);
}

// ================================================================================================
/// Q25. BIT QUESTIONS: COUNT BITS (popcount of 0..=n), REVERSE BITS (u32), POWER OF TWO, SUM OF TWO INTEGERS
/// WITHOUT `+`/`-`.
///   count_bits(5) -> [0,1,1,2,1,2];  reverse_bits(0b1011) -> 0xD000_0000;  is_pow2(16) -> true;  add(1,2) -> 3
///
/// APPROACH. Count bits: DP `bits[i] = bits[i >> 1] + (i & 1)` gives all answers in O(n) (instead of O(n log n)
/// with a per-number loop; Kernighan's `x &= x - 1` loop costs one step per set bit). Reverse: shift the result
/// left and pull bits off the input's low end, 32 iterations. Power of two: a positive number with exactly one set
/// bit, `n > 0 && n & (n - 1) == 0`. Add without `+`: `a ^ b` is the sum ignoring carries, `(a & b) << 1` the
/// carries; repeat until no carry. Terminates for two's-complement i32 because the carry shifts out of the top.
/// RUST. `n - 1` on `i32::MIN` would panic in debug builds: the `n > 0` test short-circuits first. Use
/// `wrapping_add` as the oracle, and note `u32::count_ones`, `reverse_bits`, `is_power_of_two` exist in std.
/// INTERVIEWER IS TESTING: operator precedence in Rust (`&` binds tighter than `==` here, unlike C) and signed overflow.
fn count_bits(n: usize) -> Vec<u32> {
    let mut bits = vec![0u32; n + 1];
    for i in 1..=n {
        bits[i] = bits[i >> 1] + (i as u32 & 1);
    }
    bits
}
fn hamming_weight(mut x: u32) -> u32 {
    let mut c = 0;
    while x != 0 {
        x &= x - 1; // clear lowest set bit
        c += 1;
    }
    c
}
fn reverse_bits(mut x: u32) -> u32 {
    let mut r = 0u32;
    for _ in 0..32 {
        r = (r << 1) | (x & 1);
        x >>= 1;
    }
    r
}
fn is_power_of_two(n: i32) -> bool {
    n > 0 && n & (n - 1) == 0
}
fn add_no_plus(mut a: i32, mut b: i32) -> i32 {
    while b != 0 {
        let carry = (a & b) << 1;
        a ^= b;
        b = carry;
    }
    a
}
fn q25() {
    let bits = count_bits(1000);
    for i in 0..=1000usize {
        assert_eq!(bits[i], (i as u32).count_ones());
    }
    let mut rng = Rng::new(25);
    for _ in 0..CASES {
        let x = rng.next() as u32;
        assert_eq!(reverse_bits(x), x.reverse_bits());
        assert_eq!(hamming_weight(x), x.count_ones());
        let n = rng.next() as i32 >> rng.below(32); // spread of magnitudes, both signs
        assert_eq!(
            is_power_of_two(n),
            n > 0 && (0..31).any(|p| 1i32 << p == n),
            "{n}"
        );
        let (a, b) = (
            rng.next() as i32 >> rng.below(31),
            rng.next() as i32 >> rng.below(31),
        );
        assert_eq!(add_no_plus(a, b), a.wrapping_add(b), "{a} {b}");
    }
    for p in 0..31 {
        assert!(is_power_of_two(1 << p));
    }
    for n in [0, -1, i32::MIN, i32::MAX, 3, 6] {
        assert!(!is_power_of_two(n));
    }
    assert_eq!(add_no_plus(-1, 1), 0);
    assert_eq!(add_no_plus(i32::MAX, 1), i32::MIN);
    assert_eq!(reverse_bits(0b1011), 0xD000_0000);
}

// ================================================================================================
/// Q26. 3SUM CLOSEST and NEXT PERMUTATION.
///   [-1,2,1,-4], target 1 -> 2 (-1 + 2 + 1);   [1,2,3] -> [1,3,2];  [3,2,1] -> [1,2,3];  [1,1,5] -> [1,5,1]
///
/// APPROACH. 3Sum closest: sort, fix i, two pointers on the rest, tracking the sum with the smallest
/// `|sum - target|`; O(n^2), O(1). Return early on an exact hit. Next permutation (lexicographic): (1) find the
/// rightmost `i` with `v[i] < v[i+1]` (the PIVOT); if none, the array is the last permutation, reverse it;
/// (2) find the rightmost `j` with `v[j] > v[pivot]`; (3) swap them; (4) reverse the suffix after the pivot.
/// O(n) time, O(1) space.
/// RUST. Use `abs_diff` for distances (no overflow, no sign mistakes). In next_permutation a descending suffix is
/// found by walking an index down with a `while i > 0` guard; `slice::reverse` and `swap` do the rest. `std`
/// has no `next_permutation`; the itertools crate does, but you must write it by hand in an interview.
/// INTERVIEWER IS TESTING: duplicates in next permutation (use `>=` when walking back), and the suffix-reverse step.
fn three_sum_closest(nums: &[i64], target: i64) -> i64 {
    let mut v = nums.to_vec();
    v.sort_unstable();
    let mut best = v[0] + v[1] + v[2];
    for i in 0..v.len() - 2 {
        let (mut l, mut r) = (i + 1, v.len() - 1);
        while l < r {
            let s = v[i] + v[l] + v[r];
            if s.abs_diff(target) < best.abs_diff(target) {
                best = s;
            }
            match s.cmp(&target) {
                std::cmp::Ordering::Less => l += 1,
                std::cmp::Ordering::Greater => r -= 1,
                std::cmp::Ordering::Equal => return s,
            }
        }
    }
    best
}
fn next_permutation(v: &mut [i32]) -> bool {
    let n = v.len();
    if n < 2 {
        return false;
    }
    let mut i = n - 1;
    while i > 0 && v[i - 1] >= v[i] {
        i -= 1;
    }
    if i == 0 {
        v.reverse();
        return false;
    }
    let pivot = i - 1;
    let mut j = n - 1;
    while v[j] <= v[pivot] {
        j -= 1;
    }
    v.swap(pivot, j);
    v[i..].reverse();
    true
}
fn all_perms(v: &mut Vec<i32>, k: usize, out: &mut BTreeSet<Vec<i32>>) {
    if k == v.len() {
        out.insert(v.clone());
        return;
    }
    for i in k..v.len() {
        v.swap(k, i);
        all_perms(v, k + 1, out);
        v.swap(k, i);
    }
}
fn q26() {
    let mut rng = Rng::new(26);
    for _ in 0..CASES {
        let n = 3 + rng.below(7);
        let v = rng.vec(n, -8, 8);
        let t = rng.int(-20, 20);
        let mut want = i64::MAX;
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    want = want.min((v[i] + v[j] + v[k]).abs_diff(t) as i64);
                }
            }
        }
        assert_eq!(
            three_sum_closest(&v, t).abs_diff(t) as i64,
            want,
            "{v:?} {t}"
        );
        let m = 1 + rng.below(6);
        let p: Vec<i32> = (0..m).map(|_| rng.below(4) as i32).collect();
        let mut perms = BTreeSet::new();
        all_perms(&mut p.clone(), 0, &mut perms);
        let successor = perms
            .range::<Vec<i32>, _>((std::ops::Bound::Excluded(&p), std::ops::Bound::Unbounded))
            .next();
        let mut q = p.clone();
        let had_next = next_permutation(&mut q);
        match successor {
            Some(s) => {
                assert!(had_next);
                assert_eq!(&q, s, "{p:?}");
            }
            None => {
                assert!(!had_next);
                assert_eq!(&q, perms.iter().next().unwrap());
            }
        }
    }
    assert_eq!(three_sum_closest(&[-1, 2, 1, -4], 1), 2);
    let mut a = [1, 1, 5];
    next_permutation(&mut a);
    assert_eq!(a, [1, 5, 1]);
}

// ================================================================================================
/// Q27. BINARY SEARCH VARIANTS: LOWER BOUND / UPPER BOUND (first index >= x / > x), SEARCH INSERT POSITION,
/// SQRT(x) (floor, no `sqrt`), KOKO EATING BANANAS (min speed k so all piles are eaten in h hours: SEARCH ON THE ANSWER).
///   lower_bound([1,2,2,2,3], 2) = 1, upper_bound = 4;  sqrt(8) = 2;  piles [3,6,7,11], h = 8 -> 4
///
/// APPROACH. Brute force is a linear scan. All of these are the SAME template: given a monotone predicate
/// (false...false true...true) over a range, find the FIRST true with `lo < hi` half-open binary search:
/// `mid = lo + (hi - lo) / 2; if pred(mid) { hi = mid } else { lo = mid + 1 }`. lower bound: `a[i] >= x`;
/// upper bound: `a[i] > x`; insert position = lower bound; count of x = upper - lower; sqrt: first m with
/// `m*m > x`, minus one; Koko: first k with `sum(ceil(p / k)) <= h`. O(log range) predicate calls.
/// RUST. `mid*mid` overflows: widen to `u128` (or use `checked_mul`). `u64::div_ceil` avoids the `(p + k - 1) / k`
/// overflow idiom. `slice::partition_point` is the std version of the template; know it. The answer range for sqrt is
/// `[0, min(x, 2^32)]`; for Koko `[1, max pile]`.
/// INTERVIEWER IS TESTING: loop invariant (`lo < hi`, `hi = mid`) vs the `lo <= hi` variant, off-by-one, overflow.
fn first_true(mut lo: u64, mut hi: u64, pred: impl Fn(u64) -> bool) -> u64 {
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid) { hi = mid } else { lo = mid + 1 }
    }
    lo
}
fn lower_bound(a: &[i64], x: i64) -> usize {
    first_true(0, a.len() as u64, |i| a[i as usize] >= x) as usize
}
fn upper_bound(a: &[i64], x: i64) -> usize {
    first_true(0, a.len() as u64, |i| a[i as usize] > x) as usize
}
fn my_sqrt(x: u64) -> u64 {
    first_true(0, x.min(1 << 32) + 1, |m| {
        (m as u128) * (m as u128) > x as u128
    }) - 1
}
fn min_eating_speed(piles: &[u64], h: u64) -> u64 {
    let max = *piles.iter().max().unwrap();
    first_true(1, max + 1, |k| {
        piles.iter().map(|&p| p.div_ceil(k)).sum::<u64>() <= h
    })
}
fn q27() {
    let mut rng = Rng::new(27);
    for _ in 0..CASES {
        let n = rng.below(12);
        let mut a = rng.vec(n, 0, 6);
        a.sort_unstable();
        let x = rng.int(-1, 7);
        assert_eq!(lower_bound(&a, x), a.partition_point(|&y| y < x));
        assert_eq!(upper_bound(&a, x), a.partition_point(|&y| y <= x));
        assert_eq!(lower_bound(&a, x), a.iter().take_while(|&&y| y < x).count()); // search insert position
        let v = rng.next() >> rng.below(64);
        let r = my_sqrt(v) as u128;
        assert!(
            r * r <= v as u128 && (r + 1) * (r + 1) > v as u128,
            "sqrt({v}) = {r}"
        );
        let piles: Vec<u64> = (0..1 + rng.below(6))
            .map(|_| 1 + rng.below(30) as u64)
            .collect();
        let h = piles.len() as u64 + rng.below(40) as u64;
        let want = (1..)
            .find(|&k| piles.iter().map(|&p| p.div_ceil(k)).sum::<u64>() <= h)
            .unwrap();
        assert_eq!(min_eating_speed(&piles, h), want, "{piles:?} h={h}");
    }
    assert_eq!(my_sqrt(8), 2);
    assert_eq!(my_sqrt(0), 0);
    assert_eq!(my_sqrt(u64::MAX), 4_294_967_295);
    assert_eq!(min_eating_speed(&[3, 6, 7, 11], 8), 4);
    assert_eq!(min_eating_speed(&[30, 11, 23, 4, 20], 5), 30);
    assert_eq!(lower_bound(&[1, 2, 2, 2, 3], 2), 1);
    assert_eq!(upper_bound(&[1, 2, 2, 2, 3], 2), 4);
}

fn main() {
    let qs: [(&str, fn()); 27] = [
        ("Q1", q1),
        ("Q2", q2),
        ("Q3", q3),
        ("Q4", q4),
        ("Q5", q5),
        ("Q6", q6),
        ("Q7", q7),
        ("Q8", q8),
        ("Q9", q9),
        ("Q10", q10),
        ("Q11", q11),
        ("Q12", q12),
        ("Q13", q13),
        ("Q14", q14),
        ("Q15", q15),
        ("Q16", q16),
        ("Q17", q17),
        ("Q18", q18),
        ("Q19", q19),
        ("Q20", q20),
        ("Q21", q21),
        ("Q22", q22),
        ("Q23", q23),
        ("Q24", q24),
        ("Q25", q25),
        ("Q26", q26),
        ("Q27", q27),
    ];
    for (name, q) in qs {
        q();
        println!("{name}: ok");
    }
}
