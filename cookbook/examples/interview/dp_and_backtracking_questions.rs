//! RECIPE 621 (interview): dynamic programming and backtracking: the most-asked coding questions, answered in Rust.
//! LEVEL: advanced
//! Use in: preparing for coding interviews; recognising the DP state or the choose/explore/unchoose shape behind a question.   Run: cargo run --release --example dp_and_backtracking_questions
//! COMMON USES: phone screens and on-sites (NeetCode / Blind 75 / Grind 75 style), scheduling and packing, text diffing, constraint solving.
#![allow(dead_code)]
//! Read: https://www.techinterviewhandbook.org/grind75 , https://neetcode.io/practice , https://interviewing.io/rust-interview-questions , https://cp-algorithms.com/
//!
//! QUESTION BANK (numbering restarts in each interview file; a generator reads these lines)
//! Q1. Climbing Stairs and Min Cost Climbing Stairs (tabulation vs memoisation vs rolling variables). [Easy] [asked: very common]
//! Q2. House Robber I and II (linear and circular). [Medium] [asked: very common]
//! Q3. Coin Change (fewest coins) and Coin Change II (number of ways: combinations vs permutations). [Medium] [asked: very common]
//! Q4. Longest Increasing Subsequence in O(n^2) and O(n log n). [Medium] [asked: very common]
//! Q5. Longest Common Subsequence (2-D table, rolling rows, memoisation, reconstruction). [Medium] [asked: very common]
//! Q6. Edit Distance (Levenshtein). [Medium] [asked: very common]
//! Q7. Word Break. [Medium] [asked: very common]
//! Q8. Decode Ways. [Medium] [asked: very common]
//! Q9. Unique Paths I and II and Minimum Path Sum (grid DP). [Medium] [asked: very common]
//! Q10. Maximal Square. [Medium] [asked: common]
//! Q11. 0/1 Knapsack, Partition Equal Subset Sum and Target Sum. [Medium] [asked: very common]
//! Q12. Longest Palindromic Subsequence. [Medium] [asked: common]
//! Q13. Best Time to Buy and Sell Stock with Cooldown (state machine). [Medium] [asked: common]
//! Q14. Jump Game I and II. [Medium] [asked: very common]
//! Q15. Maximum Product Subarray as a DP table. [Medium] [asked: common]
//! Q16. Regular Expression Matching and Wildcard Matching (2-D DP). [Hard] [asked: common]
//! Q17. Interleaving String and Distinct Subsequences. [Hard] [asked: occasional]
//! Q18. Burst Balloons (interval DP). [Hard] [asked: common]
//! Q19. Egg Dropping (O(k n^2) table and the O(k log n) idea). [Hard] [asked: common]
//! Q20. Triangle and Perfect Squares. [Medium] [asked: common]
//! Q21. Subsets I and II (with duplicates). [Medium] [asked: very common]
//! Q22. Permutations I and II (with duplicates). [Medium] [asked: very common]
//! Q23. Combination Sum I and II. [Medium] [asked: very common]
//! Q24. Generate Parentheses and Letter Combinations of a Phone Number. [Medium] [asked: very common]
//! Q25. Palindrome Partitioning and Restore IP Addresses. [Medium] [asked: common]
//! Q26. N-Queens (count with bitmasks, and boards). [Hard] [asked: common]
//! Q27. Word Search (grid backtracking with in-place marking). [Medium] [asked: very common]
//! Q28. Sudoku Solver (bitmasks + minimum remaining values). [Hard] [asked: common]
//! Q29. Partition to K Equal Sum Subsets. [Medium] [asked: common]
//!
//! Every question: statement, approach (brute force then optimal, complexity, pattern), Rust wrinkle, the solution(s),
//! a brute-force oracle with 2000 seeded xorshift cases plus edge cases (fewer for the heavyweight sudoku), and what
//! the interviewer is really testing.  Questions 1, 3, 5, 6, 9 and 11 each implement tabulation, memoisation with a
//! `HashMap`, memoisation with a `Vec<Option<_>>` and a space-optimised rolling array, and assert they all agree.
//!
//! DP RECIPE (say it out loud in the interview): (1) define the STATE in words ("best[i] = ... using the first i items"),
//! (2) write the TRANSITION, (3) base cases, (4) iteration ORDER (which neighbours must be ready), (5) the answer cell,
//! (6) optimise space only after it is correct (a row depends only on the previous row -> keep one or two rows).
//! BACKTRACKING RECIPE: CHOOSE (push), EXPLORE (recurse), UNCHOOSE (pop); prune early; dedupe duplicates by sorting
//! and skipping equal siblings (`j > start && v[j] == v[j - 1]`), never by hashing results.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

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
/// Q1. CLIMBING STAIRS (n steps, climb 1 or 2 at a time: how many distinct ways?) and MIN COST CLIMBING STAIRS
/// (`cost[i]` is paid when you leave step i; start on step 0 or 1; reach the top just past the last step).
///   n = 5 -> 8;   cost [10,15,20] -> 15;   cost [1,100,1,1,1,100,1,1,100,1] -> 6
///
/// APPROACH. Brute force: recursion `f(n) = f(n-1) + f(n-2)`, O(2^n). DP state: `ways[i]` = ways to reach step i.
/// The same recurrence, four ways: (1) TABULATION fill `dp[0..=n]` bottom-up, O(n)/O(n); (2) MEMOISATION with a
/// `HashMap<usize,u64>`; (3) MEMOISATION with `Vec<Option<u64>>` (faster, indices are dense); (4) ROLLING two
/// variables, O(n) time O(1) space. Min cost: `best[i] = min(best[i-1] + cost[i-1], best[i-2] + cost[i-2])`.
/// RUST. Memo functions take `&mut` caches: recursion + `&mut HashMap` has no borrow trouble because the cache is
/// only touched between recursive calls (look up, release, recurse, insert). Deep recursion can overflow the stack
/// for n ~ 100k: prefer tabulation then. The answer grows like Fibonacci: `u64` overflows past n = 92.
/// INTERVIEWER IS TESTING: do you spot overlapping subproblems and move recursion -> memo -> table -> O(1) space.
fn stairs_brute(n: usize) -> u64 {
    if n <= 1 {
        1
    } else {
        stairs_brute(n - 1) + stairs_brute(n - 2)
    }
}
fn stairs_tab(n: usize) -> u64 {
    let mut dp = vec![1u64; n + 2];
    for i in 2..=n {
        dp[i] = dp[i - 1] + dp[i - 2];
    }
    dp[n]
}
fn stairs_memo_hash(n: usize, memo: &mut HashMap<usize, u64>) -> u64 {
    if n <= 1 {
        return 1;
    }
    if let Some(&v) = memo.get(&n) {
        return v;
    }
    let v = stairs_memo_hash(n - 1, memo) + stairs_memo_hash(n - 2, memo);
    memo.insert(n, v);
    v
}
fn stairs_memo_vec(n: usize, memo: &mut Vec<Option<u64>>) -> u64 {
    if n <= 1 {
        return 1;
    }
    if let Some(v) = memo[n] {
        return v;
    }
    let v = stairs_memo_vec(n - 1, memo) + stairs_memo_vec(n - 2, memo);
    memo[n] = Some(v);
    v
}
fn stairs_rolling(n: usize) -> u64 {
    let (mut a, mut b) = (1u64, 1u64);
    for _ in 1..n {
        (a, b) = (b, a + b);
    }
    b
}
fn min_cost_climbing(cost: &[u64]) -> u64 {
    let (mut a, mut b) = (0u64, 0u64); // best cost to stand on step i-2 and i-1
    for i in 2..=cost.len() {
        (a, b) = (b, (b + cost[i - 1]).min(a + cost[i - 2]));
    }
    b
}
fn min_cost_brute(cost: &[u64], pos: usize) -> u64 {
    if pos >= cost.len() {
        0
    } else {
        cost[pos] + min_cost_brute(cost, pos + 1).min(min_cost_brute(cost, pos + 2))
    }
}
fn q1() {
    for n in 0..=20 {
        let want = stairs_brute(n);
        assert_eq!(stairs_tab(n), want);
        assert_eq!(stairs_memo_hash(n, &mut HashMap::new()), want);
        assert_eq!(stairs_memo_vec(n, &mut vec![None; n + 1]), want);
        assert_eq!(stairs_rolling(n), want);
    }
    assert_eq!(stairs_rolling(90), stairs_tab(90));
    let mut rng = Rng::new(1);
    for _ in 0..CASES {
        let n = 2 + rng.below(12);
        let cost: Vec<u64> = (0..n).map(|_| rng.below(20) as u64).collect();
        assert_eq!(
            min_cost_climbing(&cost),
            min_cost_brute(&cost, 0).min(min_cost_brute(&cost, 1)),
            "{cost:?}"
        );
    }
    assert_eq!(min_cost_climbing(&[10, 15, 20]), 15);
    assert_eq!(min_cost_climbing(&[1, 100, 1, 1, 1, 100, 1, 1, 100, 1]), 6);
}

// ================================================================================================
/// Q2. HOUSE ROBBER I (max sum of non-adjacent elements) and II (the houses are in a CIRCLE: first and last are adjacent).
///   [2,7,9,3,1] -> 12;   circular [2,3,2] -> 3;   circular [1,2,3,1] -> 4
///
/// APPROACH. Brute force: all subsets with no two adjacent, O(2^n). DP state: `best[i]` = best using the first i houses:
/// `best[i] = max(best[i-1], best[i-2] + nums[i])` ("skip or take"), O(n), O(1) with two rolling variables.
/// Circular: the first and last cannot both be taken, so the answer is `max(rob(nums[1..]), rob(nums[..n-1]))`;
/// the n == 1 case is special (a single house is both first and last). Pattern: 1-D DP with a take/skip choice.
/// RUST. Slicing `&nums[1..]` and `&nums[..n - 1]` reuses the linear solver at zero cost; guard `n == 0` and `n == 1`
/// before computing `n - 1`.
/// INTERVIEWER IS TESTING: the circular reduction to two linear problems, and n = 1.
fn rob(nums: &[i64]) -> i64 {
    let (mut prev2, mut prev1) = (0, 0);
    for &x in nums {
        (prev2, prev1) = (prev1, prev1.max(prev2 + x));
    }
    prev1
}
fn rob_circular(nums: &[i64]) -> i64 {
    match nums.len() {
        0 => 0,
        1 => nums[0],
        n => rob(&nums[1..]).max(rob(&nums[..n - 1])),
    }
}
fn q2() {
    let mut rng = Rng::new(2);
    for _ in 0..CASES {
        let n = rng.below(11);
        let v = rng.vec(n, 0, 9);
        let (mut lin, mut circ) = (0, 0);
        for mask in 0u32..1 << n {
            let take = |i: usize| mask >> i & 1 == 1;
            if (1..n).any(|i| take(i) && take(i - 1)) {
                continue;
            }
            let sum: i64 = (0..n).filter(|&i| take(i)).map(|i| v[i]).sum();
            lin = lin.max(sum);
            if !(n > 1 && take(0) && take(n - 1)) {
                circ = circ.max(sum);
            }
        }
        assert_eq!(rob(&v), lin, "{v:?}");
        assert_eq!(rob_circular(&v), circ, "{v:?}");
    }
    assert_eq!(rob(&[2, 7, 9, 3, 1]), 12);
    assert_eq!(rob_circular(&[2, 3, 2]), 3);
    assert_eq!(rob_circular(&[1, 2, 3, 1]), 4);
}

// ================================================================================================
/// Q3. COIN CHANGE (fewest coins to make `amount`, -1/None if impossible) and COIN CHANGE II (number of COMBINATIONS).
///   coins [1,2,5], amount 11 -> 3 coins;   coins [2], amount 3 -> impossible;   ways(5, [1,2,5]) -> 4
///
/// APPROACH. Brute force: try every coin at every step, exponential. DP state `dp[a]` = fewest coins for amount a:
/// `dp[a] = 1 + min(dp[a - c])` over coins c <= a; O(amount * coins) time, O(amount) space. (BFS over amounts is an
/// equivalent shortest-path view and is used as the oracle.) Ways: `dp[a] += dp[a - c]` with the COIN loop OUTSIDE and the amount loop inside, so each
/// multiset is counted once; swapping the loops counts ORDERED sequences (permutations), the classic bug.
/// Tabulation, `HashMap` memo, `Vec<Option<_>>` memo all given for the min version and asserted equal.
/// RUST. Use `usize::MAX` as "impossible" and test before adding 1 (no overflow); count ways in `u64`.
/// INTERVIEWER IS TESTING: the infinity sentinel, amount 0 -> 0, and combinations vs permutations.
const INF: usize = usize::MAX;
fn coin_min_tab(coins: &[usize], amount: usize) -> Option<usize> {
    let mut dp = vec![INF; amount + 1];
    dp[0] = 0;
    for a in 1..=amount {
        for &c in coins {
            if c <= a && dp[a - c] != INF {
                dp[a] = dp[a].min(dp[a - c] + 1);
            }
        }
    }
    (dp[amount] != INF).then_some(dp[amount])
}
fn coin_min_memo_hash(coins: &[usize], a: usize, memo: &mut HashMap<usize, usize>) -> usize {
    if a == 0 {
        return 0;
    }
    if let Some(&v) = memo.get(&a) {
        return v;
    }
    let mut best = INF;
    for &c in coins {
        if c <= a {
            let sub = coin_min_memo_hash(coins, a - c, memo);
            if sub != INF {
                best = best.min(sub + 1);
            }
        }
    }
    memo.insert(a, best);
    best
}
fn coin_min_memo_vec(coins: &[usize], a: usize, memo: &mut Vec<Option<usize>>) -> usize {
    if a == 0 {
        return 0;
    }
    if let Some(v) = memo[a] {
        return v;
    }
    let mut best = INF;
    for &c in coins {
        if c <= a {
            let sub = coin_min_memo_vec(coins, a - c, memo);
            if sub != INF {
                best = best.min(sub + 1);
            }
        }
    }
    memo[a] = Some(best);
    best
}
fn coin_min_bfs(coins: &[usize], amount: usize) -> Option<usize> {
    let mut dist = vec![INF; amount + 1];
    dist[0] = 0;
    let mut q = VecDeque::from([0usize]);
    while let Some(a) = q.pop_front() {
        for &c in coins {
            if a + c <= amount && dist[a + c] == INF {
                dist[a + c] = dist[a] + 1;
                q.push_back(a + c);
            }
        }
    }
    (dist[amount] != INF).then_some(dist[amount])
}
fn coin_ways(coins: &[usize], amount: usize) -> u64 {
    let mut dp = vec![0u64; amount + 1];
    dp[0] = 1;
    for &c in coins {
        for a in c..=amount {
            dp[a] += dp[a - c];
        }
    }
    dp[amount]
}
fn coin_ordered_ways(coins: &[usize], amount: usize) -> u64 {
    let mut dp = vec![0u64; amount + 1];
    dp[0] = 1;
    for a in 1..=amount {
        for &c in coins {
            if c <= a {
                dp[a] += dp[a - c];
            }
        }
    }
    dp[amount]
}
fn coin_ways_brute(coins: &[usize], i: usize, rem: usize) -> u64 {
    if rem == 0 {
        return 1;
    }
    if i == coins.len() {
        return 0;
    }
    coin_ways_brute(coins, i + 1, rem)
        + if coins[i] <= rem {
            coin_ways_brute(coins, i, rem - coins[i])
        } else {
            0
        }
}
fn q3() {
    let mut rng = Rng::new(3);
    for _ in 0..CASES {
        let mut coins: Vec<usize> = (0..1 + rng.below(3)).map(|_| 1 + rng.below(6)).collect();
        coins.sort_unstable();
        coins.dedup();
        let amount = rng.below(25);
        let want = coin_min_bfs(&coins, amount);
        assert_eq!(coin_min_tab(&coins, amount), want, "{coins:?} {amount}");
        let h = coin_min_memo_hash(&coins, amount, &mut HashMap::new());
        let v = coin_min_memo_vec(&coins, amount, &mut vec![None; amount + 1]);
        assert_eq!((h != INF).then_some(h), want);
        assert_eq!(h, v);
        assert_eq!(
            coin_ways(&coins, amount),
            coin_ways_brute(&coins, 0, amount),
            "{coins:?} {amount}"
        );
    }
    assert_eq!(coin_min_tab(&[1, 2, 5], 11), Some(3));
    assert_eq!(coin_min_tab(&[2], 3), None);
    assert_eq!(coin_min_tab(&[1], 0), Some(0));
    assert_eq!(coin_ways(&[1, 2, 5], 5), 4);
    assert_eq!(coin_ways(&[1, 2], 3), 2);
    assert_eq!(coin_ordered_ways(&[1, 2], 3), 3); // 1+1+1, 1+2, 2+1: ordered sequences, a different question
}

// ================================================================================================
/// Q4. LONGEST INCREASING SUBSEQUENCE (strictly increasing, not contiguous).
///   [10,9,2,5,3,7,101,18] -> 4 ([2,3,7,101])
///
/// APPROACH. Brute force: all subsets, O(2^n). DP O(n^2): `dp[i]` = LIS ending at i = 1 + max(dp[j]) over j < i with
/// `nums[j] < nums[i]`. O(n log n) PATIENCE SORTING: keep `tails[k]` = smallest possible tail of an increasing subsequence of
/// length k+1; for each x binary-search the first tail >= x (`partition_point(|&t| t < x)`), replace it (or push). `tails`
/// is always sorted, and its length is the answer (it is NOT itself the subsequence). For non-decreasing use `<=`.
/// RUST. `Vec::partition_point` is the std binary search for "first index where the predicate turns false".
/// INTERVIEWER IS TESTING: whether you know the O(n log n) method, and can explain why `tails` is not an actual LIS.
fn lis_n2(nums: &[i64]) -> usize {
    let mut dp = vec![1usize; nums.len()];
    for i in 0..nums.len() {
        for j in 0..i {
            if nums[j] < nums[i] {
                dp[i] = dp[i].max(dp[j] + 1);
            }
        }
    }
    dp.into_iter().max().unwrap_or(0)
}
fn lis_nlogn(nums: &[i64]) -> usize {
    let mut tails: Vec<i64> = Vec::new();
    for &x in nums {
        let k = tails.partition_point(|&t| t < x);
        if k == tails.len() {
            tails.push(x)
        } else {
            tails[k] = x
        }
    }
    tails.len()
}
fn q4() {
    let mut rng = Rng::new(4);
    for _ in 0..CASES {
        let n = rng.below(12);
        let v = rng.vec(n, 0, 9);
        let mut want = 0;
        for mask in 0u32..1 << n {
            let sub: Vec<i64> = (0..n)
                .filter(|&i| mask >> i & 1 == 1)
                .map(|i| v[i])
                .collect();
            if sub.windows(2).all(|w| w[0] < w[1]) {
                want = want.max(sub.len());
            }
        }
        assert_eq!(lis_n2(&v), want, "{v:?}");
        assert_eq!(lis_nlogn(&v), want, "{v:?}");
    }
    assert_eq!(lis_nlogn(&[10, 9, 2, 5, 3, 7, 101, 18]), 4);
    assert_eq!(lis_nlogn(&[7, 7, 7]), 1);
}

// ================================================================================================
/// Q5. LONGEST COMMON SUBSEQUENCE of two strings (and reconstruct one).   "abcde","ace" -> 3 ("ace")
///
/// APPROACH. Brute force: every subsequence of `a` tested against `b`, O(2^n * m). DP state `dp[i][j]` = LCS of
/// `a[..i]` and `b[..j]`: if the last chars match `dp[i-1][j-1] + 1` else `max(dp[i-1][j], dp[i][j-1])`. O(nm) time.
/// Row i depends only on row i-1, so keep TWO rows (or one row plus a saved diagonal): O(min(n,m)) space. To recover the
/// string, keep the full table and walk back from `(n, m)`. Variants built from the same table: shortest common
/// supersequence, edit distance with only insert/delete, diff tools. Four implementations below must agree.
/// RUST. Work on `&[u8]` for ASCII; for Unicode collect `Vec<char>`. Index the table as `dp[i][j]` with a leading row/column
/// of zeros so no `i - 1` underflow happens at 0.
/// INTERVIEWER IS TESTING: base-case row/column, the walk-back, and the rolling-row follow-up.
fn lcs_tab(a: &[u8], b: &[u8]) -> Vec<Vec<usize>> {
    let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }
    dp
}
fn lcs_rolling(a: &[u8], b: &[u8]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        let mut cur = vec![0usize; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1] + 1
            } else {
                prev[j].max(cur[j - 1])
            };
        }
        prev = cur;
    }
    prev[b.len()]
}
fn lcs_memo_vec(
    a: &[u8],
    b: &[u8],
    i: usize,
    j: usize,
    memo: &mut Vec<Vec<Option<usize>>>,
) -> usize {
    if i == 0 || j == 0 {
        return 0;
    }
    if let Some(v) = memo[i][j] {
        return v;
    }
    let v = if a[i - 1] == b[j - 1] {
        lcs_memo_vec(a, b, i - 1, j - 1, memo) + 1
    } else {
        lcs_memo_vec(a, b, i - 1, j, memo).max(lcs_memo_vec(a, b, i, j - 1, memo))
    };
    memo[i][j] = Some(v);
    v
}
fn lcs_memo_hash(
    a: &[u8],
    b: &[u8],
    i: usize,
    j: usize,
    memo: &mut HashMap<(usize, usize), usize>,
) -> usize {
    if i == 0 || j == 0 {
        return 0;
    }
    if let Some(&v) = memo.get(&(i, j)) {
        return v;
    }
    let v = if a[i - 1] == b[j - 1] {
        lcs_memo_hash(a, b, i - 1, j - 1, memo) + 1
    } else {
        lcs_memo_hash(a, b, i - 1, j, memo).max(lcs_memo_hash(a, b, i, j - 1, memo))
    };
    memo.insert((i, j), v);
    v
}
fn lcs_string(a: &[u8], b: &[u8]) -> Vec<u8> {
    let dp = lcs_tab(a, b);
    let (mut i, mut j, mut out) = (a.len(), b.len(), vec![]);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            out.push(a[i - 1]);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out.reverse();
    out
}
fn is_subsequence(x: &[u8], y: &[u8]) -> bool {
    let mut it = y.iter();
    x.iter().all(|c| it.any(|d| d == c))
}
fn q5() {
    let mut rng = Rng::new(5);
    for _ in 0..CASES {
        let (a, b) = (rng.string("abc", 8), rng.string("abc", 8));
        let (a, b) = (a.as_bytes(), b.as_bytes());
        let mut want = 0;
        for mask in 0u32..1 << a.len() {
            let sub: Vec<u8> = (0..a.len())
                .filter(|&i| mask >> i & 1 == 1)
                .map(|i| a[i])
                .collect();
            if sub.len() > want && is_subsequence(&sub, b) {
                want = sub.len();
            }
        }
        let table = lcs_tab(a, b);
        assert_eq!(table[a.len()][b.len()], want);
        assert_eq!(lcs_rolling(a, b), want);
        assert_eq!(
            lcs_memo_vec(
                a,
                b,
                a.len(),
                b.len(),
                &mut vec![vec![None; b.len() + 1]; a.len() + 1]
            ),
            want
        );
        assert_eq!(
            lcs_memo_hash(a, b, a.len(), b.len(), &mut HashMap::new()),
            want
        );
        let s = lcs_string(a, b);
        assert!(s.len() == want && is_subsequence(&s, a) && is_subsequence(&s, b));
    }
    assert_eq!(lcs_rolling(b"abcde", b"ace"), 3);
    assert_eq!(lcs_string(b"abcde", b"ace"), b"ace");
}

// ================================================================================================
/// Q6. EDIT DISTANCE (Levenshtein): fewest insert / delete / replace operations to turn `word1` into `word2`.
///   "horse" -> "ros" = 3;   "intention" -> "execution" = 5
///
/// APPROACH. Brute force: try all three operations at the first mismatch, O(3^n). DP state `dp[i][j]` = distance between
/// the first i chars of a and the first j chars of b: equal chars -> `dp[i-1][j-1]`, else `1 + min(dp[i-1][j] (delete),
/// dp[i][j-1] (insert), dp[i-1][j-1] (replace))`. Base: `dp[i][0] = i`, `dp[0][j] = j`. O(nm) time; two rolling rows
/// (or one) give O(min(n,m)) space. Tabulation, rolling and `HashMap` memo asserted equal. Related: LCS (Q5),
/// one-edit-distance, Damerau (adds transposition), banded edit distance for near-equal strings in O(k n).
/// RUST. Collect `Vec<char>` first for Unicode; edit distance by bytes would count a multi-byte char as several edits.
/// INTERVIEWER IS TESTING: base rows, which of the three predecessors is which operation, space reduction.
fn edit_tab(a: &[char], b: &[char]) -> usize {
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1])
            };
        }
    }
    dp[a.len()][b.len()]
}
fn edit_rolling(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut diag = row[0]; // dp[i-1][j-1]
        row[0] = i;
        for j in 1..=b.len() {
            let up = row[j]; // dp[i-1][j]
            row[j] = if a[i - 1] == b[j - 1] {
                diag
            } else {
                1 + up.min(row[j - 1]).min(diag)
            };
            diag = up;
        }
    }
    row[b.len()]
}
fn edit_memo(
    a: &[char],
    b: &[char],
    i: usize,
    j: usize,
    memo: &mut HashMap<(usize, usize), usize>,
) -> usize {
    if i == 0 {
        return j;
    }
    if j == 0 {
        return i;
    }
    if let Some(&v) = memo.get(&(i, j)) {
        return v;
    }
    let v = if a[i - 1] == b[j - 1] {
        edit_memo(a, b, i - 1, j - 1, memo)
    } else {
        1 + edit_memo(a, b, i - 1, j, memo)
            .min(edit_memo(a, b, i, j - 1, memo))
            .min(edit_memo(a, b, i - 1, j - 1, memo))
    };
    memo.insert((i, j), v);
    v
}
fn edit_brute(a: &[char], b: &[char]) -> usize {
    if a.is_empty() || b.is_empty() {
        return a.len().max(b.len());
    }
    if a[0] == b[0] {
        return edit_brute(&a[1..], &b[1..]);
    }
    1 + edit_brute(&a[1..], b)
        .min(edit_brute(a, &b[1..]))
        .min(edit_brute(&a[1..], &b[1..]))
}
fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}
fn q6() {
    let mut rng = Rng::new(6);
    for _ in 0..CASES {
        let (a, b) = (chars(&rng.string("abc", 6)), chars(&rng.string("abc", 6)));
        let want = edit_brute(&a, &b);
        assert_eq!(edit_tab(&a, &b), want, "{a:?} {b:?}");
        assert_eq!(edit_rolling(&a, &b), want);
        assert_eq!(
            edit_memo(&a, &b, a.len(), b.len(), &mut HashMap::new()),
            want
        );
    }
    assert_eq!(edit_tab(&chars("horse"), &chars("ros")), 3);
    assert_eq!(edit_rolling(&chars("intention"), &chars("execution")), 5);
    assert_eq!(edit_tab(&chars("héllo"), &chars("hello")), 1); // one edit by chars, would be 2 by bytes
}

// ================================================================================================
/// Q7. WORD BREAK. Can `s` be segmented into a space-separated sequence of dictionary words (words reusable)?
///   "leetcode", ["leet","code"] -> true;   "catsandog", ["cats","dog","sand","and","cat"] -> false
///
/// APPROACH. Brute force: recursion trying every prefix, O(2^n). DP state `dp[i]` = `s[..i]` is segmentable:
/// `dp[i] = OR over j < i of dp[j] && dict.contains(s[j..i])`. O(n^2 * L) with hashing (bounding j by the longest word
/// length gives O(n * maxlen)). Pattern: prefix DP, "last word" choice. Follow-up Word Break II (return all
/// sentences) needs backtracking + memoisation.
/// RUST. Store the dictionary as `HashSet<&str>` and slice `&s[j..i]` (ASCII or char-boundary-safe positions only; for Unicode
/// iterate over `char_indices` boundaries). `dp[0] = true` (empty prefix) is the base case.
/// INTERVIEWER IS TESTING: the base case, the complexity of the substring hashing, and the max-word-length bound.
fn word_break(s: &str, dict: &[&str]) -> bool {
    let words: HashSet<&str> = dict.iter().copied().collect();
    let max_len = dict.iter().map(|w| w.len()).max().unwrap_or(0);
    let mut dp = vec![false; s.len() + 1];
    dp[0] = true;
    for i in 1..=s.len() {
        for j in i.saturating_sub(max_len)..i {
            if dp[j] && words.contains(&s[j..i]) {
                dp[i] = true;
                break;
            }
        }
    }
    dp[s.len()]
}
fn word_break_brute(s: &str, dict: &[&str]) -> bool {
    s.is_empty()
        || dict
            .iter()
            .any(|w| s.starts_with(w) && !w.is_empty() && word_break_brute(&s[w.len()..], dict))
}
fn q7() {
    let mut rng = Rng::new(7);
    for _ in 0..CASES {
        let s = rng.string("ab", 10);
        let words: Vec<String> = (0..1 + rng.below(3))
            .map(|_| rng.string("ab", 3))
            .filter(|w| !w.is_empty())
            .collect();
        let dict: Vec<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(
            word_break(&s, &dict),
            word_break_brute(&s, &dict),
            "{s:?} {dict:?}"
        );
    }
    assert!(word_break("leetcode", &["leet", "code"]));
    assert!(!word_break(
        "catsandog",
        &["cats", "dog", "sand", "and", "cat"]
    ));
    assert!(word_break("applepenapple", &["apple", "pen"]));
}

// ================================================================================================
/// Q8. DECODE WAYS. A message of digits maps 'A'=1 ... 'Z'=26. Count the decodings of a digit string.
///   "12" -> 2 (AB, L);   "226" -> 3;   "06" -> 0;   "10" -> 1;   "100" -> 0
///
/// APPROACH. Brute force: recursion taking one or two digits, O(2^n). DP state `dp[i]` = ways to decode the first i digits:
/// add `dp[i-1]` if digit i is not '0'; add `dp[i-2]` if the two-digit number is in 10..=26. O(n) time, O(1) space with
/// two rolling variables. Pattern: 1-D DP like climbing stairs with validity guards on each step.
/// RUST. Read digits as `bytes`, subtract `b'0'`; use `u64` (counts grow like Fibonacci). Leading zero string -> 0.
/// INTERVIEWER IS TESTING: zero handling ("0", "30", "100") and an empty input policy (here 0).
fn decode_ways(s: &str) -> u64 {
    let b = s.as_bytes();
    if b.is_empty() || b[0] == b'0' {
        return 0;
    }
    let (mut prev2, mut prev1) = (1u64, 1u64); // dp[i-2], dp[i-1] with dp[0] = dp[1] = 1
    for i in 1..b.len() {
        let mut cur = 0;
        if b[i] != b'0' {
            cur += prev1;
        }
        let two = (b[i - 1] - b'0') * 10 + (b[i] - b'0');
        if (10..=26).contains(&two) {
            cur += prev2;
        }
        (prev2, prev1) = (prev1, cur);
    }
    prev1
}
fn decode_brute(b: &[u8]) -> u64 {
    if b.is_empty() {
        return 1;
    }
    if b[0] == b'0' {
        return 0;
    }
    let mut ways = decode_brute(&b[1..]);
    if b.len() >= 2 && (b[0] - b'0') * 10 + (b[1] - b'0') <= 26 {
        ways += decode_brute(&b[2..]);
    }
    ways
}
fn q8() {
    let mut rng = Rng::new(8);
    for _ in 0..CASES {
        let s = rng.string("0123456789", 10);
        let want = if s.is_empty() {
            0
        } else {
            decode_brute(s.as_bytes())
        };
        assert_eq!(decode_ways(&s), want, "{s:?}");
    }
    for (s, want) in [
        ("12", 2),
        ("226", 3),
        ("06", 0),
        ("10", 1),
        ("100", 0),
        ("27", 1),
        ("2101", 1),
    ] {
        assert_eq!(decode_ways(s), want, "{s}");
    }
}

// ================================================================================================
/// Q9. UNIQUE PATHS I (m x n grid, move right/down), UNIQUE PATHS II (with obstacles) and MINIMUM PATH SUM.
///   3x7 -> 28;   obstacle grid [[0,0,0],[0,1,0],[0,0,0]] -> 2;   cost [[1,3,1],[1,5,1],[4,2,1]] -> 7
///
/// APPROACH. Brute force: recursion right/down, exponential. DP state `dp[r][c]` = paths (or min cost) to reach the
/// cell = top + left. Row r depends only on row r-1 so ONE ROW suffices: update in place left to right (`dp[c] += dp[c-1]`
/// where `dp[c]` still holds the cell above). O(mn) time, O(n) space. Unobstructed count is also the closed form
/// C(m+n-2, m-1). Memoisation variants (`Vec<Vec<Option<_>>>` and `HashMap`) are shown for paths I and agree.
/// RUST. First row/column bases: seed the row with 1s (or handle `c == 0` separately to avoid `c - 1` underflow on
/// `usize`). Counts overflow `u64` for large grids: say so, or use `u128`/modulo.
/// INTERVIEWER IS TESTING: obstacle in the start/end cell, 1 x n grids, and in-place row reuse.
fn unique_paths(m: usize, n: usize) -> u64 {
    let mut dp = vec![1u64; n];
    for _ in 1..m {
        for c in 1..n {
            dp[c] += dp[c - 1];
        }
    }
    dp[n - 1]
}
fn unique_paths_memo(r: usize, c: usize, memo: &mut Vec<Vec<Option<u64>>>) -> u64 {
    if r == 0 || c == 0 {
        return 1;
    }
    if let Some(v) = memo[r][c] {
        return v;
    }
    let v = unique_paths_memo(r - 1, c, memo) + unique_paths_memo(r, c - 1, memo);
    memo[r][c] = Some(v);
    v
}
fn unique_paths_hash(r: usize, c: usize, memo: &mut HashMap<(usize, usize), u64>) -> u64 {
    if r == 0 || c == 0 {
        return 1;
    }
    if let Some(&v) = memo.get(&(r, c)) {
        return v;
    }
    let v = unique_paths_hash(r - 1, c, memo) + unique_paths_hash(r, c - 1, memo);
    memo.insert((r, c), v);
    v
}
fn unique_paths_obstacles(g: &[Vec<u8>]) -> u64 {
    let n = g[0].len();
    let mut dp = vec![0u64; n];
    dp[0] = 1;
    for row in g {
        for c in 0..n {
            if row[c] == 1 {
                dp[c] = 0;
            } else if c > 0 {
                dp[c] += dp[c - 1];
            }
        }
    }
    dp[n - 1]
}
fn paths_brute(g: &[Vec<u8>], r: usize, c: usize) -> u64 {
    if g[r][c] == 1 {
        return 0;
    }
    if r == g.len() - 1 && c == g[0].len() - 1 {
        return 1;
    }
    let down = if r + 1 < g.len() {
        paths_brute(g, r + 1, c)
    } else {
        0
    };
    let right = if c + 1 < g[0].len() {
        paths_brute(g, r, c + 1)
    } else {
        0
    };
    down + right
}
fn min_path_sum(g: &[Vec<u64>]) -> u64 {
    let n = g[0].len();
    let mut dp = vec![u64::MAX; n];
    dp[0] = 0;
    for row in g {
        for c in 0..n {
            let best_prev = if c > 0 { dp[c].min(dp[c - 1]) } else { dp[c] };
            dp[c] = best_prev + row[c];
        }
    }
    dp[n - 1]
}
fn min_path_brute(g: &[Vec<u64>], r: usize, c: usize) -> u64 {
    let (rows, cols) = (g.len(), g[0].len());
    if r == rows - 1 && c == cols - 1 {
        return g[r][c];
    }
    let mut best = u64::MAX;
    if r + 1 < rows {
        best = best.min(min_path_brute(g, r + 1, c));
    }
    if c + 1 < cols {
        best = best.min(min_path_brute(g, r, c + 1));
    }
    g[r][c] + best
}
fn q9() {
    let mut rng = Rng::new(9);
    for _ in 0..CASES {
        let (m, n) = (1 + rng.below(5), 1 + rng.below(5));
        let grid: Vec<Vec<u8>> = (0..m)
            .map(|_| (0..n).map(|_| u8::from(rng.below(4) == 0)).collect())
            .collect();
        assert_eq!(
            unique_paths_obstacles(&grid),
            paths_brute(&grid, 0, 0),
            "{grid:?}"
        );
        let free = vec![vec![0u8; n]; m];
        let want = paths_brute(&free, 0, 0);
        assert_eq!(unique_paths(m, n), want);
        assert_eq!(
            unique_paths_memo(m - 1, n - 1, &mut vec![vec![None; n]; m]),
            want
        );
        assert_eq!(unique_paths_hash(m - 1, n - 1, &mut HashMap::new()), want);
        let cost: Vec<Vec<u64>> = (0..m)
            .map(|_| (0..n).map(|_| rng.below(9) as u64).collect())
            .collect();
        assert_eq!(min_path_sum(&cost), min_path_brute(&cost, 0, 0), "{cost:?}");
    }
    assert_eq!(unique_paths(3, 7), 28);
    assert_eq!(
        unique_paths_obstacles(&[vec![0, 0, 0], vec![0, 1, 0], vec![0, 0, 0]]),
        2
    );
    assert_eq!(unique_paths_obstacles(&[vec![1]]), 0);
    assert_eq!(
        min_path_sum(&[vec![1, 3, 1], vec![1, 5, 1], vec![4, 2, 1]]),
        7
    );
}

// ================================================================================================
/// Q10. MAXIMAL SQUARE. In a 0/1 matrix find the area of the largest square of all 1s.
///   [[1,0,1,0,0],[1,0,1,1,1],[1,1,1,1,1],[1,0,0,1,0]] -> 4
///
/// APPROACH. Brute force: every top-left corner and growing size with an all-ones check, O((mn)^2 * min) or better with
/// prefix sums. DP state `dp[r][c]` = side of the largest all-ones square whose BOTTOM-RIGHT corner is (r, c):
/// `min(up, left, up-left) + 1` when the cell is 1, else 0. Answer = max side squared. O(mn) time, O(n) space with one
/// row plus a saved diagonal variable.
/// RUST. The in-place row update must save the old `dp[c]` (the diagonal for the next cell) BEFORE overwriting it: keep
/// `prev_diag` in a local. Use `usize` and index `c - 1` only under `c > 0`.
/// INTERVIEWER IS TESTING: the min-of-three recurrence and why it works (all three neighbours bound the square).
fn maximal_square(g: &[Vec<u8>]) -> usize {
    let n = g[0].len();
    let mut dp = vec![0usize; n + 1]; // dp[c + 1] is column c; dp[0] is a zero sentinel
    let mut best = 0;
    for row in g {
        let mut prev_diag = 0;
        for c in 0..n {
            let up = dp[c + 1];
            dp[c + 1] = if row[c] == 1 {
                1 + up.min(dp[c]).min(prev_diag)
            } else {
                0
            };
            best = best.max(dp[c + 1]);
            prev_diag = up;
        }
    }
    best * best
}
fn q10() {
    let mut rng = Rng::new(10);
    for _ in 0..CASES {
        let (m, n) = (1 + rng.below(6), 1 + rng.below(6));
        let g: Vec<Vec<u8>> = (0..m)
            .map(|_| (0..n).map(|_| u8::from(rng.below(4) != 0)).collect())
            .collect();
        let mut want = 0;
        for r in 0..m {
            for c in 0..n {
                for side in 1..=(m - r).min(n - c) {
                    if (r..r + side).all(|i| (c..c + side).all(|j| g[i][j] == 1)) {
                        want = want.max(side * side);
                    }
                }
            }
        }
        assert_eq!(maximal_square(&g), want, "{g:?}");
    }
    let g = vec![
        vec![1, 0, 1, 0, 0],
        vec![1, 0, 1, 1, 1],
        vec![1, 1, 1, 1, 1],
        vec![1, 0, 0, 1, 0],
    ];
    assert_eq!(maximal_square(&g), 4);
    assert_eq!(maximal_square(&[vec![0]]), 0);
}

// ================================================================================================
/// Q11. 0/1 KNAPSACK, PARTITION EQUAL SUBSET SUM and TARGET SUM.
///   knapsack weights [1,3,4,5] values [1,4,5,7] cap 7 -> 9;   partition [1,5,11,5] -> true;
///   target sum nums [1,1,1,1,1] target 3 -> 5 (assign + or - to each number)
///
/// APPROACH. Brute force: all subsets / all sign assignments, O(2^n). Knapsack DP state `dp[w]` = best value with capacity
/// w using items so far: `dp[w] = max(dp[w], dp[w - wt] + val)`. In 2-D `dp[i][w]` it is the textbook table; in 1-D the
/// capacity loop MUST run DOWNWARDS so each item is used at most once (upwards = unbounded knapsack, a different problem).
/// Partition: it is a subset-sum feasibility question, target = total / 2 (odd total -> false), as a boolean knapsack.
/// Target sum: with P the sum of the + group, `P - (S - P) = t` so `P = (S + t) / 2`: count subsets with sum P (0/1 knapsack
/// counting), after checking `|t| <= S` and parity. O(n * cap) time, O(cap) space. Tabulation 2-D, memoisation (Vec<Option>
/// and HashMap) and the rolling 1-D version are all asserted equal for knapsack.
/// RUST. Descending loop `(wt..=cap).rev()`; guard `wt > cap` (empty range is fine). `dp[w - wt]` cannot underflow because
/// the range starts at `wt`. Zero-valued items in Target Sum: the descending loop correctly doubles the count.
/// INTERVIEWER IS TESTING: the reverse loop direction and the reduction of Target Sum to subset-sum counting.
fn knap_1d(wt: &[usize], val: &[u64], cap: usize) -> u64 {
    let mut dp = vec![0u64; cap + 1];
    for i in 0..wt.len() {
        for w in (wt[i]..=cap).rev() {
            dp[w] = dp[w].max(dp[w - wt[i]] + val[i]);
        }
    }
    dp[cap]
}
fn knap_2d(wt: &[usize], val: &[u64], cap: usize) -> u64 {
    let mut dp = vec![vec![0u64; cap + 1]; wt.len() + 1];
    for i in 1..=wt.len() {
        for w in 0..=cap {
            dp[i][w] = dp[i - 1][w];
            if w >= wt[i - 1] {
                dp[i][w] = dp[i][w].max(dp[i - 1][w - wt[i - 1]] + val[i - 1]);
            }
        }
    }
    dp[wt.len()][cap]
}
fn knap_memo_vec(
    i: usize,
    cap: usize,
    wt: &[usize],
    val: &[u64],
    memo: &mut Vec<Vec<Option<u64>>>,
) -> u64 {
    if i == wt.len() {
        return 0;
    }
    if let Some(v) = memo[i][cap] {
        return v;
    }
    let mut best = knap_memo_vec(i + 1, cap, wt, val, memo);
    if wt[i] <= cap {
        best = best.max(val[i] + knap_memo_vec(i + 1, cap - wt[i], wt, val, memo));
    }
    memo[i][cap] = Some(best);
    best
}
fn knap_memo_hash(
    i: usize,
    cap: usize,
    wt: &[usize],
    val: &[u64],
    memo: &mut HashMap<(usize, usize), u64>,
) -> u64 {
    if i == wt.len() {
        return 0;
    }
    if let Some(&v) = memo.get(&(i, cap)) {
        return v;
    }
    let mut best = knap_memo_hash(i + 1, cap, wt, val, memo);
    if wt[i] <= cap {
        best = best.max(val[i] + knap_memo_hash(i + 1, cap - wt[i], wt, val, memo));
    }
    memo.insert((i, cap), best);
    best
}
fn can_partition(nums: &[usize]) -> bool {
    let total: usize = nums.iter().sum();
    if total % 2 == 1 {
        return false;
    }
    let half = total / 2;
    let mut reach = vec![false; half + 1];
    reach[0] = true;
    for &x in nums {
        for s in (x..=half).rev() {
            reach[s] |= reach[s - x];
        }
    }
    reach[half]
}
fn target_sum(nums: &[usize], t: i64) -> u64 {
    let total: i64 = nums.iter().sum::<usize>() as i64;
    if t.abs() > total || (total + t) % 2 != 0 {
        return 0;
    }
    let p = ((total + t) / 2) as usize;
    let mut dp = vec![0u64; p + 1];
    dp[0] = 1;
    for &x in nums {
        for s in (x..=p).rev() {
            dp[s] += dp[s - x];
        }
    }
    dp[p]
}
fn q11() {
    let mut rng = Rng::new(11);
    for _ in 0..CASES {
        let n = rng.below(9);
        let wt: Vec<usize> = (0..n).map(|_| 1 + rng.below(6)).collect();
        let val: Vec<u64> = (0..n).map(|_| rng.below(10) as u64).collect();
        let cap = rng.below(15);
        let mut want = 0;
        for mask in 0u32..1 << n {
            let w: usize = (0..n).filter(|&i| mask >> i & 1 == 1).map(|i| wt[i]).sum();
            if w <= cap {
                want = want.max(
                    (0..n)
                        .filter(|&i| mask >> i & 1 == 1)
                        .map(|i| val[i])
                        .sum::<u64>(),
                );
            }
        }
        assert_eq!(knap_1d(&wt, &val, cap), want, "{wt:?} {val:?} {cap}");
        assert_eq!(knap_2d(&wt, &val, cap), want);
        assert_eq!(
            knap_memo_vec(0, cap, &wt, &val, &mut vec![vec![None; cap + 1]; n]),
            want
        );
        assert_eq!(knap_memo_hash(0, cap, &wt, &val, &mut HashMap::new()), want);
        let nums: Vec<usize> = (0..n).map(|_| rng.below(8)).collect();
        let total: usize = nums.iter().sum();
        let can = (0u32..1 << n).any(|m| {
            2 * (0..n)
                .filter(|&i| m >> i & 1 == 1)
                .map(|i| nums[i])
                .sum::<usize>()
                == total
        });
        assert_eq!(can_partition(&nums), can, "{nums:?}");
        let t = rng.int(-12, 12);
        let ways = (0u32..1 << n)
            .filter(|&m| {
                (0..n)
                    .map(|i| {
                        if m >> i & 1 == 1 {
                            nums[i] as i64
                        } else {
                            -(nums[i] as i64)
                        }
                    })
                    .sum::<i64>()
                    == t
            })
            .count() as u64;
        assert_eq!(target_sum(&nums, t), ways, "{nums:?} {t}");
    }
    assert_eq!(knap_1d(&[1, 3, 4, 5], &[1, 4, 5, 7], 7), 9);
    assert!(can_partition(&[1, 5, 11, 5]));
    assert!(!can_partition(&[1, 2, 3, 5]));
    assert_eq!(target_sum(&[1, 1, 1, 1, 1], 3), 5);
    assert_eq!(target_sum(&[0, 0, 1], 1), 4);
}

// ================================================================================================
/// Q12. LONGEST PALINDROMIC SUBSEQUENCE (not contiguous).   "bbbab" -> 4 ("bbbb");   "cbbd" -> 2
///
/// APPROACH. Brute force: all subsequences, O(2^n). INTERVAL DP: `dp[i][j]` = answer for `s[i..=j]`: if the ends match
/// `dp[i+1][j-1] + 2` else `max(dp[i+1][j], dp[i][j-1])`; base `dp[i][i] = 1`. Fill with i descending, j ascending (the
/// cells it needs are below / to the left). O(n^2) time and O(n) space with a rolling row. Equivalent: LCS of `s` and
/// reverse(s). Minimum insertions to make a string a palindrome = n - this answer.
/// RUST. Iterate `i` from `n - 1` down to 0 with `(0..n).rev()`; never write `for i in n-1..=0`. Cells where `j < i` are
/// treated as 0, which a zero-initialised table gives for free.
/// INTERVIEWER IS TESTING: iteration order of an interval DP, and the LCS-with-reverse equivalence.
fn lps(s: &[u8]) -> usize {
    let n = s.len();
    let mut dp = vec![vec![0usize; n]; n];
    for i in (0..n).rev() {
        dp[i][i] = 1;
        for j in i + 1..n {
            dp[i][j] = if s[i] == s[j] {
                dp[i + 1][j - 1] + 2
            } else {
                dp[i + 1][j].max(dp[i][j - 1])
            };
        }
    }
    if n == 0 { 0 } else { dp[0][n - 1] }
}
fn q12() {
    let mut rng = Rng::new(12);
    for _ in 0..CASES {
        let s = rng.string("abc", 10);
        let b = s.as_bytes();
        let mut want = 0;
        for mask in 0u32..1 << b.len() {
            let sub: Vec<u8> = (0..b.len())
                .filter(|&i| mask >> i & 1 == 1)
                .map(|i| b[i])
                .collect();
            if sub.iter().eq(sub.iter().rev()) {
                want = want.max(sub.len());
            }
        }
        assert_eq!(lps(b), want, "{s:?}");
        let rev: Vec<u8> = b.iter().rev().copied().collect();
        assert_eq!(lcs_rolling(b, &rev), want);
    }
    assert_eq!(lps(b"bbbab"), 4);
    assert_eq!(lps(b"cbbd"), 2);
}

// ================================================================================================
/// Q13. BEST TIME TO BUY AND SELL STOCK WITH COOLDOWN. Unlimited transactions, but after selling you must wait one day.
///   [1,2,3,0,2] -> 3 (buy, sell, cooldown, buy, sell)
///
/// APPROACH. Brute force: recursion over (day, holding). STATE MACHINE DP with three states per day: `hold` (own a
/// share), `sold` (just sold today), `rest` (own nothing, free to buy). Transitions: `hold' = max(hold, rest - p)`,
/// `sold' = hold + p`, `rest' = max(rest, sold)`. Answer = max(sold, rest) on the last day. O(n) time, O(1) space.
/// Pattern: model "what am I allowed to do next" as states, one variable per state.
/// RUST. Initialise `hold` to a large negative sentinel (`i64::MIN / 2`) so subtracting a price cannot overflow; tuple
/// assignment `(hold, sold, rest) = (...)` updates all three from OLD values at once.
/// INTERVIEWER IS TESTING: drawing the state diagram first; using old values for all updates.
fn stock_cooldown(p: &[i64]) -> i64 {
    let (mut hold, mut sold, mut rest) = (i64::MIN / 2, 0i64, 0i64);
    for &x in p {
        (hold, sold, rest) = (hold.max(rest - x), hold + x, rest.max(sold));
    }
    sold.max(rest)
}
fn stock_cooldown_brute(p: &[i64], i: usize, holding: bool) -> i64 {
    if i >= p.len() {
        return 0;
    }
    let skip = stock_cooldown_brute(p, i + 1, holding);
    if holding {
        skip.max(p[i] + stock_cooldown_brute(p, i + 2, false))
    } else {
        skip.max(-p[i] + stock_cooldown_brute(p, i + 1, true))
    }
}
fn q13() {
    let mut rng = Rng::new(13);
    for _ in 0..CASES {
        let n = rng.below(11);
        let p = rng.vec(n, 0, 9);
        assert_eq!(
            stock_cooldown(&p),
            stock_cooldown_brute(&p, 0, false),
            "{p:?}"
        );
    }
    assert_eq!(stock_cooldown(&[1, 2, 3, 0, 2]), 3);
    assert_eq!(stock_cooldown(&[1]), 0);
}

// ================================================================================================
/// Q14. JUMP GAME I (can you reach the last index; `nums[i]` = max jump length) and II (minimum number of jumps).
///   [2,3,1,1,4] -> true, 2 jumps;   [3,2,1,0,4] -> false
///
/// APPROACH. Brute force / DP: `reach[i] = any j < i reachable with j + nums[j] >= i` (min jumps: `dp[i] = min dp[j] + 1`),
/// O(n^2). GREEDY O(n): track the FARTHEST index reachable; if the scan index passes it, you are stuck (I). For II treat
/// it as BFS LEVELS: the current level ends at `cur_end`; when the scan reaches `cur_end` you must jump once more and
/// the next level ends at the farthest reach seen. O(n) time, O(1) space.
/// RUST. `i + nums[i]` as `usize` cannot go negative so no underflow, but ensure the loop stops at `n - 1`
/// for II (we never need to jump from the last index). Return `Option<usize>` for the unreachable case.
/// INTERVIEWER IS TESTING: can you justify the greedy (farthest reach is monotone) and the level boundary in II.
fn can_jump(nums: &[usize]) -> bool {
    let mut far = 0;
    for (i, &x) in nums.iter().enumerate() {
        if i > far {
            return false;
        }
        far = far.max(i + x);
    }
    true
}
fn min_jumps(nums: &[usize]) -> Option<usize> {
    let n = nums.len();
    if n == 1 {
        return Some(0);
    }
    let (mut jumps, mut cur_end, mut far) = (0, 0, 0);
    for i in 0..n - 1 {
        far = far.max(i + nums[i]);
        if i == cur_end {
            if far <= i {
                return None;
            }
            jumps += 1;
            cur_end = far;
            if cur_end >= n - 1 {
                return Some(jumps);
            }
        }
    }
    None
}
fn min_jumps_dp(nums: &[usize]) -> Option<usize> {
    let n = nums.len();
    let mut dp = vec![INF; n];
    dp[0] = 0;
    for i in 1..n {
        for j in 0..i {
            if dp[j] != INF && j + nums[j] >= i {
                dp[i] = dp[i].min(dp[j] + 1);
            }
        }
    }
    (dp[n - 1] != INF).then_some(dp[n - 1])
}
fn q14() {
    let mut rng = Rng::new(14);
    for _ in 0..CASES {
        let n = 1 + rng.below(10);
        let v: Vec<usize> = (0..n).map(|_| rng.below(4)).collect();
        let want = min_jumps_dp(&v);
        assert_eq!(min_jumps(&v), want, "{v:?}");
        assert_eq!(can_jump(&v), want.is_some(), "{v:?}");
    }
    assert!(can_jump(&[2, 3, 1, 1, 4]));
    assert!(!can_jump(&[3, 2, 1, 0, 4]));
    assert_eq!(min_jumps(&[2, 3, 1, 1, 4]), Some(2));
    assert_eq!(min_jumps(&[0]), Some(0));
}

// ================================================================================================
/// Q15. MAXIMUM PRODUCT SUBARRAY as a DP TABLE (compare with the rolling-variable version in the arrays recipe).
///   [2,3,-2,4] -> 6;   [-2,3,-4] -> 24
///
/// APPROACH. State per index: `mx[i]` and `mn[i]` = max and min product of a subarray ENDING at i. Because multiplying by a
/// negative swaps max and min, each needs both predecessors: `mx[i] = max(x, x*mx[i-1], x*mn[i-1])`, `mn[i] = min(...)`.
/// Answer = max over `mx`. O(n) time and space as written; keep only the last pair for O(1). Brute force: O(n^2).
/// RUST. Tables of `i64` initialised from the first element; do not start `mx` at 0 (an all-negative single element would
/// be wrong). Overflow is possible for long inputs, mention `checked_mul` or `i128`.
/// INTERVIEWER IS TESTING: why a single table fails and zeros restart the run.
fn max_product_table(nums: &[i64]) -> i64 {
    let n = nums.len();
    let (mut mx, mut mn) = (vec![nums[0]; n], vec![nums[0]; n]);
    for i in 1..n {
        let (a, b) = (nums[i] * mx[i - 1], nums[i] * mn[i - 1]);
        mx[i] = nums[i].max(a).max(b);
        mn[i] = nums[i].min(a).min(b);
    }
    *mx.iter().max().unwrap()
}
fn q15() {
    let mut rng = Rng::new(15);
    for _ in 0..CASES {
        let n = 1 + rng.below(9);
        let v = rng.vec(n, -3, 3);
        let mut want = i64::MIN;
        for l in 0..n {
            for r in l + 1..=n {
                want = want.max(v[l..r].iter().product());
            }
        }
        assert_eq!(max_product_table(&v), want, "{v:?}");
    }
    assert_eq!(max_product_table(&[2, 3, -2, 4]), 6);
    assert_eq!(max_product_table(&[-2, 3, -4]), 24);
}

// ================================================================================================
/// Q16. REGULAR EXPRESSION MATCHING (`.` any one char, `x*` zero or more x) and WILDCARD MATCHING (`?` any one char,
/// `*` any sequence). Whole-string match.
///   regex: "aa" vs "a*" -> true; "ab" vs ".*" -> true; "mississippi" vs "mis*is*p*." -> false
///   wildcard: "adceb" vs "*a*b" -> true; "acdcb" vs "a*c?b" -> false
///
/// APPROACH. Brute force: recursion on the first characters, exponential for stars. DP state `dp[i][j]` = `s[..i]` matches
/// `p[..j]`. Regex: if `p[j-1] == '*'` then `dp[i][j] = dp[i][j-2]` (zero copies) `|| (s[i-1] matches p[j-2] && dp[i-1][j])`
/// (one more copy); else `s[i-1] matches p[j-1] && dp[i-1][j-1]`. Base row: patterns like `a*b*` match the empty string.
/// Wildcard: `*` matches empty (`dp[i][j-1]`) or one more char (`dp[i-1][j]`). O(nm) time. Wildcard also has an O(1)-space
/// GREEDY: remember the last `*` and where it started matching, backtrack to it on mismatch; worst case O(nm), usually linear.
/// RUST. Patterns as `&[u8]`; `j >= 2` is guaranteed in valid regexes (a `*` always follows a char) but check it, because
/// `j - 2` on `usize` panics. The oracles are the textbook recursive matchers, independent of the tables.
/// INTERVIEWER IS TESTING: the empty-prefix base row, and why `.*` / leading `*` behave differently in the two problems.
fn regex_match(s: &[u8], p: &[u8]) -> bool {
    let (n, m) = (s.len(), p.len());
    let mut dp = vec![vec![false; m + 1]; n + 1];
    dp[0][0] = true;
    for j in 2..=m {
        dp[0][j] = p[j - 1] == b'*' && dp[0][j - 2];
    }
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = if p[j - 1] == b'*' {
                j >= 2
                    && (dp[i][j - 2]
                        || ((p[j - 2] == b'.' || p[j - 2] == s[i - 1]) && dp[i - 1][j]))
            } else {
                (p[j - 1] == b'.' || p[j - 1] == s[i - 1]) && dp[i - 1][j - 1]
            };
        }
    }
    dp[n][m]
}
fn regex_rec(s: &[u8], p: &[u8]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    let first = !s.is_empty() && (p[0] == s[0] || p[0] == b'.');
    if p.len() >= 2 && p[1] == b'*' {
        regex_rec(s, &p[2..]) || (first && regex_rec(&s[1..], p))
    } else {
        first && regex_rec(&s[1..], &p[1..])
    }
}
fn wildcard_dp(s: &[u8], p: &[u8]) -> bool {
    let (n, m) = (s.len(), p.len());
    let mut dp = vec![vec![false; m + 1]; n + 1];
    dp[0][0] = true;
    for j in 1..=m {
        dp[0][j] = p[j - 1] == b'*' && dp[0][j - 1];
    }
    for i in 1..=n {
        for j in 1..=m {
            dp[i][j] = if p[j - 1] == b'*' {
                dp[i][j - 1] || dp[i - 1][j]
            } else {
                (p[j - 1] == b'?' || p[j - 1] == s[i - 1]) && dp[i - 1][j - 1]
            };
        }
    }
    dp[n][m]
}
fn wildcard_greedy(s: &[u8], p: &[u8]) -> bool {
    let (mut i, mut j) = (0, 0);
    let mut star: Option<(usize, usize)> = None; // (pattern index after '*', text index it currently absorbs up to)
    while i < s.len() {
        if j < p.len() && (p[j] == b'?' || p[j] == s[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == b'*' {
            star = Some((j + 1, i));
            j += 1;
        } else if let Some((sj, si)) = star {
            star = Some((sj, si + 1));
            (j, i) = (sj, si + 1);
        } else {
            return false;
        }
    }
    p[j..].iter().all(|&c| c == b'*')
}
fn wildcard_rec(s: &[u8], p: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') => wildcard_rec(s, &p[1..]) || (!s.is_empty() && wildcard_rec(&s[1..], p)),
        Some(&c) => !s.is_empty() && (c == b'?' || c == s[0]) && wildcard_rec(&s[1..], &p[1..]),
    }
}
fn q16() {
    let mut rng = Rng::new(16);
    for _ in 0..CASES {
        let s = rng.string("ab", 7);
        let mut re = String::new();
        for _ in 0..rng.below(5) {
            re.push(['a', 'b', '.'][rng.below(3)]);
            if rng.below(2) == 0 {
                re.push('*');
            }
        }
        assert_eq!(
            regex_match(s.as_bytes(), re.as_bytes()),
            regex_rec(s.as_bytes(), re.as_bytes()),
            "{s:?} {re:?}"
        );
        let w = rng.string("ab?*", 6);
        let want = wildcard_rec(s.as_bytes(), w.as_bytes());
        assert_eq!(wildcard_dp(s.as_bytes(), w.as_bytes()), want, "{s:?} {w:?}");
        assert_eq!(
            wildcard_greedy(s.as_bytes(), w.as_bytes()),
            want,
            "{s:?} {w:?}"
        );
    }
    assert!(regex_match(b"aa", b"a*"));
    assert!(regex_match(b"ab", b".*"));
    assert!(!regex_match(b"mississippi", b"mis*is*p*."));
    assert!(regex_match(b"", b"a*b*"));
    assert!(wildcard_dp(b"adceb", b"*a*b") && wildcard_greedy(b"adceb", b"*a*b"));
    assert!(!wildcard_dp(b"acdcb", b"a*c?b") && !wildcard_greedy(b"acdcb", b"a*c?b"));
}

// ================================================================================================
/// Q17. INTERLEAVING STRING (is `s3` an interleaving of `s1` and `s2`, each kept in order?) and DISTINCT SUBSEQUENCES
/// (how many subsequences of `s` equal `t`?).
///   interleave "aabcc","dbbca","aadbbcbcac" -> true;   distinct "rabbbit","rabbit" -> 3
///
/// APPROACH. Interleaving: length check first, then `dp[i][j]` = `s3[..i+j]` is an interleaving of `s1[..i]`, `s2[..j]`:
/// `(dp[i-1][j] && s1[i-1] == s3[i+j-1]) || (dp[i][j-1] && s2[j-1] == s3[i+j-1])`; one rolling row, O(nm) time, O(m) space.
/// Distinct subsequences: `dp[j]` = ways to form `t[..j]` from the processed prefix of `s`; for each char of s, scan j DOWNWARDS
/// and add `dp[j-1]` when `s[i] == t[j-1]` (descending so each char of s is used at most once per target position).
/// Brute force for both: recursion on the first characters. Pattern: 2-D sequence DP like LCS.
/// RUST. `u64` for counts; base `dp[0] = 1` (empty target). The descending loop is the same trick as 0/1 knapsack.
/// INTERVIEWER IS TESTING: the length precheck, rolling-row direction, and spotting the knapsack similarity.
fn is_interleave(a: &[u8], b: &[u8], c: &[u8]) -> bool {
    if a.len() + b.len() != c.len() {
        return false;
    }
    let mut dp = vec![false; b.len() + 1];
    for i in 0..=a.len() {
        for j in 0..=b.len() {
            dp[j] = if i == 0 && j == 0 {
                true
            } else {
                (i > 0 && dp[j] && a[i - 1] == c[i + j - 1])
                    || (j > 0 && dp[j - 1] && b[j - 1] == c[i + j - 1])
            };
        }
    }
    dp[b.len()]
}
fn interleave_rec(a: &[u8], b: &[u8], c: &[u8]) -> bool {
    match c.first() {
        None => a.is_empty() && b.is_empty(),
        Some(&x) => {
            (a.first() == Some(&x) && interleave_rec(&a[1..], b, &c[1..]))
                || (b.first() == Some(&x) && interleave_rec(a, &b[1..], &c[1..]))
        }
    }
}
fn num_distinct(s: &[u8], t: &[u8]) -> u64 {
    let mut dp = vec![0u64; t.len() + 1];
    dp[0] = 1;
    for &c in s {
        for j in (1..=t.len()).rev() {
            if t[j - 1] == c {
                dp[j] += dp[j - 1];
            }
        }
    }
    dp[t.len()]
}
fn distinct_rec(s: &[u8], t: &[u8]) -> u64 {
    if t.is_empty() {
        return 1;
    }
    if s.len() < t.len() {
        return 0;
    }
    distinct_rec(&s[1..], t)
        + if s[0] == t[0] {
            distinct_rec(&s[1..], &t[1..])
        } else {
            0
        }
}
fn q17() {
    let mut rng = Rng::new(17);
    for _ in 0..CASES {
        let (a, b) = (rng.string("ab", 5), rng.string("ab", 5));
        let mut c: Vec<u8> = a.bytes().chain(b.bytes()).collect();
        if rng.below(2) == 0 {
            rng.shuffle(&mut c); // sometimes a true interleaving by luck, mostly not
        }
        assert_eq!(
            is_interleave(a.as_bytes(), b.as_bytes(), &c),
            interleave_rec(a.as_bytes(), b.as_bytes(), &c),
            "{a:?} {b:?} {c:?}"
        );
        let (s, t) = (rng.string("ab", 10), rng.string("ab", 4));
        assert_eq!(
            num_distinct(s.as_bytes(), t.as_bytes()),
            distinct_rec(s.as_bytes(), t.as_bytes()),
            "{s:?} {t:?}"
        );
    }
    assert!(is_interleave(b"aabcc", b"dbbca", b"aadbbcbcac"));
    assert!(!is_interleave(b"aabcc", b"dbbca", b"aadbbbaccc"));
    assert_eq!(num_distinct(b"rabbbit", b"rabbit"), 3);
    assert_eq!(num_distinct(b"babgbag", b"bag"), 5);
}

// ================================================================================================
/// Q18. BURST BALLOONS. Bursting balloon i earns `nums[i-1] * nums[i] * nums[i+1]` (out-of-range counts as 1). Maximise the
/// total coins over the best order.   [3,1,5,8] -> 167
///
/// APPROACH. Brute force: try every burst order, O(n!). The trick: think of the LAST balloon to burst in an interval. INTERVAL
/// DP over the open interval `(l, r)` of a padded array `[1, nums.., 1]`: `dp[l][r] = max over k in (l, r) of
/// dp[l][k] + dp[k][r] + a[l]*a[k]*a[r]` (k is last, so its neighbours are exactly l and r, and the two sides are
/// independent). Iterate by increasing interval length. O(n^3) time, O(n^2) space.
/// RUST. Pad with `1` on both ends (`Vec::with_capacity` + push). Loop `len` from 2 up to `n + 1`, compute `r = l + len`;
/// all indices stay in range and no subtraction can underflow.
/// INTERVIEWER IS TESTING: do you flip the question ("last to burst") so that subproblems stop interacting.
fn burst_balloons(nums: &[i64]) -> i64 {
    let mut a = vec![1];
    a.extend_from_slice(nums);
    a.push(1);
    let n = a.len();
    let mut dp = vec![vec![0i64; n]; n];
    for len in 2..n {
        for l in 0..n - len {
            let r = l + len;
            for k in l + 1..r {
                dp[l][r] = dp[l][r].max(dp[l][k] + dp[k][r] + a[l] * a[k] * a[r]);
            }
        }
    }
    dp[0][n - 1]
}
fn burst_brute(v: &mut Vec<i64>) -> i64 {
    if v.is_empty() {
        return 0;
    }
    let mut best = 0;
    for i in 0..v.len() {
        let left = if i > 0 { v[i - 1] } else { 1 };
        let right = if i + 1 < v.len() { v[i + 1] } else { 1 };
        let gain = left * v[i] * right;
        let x = v.remove(i);
        best = best.max(gain + burst_brute(v));
        v.insert(i, x);
    }
    best
}
fn q18() {
    let mut rng = Rng::new(18);
    for _ in 0..CASES {
        let n = rng.below(7);
        let v = rng.vec(n, 0, 9);
        assert_eq!(burst_balloons(&v), burst_brute(&mut v.clone()), "{v:?}");
    }
    assert_eq!(burst_balloons(&[3, 1, 5, 8]), 167);
    assert_eq!(burst_balloons(&[]), 0);
}

// ================================================================================================
/// Q19. EGG DROPPING. With `k` eggs and a building of `n` floors, what is the minimum number of drops needed in the WORST
/// case to find the critical floor?   k = 2, n = 100 -> 14;   k = 1, n = 5 -> 5;   k = 3, n = 14 -> 4
///
/// APPROACH. Classic DP `dp[e][f]` = worst-case drops with e eggs and f floors: try dropping from floor x:
/// `1 + max(dp[e-1][x-1] (breaks), dp[e][f-x] (survives))`, take the min over x. O(k n^2). Because `dp[e-1][x-1]` rises and
/// `dp[e][f-x]` falls with x, the best x is at their crossing: BINARY SEARCH x for O(k n log n). Better, FLIP THE DP: let
/// `f[m][e]` = the maximum number of floors you can fully resolve with m drops and e eggs; then
/// `f[m][e] = f[m-1][e-1] + f[m-1][e] + 1` (drop, then either below or above). Find the smallest m with `f[m][k] >= n`.
/// With rolling `f[e]` each drop costs O(k) and m is O(log n) when k is large (at most n, and about n^(1/k) for small k),
/// so roughly O(k log n). Brute force oracle: the O(k n^2) table (and a pure recursion for tiny inputs).
/// RUST. Update the 1-D array with `e` descending so `f[e-1]` is still the previous move's value; sizes are small `usize`.
/// INTERVIEWER IS TESTING: do you spot the monotone structure, and can you explain the "floors per drops" inversion.
fn egg_moves(k: usize, n: usize) -> usize {
    let mut f = vec![0usize; k + 1]; // f[e] = floors resolvable with e eggs and the current number of drops
    let mut m = 0;
    while f[k] < n {
        m += 1;
        for e in (1..=k).rev() {
            f[e] = f[e] + f[e - 1] + 1;
        }
    }
    m
}
fn egg_table(k: usize, n: usize) -> usize {
    let mut dp = vec![vec![0usize; n + 1]; k + 1];
    for f in 1..=n {
        dp[1][f] = f;
    }
    for e in 2..=k {
        for f in 1..=n {
            dp[e][f] = (1..=f)
                .map(|x| 1 + dp[e - 1][x - 1].max(dp[e][f - x]))
                .min()
                .unwrap();
        }
    }
    dp[k][n]
}
fn egg_rec(k: usize, n: usize) -> usize {
    if n == 0 || n == 1 {
        return n;
    }
    if k == 1 {
        return n;
    }
    (1..=n)
        .map(|x| 1 + egg_rec(k - 1, x - 1).max(egg_rec(k, n - x)))
        .min()
        .unwrap()
}
fn q19() {
    for k in 1..=3 {
        for n in 0..=9 {
            assert_eq!(egg_rec(k, n), egg_table(k, n), "k={k} n={n}");
        }
    }
    let mut rng = Rng::new(19);
    for _ in 0..CASES {
        let (k, n) = (1 + rng.below(5), rng.below(60));
        assert_eq!(egg_moves(k, n), egg_table(k, n), "k={k} n={n}");
    }
    assert_eq!(egg_moves(2, 100), 14);
    assert_eq!(egg_moves(1, 5), 5);
    assert_eq!(egg_moves(3, 14), 4);
}

// ================================================================================================
/// Q20. TRIANGLE (minimum path sum from the top to the bottom, moving to an adjacent number below) and PERFECT SQUARES
/// (fewest perfect squares summing to n).
///   triangle [[2],[3,4],[6,5,7],[4,1,8,3]] -> 11;   perfect squares: 12 -> 3 (4+4+4), 13 -> 2
///
/// APPROACH. Triangle: brute force over all 2^(rows-1) paths. Bottom-up DP in ONE array: start with the last row, and for
/// each row above `dp[i] = tri[r][i] + min(dp[i], dp[i+1])`. O(cells) time, O(rows) space, no explicit table, and
/// no edge cases at the borders. Perfect squares: `dp[i] = 1 + min(dp[i - s*s])` over squares s*s <= i, O(n sqrt n); or BFS over
/// remainders; or Lagrange's four-square theorem (the answer is at most 4) for an O(sqrt n) check, used as the oracle.
/// RUST. Build the 1-D array by cloning the last row; iterate rows with `(0..len - 1).rev()`. Squares: loop
/// `s` while `s * s <= i` (no floating-point `sqrt`).
/// INTERVIEWER IS TESTING: bottom-up avoids border special cases; knowing the four-square bound shows depth.
fn triangle_min(tri: &[Vec<i64>]) -> i64 {
    let mut dp = tri[tri.len() - 1].clone();
    for r in (0..tri.len() - 1).rev() {
        for i in 0..=r {
            dp[i] = tri[r][i] + dp[i].min(dp[i + 1]);
        }
    }
    dp[0]
}
fn triangle_brute(tri: &[Vec<i64>], r: usize, i: usize) -> i64 {
    if r == tri.len() - 1 {
        tri[r][i]
    } else {
        tri[r][i] + triangle_brute(tri, r + 1, i).min(triangle_brute(tri, r + 1, i + 1))
    }
}
fn num_squares(n: usize) -> usize {
    let mut dp = vec![usize::MAX; n + 1];
    dp[0] = 0;
    for i in 1..=n {
        let mut s = 1;
        while s * s <= i {
            dp[i] = dp[i].min(dp[i - s * s] + 1);
            s += 1;
        }
    }
    dp[n]
}
fn squares_can(n: usize, k: usize) -> bool {
    if k == 0 {
        return n == 0;
    }
    let mut s = 1;
    while s * s <= n {
        if squares_can(n - s * s, k - 1) {
            return true;
        }
        s += 1;
    }
    false
}
fn q20() {
    let mut rng = Rng::new(20);
    for _ in 0..CASES {
        let rows = 1 + rng.below(6);
        let tri: Vec<Vec<i64>> = (0..rows).map(|r| rng.vec(r + 1, -9, 9)).collect();
        assert_eq!(triangle_min(&tri), triangle_brute(&tri, 0, 0), "{tri:?}");
    }
    for n in 1..=80 {
        let want = (1..=4).find(|&k| squares_can(n, k)).unwrap();
        assert_eq!(num_squares(n), want, "n={n}");
    }
    assert_eq!(
        triangle_min(&[vec![2], vec![3, 4], vec![6, 5, 7], vec![4, 1, 8, 3]]),
        11
    );
    assert_eq!((num_squares(12), num_squares(13)), (3, 2));
}

// ================================================================================================
// BACKTRACKING SECTION
// ================================================================================================
/// Q21. SUBSETS I (distinct numbers, return all subsets) and SUBSETS II (input may contain duplicates, return each distinct
/// subset once).   [1,2,3] -> 8 subsets;   [1,2,2] -> [[],[1],[1,2],[1,2,2],[2],[2,2]]
///
/// APPROACH. Bitmask over 2^n masks is the oracle. Backtracking CHOOSE / EXPLORE / UNCHOOSE: at each index either skip or take
/// (I), or the "for j in start.." form that records the current path at every node (used for II). For II: SORT, and inside
/// the loop skip `nums[j] == nums[j-1]` when `j > start` (the same value may be taken once per depth). O(n * 2^n) time and
/// output size. Never dedupe with a `HashSet` of results unless asked: it costs memory and hides the real insight.
/// RUST. `cur: &mut Vec<_>` with push/pop; `out.push(cur.clone())` copies only at the leaves/nodes that are results.
/// INTERVIEWER IS TESTING: the unchoose step, and the `j > start` (not `j > 0`) guard.
fn subsets(nums: &[i64]) -> Vec<Vec<i64>> {
    fn go(i: usize, nums: &[i64], cur: &mut Vec<i64>, out: &mut Vec<Vec<i64>>) {
        if i == nums.len() {
            out.push(cur.clone());
            return;
        }
        go(i + 1, nums, cur, out); // skip
        cur.push(nums[i]); // choose
        go(i + 1, nums, cur, out); // explore
        cur.pop(); // unchoose
    }
    let mut out = vec![];
    go(0, nums, &mut vec![], &mut out);
    out
}
fn subsets_with_dup(nums: &[i64]) -> Vec<Vec<i64>> {
    fn go(start: usize, v: &[i64], cur: &mut Vec<i64>, out: &mut Vec<Vec<i64>>) {
        out.push(cur.clone());
        for j in start..v.len() {
            if j > start && v[j] == v[j - 1] {
                continue;
            }
            cur.push(v[j]);
            go(j + 1, v, cur, out);
            cur.pop();
        }
    }
    let mut v = nums.to_vec();
    v.sort_unstable();
    let mut out = vec![];
    go(0, &v, &mut vec![], &mut out);
    out
}
fn q21() {
    let mut rng = Rng::new(21);
    for _ in 0..CASES {
        let n = rng.below(8);
        let v = rng.vec(n, 0, 4);
        let masks: BTreeSet<Vec<i64>> = (0u32..1 << n)
            .map(|m| {
                let mut s: Vec<i64> = (0..n).filter(|&i| m >> i & 1 == 1).map(|i| v[i]).collect();
                s.sort_unstable();
                s
            })
            .collect();
        let got = subsets_with_dup(&v);
        let got_set: BTreeSet<Vec<i64>> = got.iter().cloned().collect();
        assert_eq!(
            got.len(),
            got_set.len(),
            "duplicate subset emitted for {v:?}"
        );
        assert_eq!(got_set, masks, "{v:?}");
        let mut distinct = v.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(subsets(&distinct).len(), 1 << distinct.len());
    }
    assert_eq!(
        subsets_with_dup(&[2, 1, 2]),
        vec![
            vec![],
            vec![1],
            vec![1, 2],
            vec![1, 2, 2],
            vec![2],
            vec![2, 2]
        ]
    );
}

// ================================================================================================
/// Q22. PERMUTATIONS I (distinct numbers) and II (duplicates allowed, return unique permutations).
///   [1,2,3] -> 6 permutations;   [1,1,2] -> [[1,1,2],[1,2,1],[2,1,1]]
///
/// APPROACH. I: swap-based backtracking: fix position k, swap each later element into it, recurse, swap back (unchoose).
/// O(n * n!) . II: SORT, keep a `used[]` array, build the permutation position by position, and skip `v[i]` if it equals
/// `v[i-1]` and `v[i-1]` is NOT currently used (equal values must be taken left-to-right). Count of results
/// is n! / prod(c!). Oracle: all permutations from the swap method collected into a `BTreeSet`.
/// RUST. Swapping two elements of one `Vec` is `v.swap(a, b)` (no double-borrow issue). `used: Vec<bool>` passed `&mut`.
/// INTERVIEWER IS TESTING: that the duplicate rule is `!used[i - 1]`, and that you can state the output count.
fn permutations(nums: &[i64]) -> Vec<Vec<i64>> {
    fn go(k: usize, v: &mut Vec<i64>, out: &mut Vec<Vec<i64>>) {
        if k == v.len() {
            out.push(v.clone());
            return;
        }
        for i in k..v.len() {
            v.swap(k, i);
            go(k + 1, v, out);
            v.swap(k, i);
        }
    }
    let mut out = vec![];
    go(0, &mut nums.to_vec(), &mut out);
    out
}
fn permutations_unique(nums: &[i64]) -> Vec<Vec<i64>> {
    fn go(v: &[i64], used: &mut Vec<bool>, cur: &mut Vec<i64>, out: &mut Vec<Vec<i64>>) {
        if cur.len() == v.len() {
            out.push(cur.clone());
            return;
        }
        for i in 0..v.len() {
            if used[i] || (i > 0 && v[i] == v[i - 1] && !used[i - 1]) {
                continue;
            }
            used[i] = true;
            cur.push(v[i]);
            go(v, used, cur, out);
            cur.pop();
            used[i] = false;
        }
    }
    let mut v = nums.to_vec();
    v.sort_unstable();
    let mut out = vec![];
    go(&v, &mut vec![false; v.len()], &mut vec![], &mut out);
    out
}
fn q22() {
    let mut rng = Rng::new(22);
    for _ in 0..CASES {
        let n = rng.below(7);
        let v = rng.vec(n, 0, 3);
        let all: BTreeSet<Vec<i64>> = permutations(&v).into_iter().collect();
        let got = permutations_unique(&v);
        assert_eq!(got.len(), all.len(), "emitted duplicates for {v:?}");
        assert_eq!(got.iter().cloned().collect::<BTreeSet<_>>(), all);
        let mut counts: HashMap<i64, usize> = HashMap::new();
        v.iter().for_each(|&x| *counts.entry(x).or_insert(0) += 1);
        let fact = |k: usize| (1..=k).product::<usize>();
        assert_eq!(
            got.len(),
            fact(n) / counts.values().map(|&c| fact(c)).product::<usize>()
        );
        let mut distinct = v.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(permutations(&distinct).len(), fact(distinct.len()));
    }
    assert_eq!(
        permutations_unique(&[1, 1, 2]),
        vec![vec![1, 1, 2], vec![1, 2, 1], vec![2, 1, 1]]
    );
}

// ================================================================================================
/// Q23. COMBINATION SUM I (candidates distinct, each usable unlimited times) and II (candidates may repeat, each used once).
/// Return all unique combinations summing to target.
///   I: [2,3,6,7], 7 -> [[2,2,3],[7]];   II: [10,1,2,7,6,1,5], 8 -> [[1,1,6],[1,2,5],[1,7],[2,6]]
///
/// APPROACH. Sort, then DFS with a `start` index so combinations are generated in non-decreasing order (no permutation
/// duplicates). I recurses with the SAME index (reuse); II recurses with `i + 1` and skips equal siblings (`i > start &&
/// c[i] == c[i-1]`). PRUNE: since sorted, `break` as soon as a candidate exceeds the remainder. Complexity is output
/// sensitive; worst case exponential in target / min candidate. The count for I equals Coin Change II (Q3), used as an oracle.
/// RUST. Take `remain: i64` (or check `c <= remain` before subtracting) so the unsigned remainder never underflows.
/// INTERVIEWER IS TESTING: pruning with sort+break, and the difference between reuse (same index) and no reuse (`i + 1`).
fn combination_sum(cands: &[usize], target: usize) -> Vec<Vec<usize>> {
    fn go(
        start: usize,
        remain: usize,
        c: &[usize],
        cur: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if remain == 0 {
            out.push(cur.clone());
            return;
        }
        for i in start..c.len() {
            if c[i] > remain {
                break;
            }
            cur.push(c[i]);
            go(i, remain - c[i], c, cur, out);
            cur.pop();
        }
    }
    let mut c = cands.to_vec();
    c.sort_unstable();
    let mut out = vec![];
    go(0, target, &c, &mut vec![], &mut out);
    out
}
fn combination_sum2(cands: &[usize], target: usize) -> Vec<Vec<usize>> {
    fn go(
        start: usize,
        remain: usize,
        c: &[usize],
        cur: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if remain == 0 {
            out.push(cur.clone());
            return;
        }
        for i in start..c.len() {
            if c[i] > remain {
                break;
            }
            if i > start && c[i] == c[i - 1] {
                continue;
            }
            cur.push(c[i]);
            go(i + 1, remain - c[i], c, cur, out);
            cur.pop();
        }
    }
    let mut c = cands.to_vec();
    c.sort_unstable();
    let mut out = vec![];
    go(0, target, &c, &mut vec![], &mut out);
    out
}
fn q23() {
    let mut rng = Rng::new(23);
    for _ in 0..CASES {
        let mut c: Vec<usize> = (0..1 + rng.below(4)).map(|_| 1 + rng.below(6)).collect();
        c.sort_unstable();
        c.dedup();
        let target = rng.below(14);
        let got = combination_sum(&c, target);
        assert!(got.iter().all(|x| x.iter().sum::<usize>() == target));
        assert_eq!(got.len() as u64, coin_ways(&c, target), "{c:?} {target}");
        let n = rng.below(9);
        let d: Vec<usize> = (0..n).map(|_| 1 + rng.below(5)).collect();
        let want: BTreeSet<Vec<usize>> = (0u32..1 << n)
            .map(|m| {
                let mut s: Vec<usize> = (0..n).filter(|&i| m >> i & 1 == 1).map(|i| d[i]).collect();
                s.sort_unstable();
                s
            })
            .filter(|s| s.iter().sum::<usize>() == target)
            .collect();
        let got2 = combination_sum2(&d, target);
        assert_eq!(got2.len(), want.len(), "duplicates for {d:?} {target}");
        assert_eq!(got2.into_iter().collect::<BTreeSet<_>>(), want);
    }
    assert_eq!(
        combination_sum(&[2, 3, 6, 7], 7),
        vec![vec![2, 2, 3], vec![7]]
    );
    assert_eq!(
        combination_sum2(&[10, 1, 2, 7, 6, 1, 5], 8),
        vec![vec![1, 1, 6], vec![1, 2, 5], vec![1, 7], vec![2, 6]]
    );
}

// ================================================================================================
/// Q24. GENERATE PARENTHESES (all well-formed strings of n pairs) and LETTER COMBINATIONS OF A PHONE NUMBER.
///   n = 3 -> ["((()))","(()())","(())()","()(())","()()()"];   "23" -> ["ad","ae","af","bd","be","bf","cd","ce","cf"]
///
/// APPROACH. Parentheses: brute force generates all 2^(2n) strings and filters. Backtracking with two counters: add '('
/// while `open < n`, add ')' while `close < open`; every path is valid, so no filtering. Output size is the Catalan number
/// C(n) ~ 4^n / n^1.5. Phone: depth-first over digits, appending each mapped letter (or an iterative queue / odometer
/// product), 3^a * 4^b results. Empty input -> empty output (not `[""]`).
/// RUST. Build into one reusable `String` / `Vec<u8>` with `push` and `pop`, cloning only at a leaf. A `const` array of
/// `&str` indexed by `digit - b'2'` is the map.
/// INTERVIEWER IS TESTING: the pruning rule (`close < open`), and the empty-input edge case.
fn generate_parentheses(n: usize) -> Vec<String> {
    fn go(open: usize, close: usize, n: usize, cur: &mut String, out: &mut Vec<String>) {
        if cur.len() == 2 * n {
            out.push(cur.clone());
            return;
        }
        if open < n {
            cur.push('(');
            go(open + 1, close, n, cur, out);
            cur.pop();
        }
        if close < open {
            cur.push(')');
            go(open, close + 1, n, cur, out);
            cur.pop();
        }
    }
    let mut out = vec![];
    go(0, 0, n, &mut String::new(), &mut out);
    out
}
const KEYS: [&str; 8] = ["abc", "def", "ghi", "jkl", "mno", "pqrs", "tuv", "wxyz"];
fn letter_combinations(digits: &str) -> Vec<String> {
    fn go(i: usize, d: &[u8], cur: &mut String, out: &mut Vec<String>) {
        if i == d.len() {
            out.push(cur.clone());
            return;
        }
        for ch in KEYS[(d[i] - b'2') as usize].chars() {
            cur.push(ch);
            go(i + 1, d, cur, out);
            cur.pop();
        }
    }
    if digits.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    go(0, digits.as_bytes(), &mut String::new(), &mut out);
    out
}
fn q24() {
    for n in 0..=6 {
        let want: Vec<String> = (0u32..1 << (2 * n))
            .rev()
            .map(|m| {
                (0..2 * n)
                    .rev()
                    .map(|i| if m >> i & 1 == 1 { '(' } else { ')' })
                    .collect::<String>()
            })
            .filter(|s| {
                let mut bal = 0i32;
                s.chars().all(|c| {
                    bal += if c == '(' { 1 } else { -1 };
                    bal >= 0
                }) && bal == 0
            })
            .collect();
        assert_eq!(generate_parentheses(n), want, "n={n}");
    }
    let mut rng = Rng::new(24);
    for _ in 0..CASES {
        let d = rng.string("23456789", 4);
        let got = letter_combinations(&d);
        let total: usize = d.bytes().map(|b| KEYS[(b - b'2') as usize].len()).product();
        assert_eq!(got.len(), if d.is_empty() { 0 } else { total });
        // odometer oracle: decode the k-th combination from a mixed-radix number
        for (k, s) in got.iter().enumerate() {
            let mut rest = k;
            let mut want: Vec<char> = vec![];
            for b in d.bytes().rev() {
                let letters: Vec<char> = KEYS[(b - b'2') as usize].chars().collect();
                want.push(letters[rest % letters.len()]);
                rest /= letters.len();
            }
            want.reverse();
            assert_eq!(*s, want.into_iter().collect::<String>());
        }
    }
    assert_eq!(
        generate_parentheses(3),
        vec!["((()))", "(()())", "(())()", "()(())", "()()()"]
    );
    assert_eq!(letter_combinations("23").len(), 9);
}

// ================================================================================================
/// Q25. PALINDROME PARTITIONING (all ways to cut `s` into palindromic pieces) and RESTORE IP ADDRESSES (all valid IPv4
/// addresses obtainable by inserting 3 dots into a digit string).
///   "aab" -> [["a","a","b"],["aa","b"]];   "25525511135" -> ["255.255.11.135","255.255.111.35"]
///
/// APPROACH. Palindromes: precompute `pal[i][j]` with an O(n^2) DP (`s[i]==s[j] && pal[i+1][j-1]`), then backtrack over the cut position;
/// brute force = all 2^(n-1) cut masks (the oracle). Worst case O(n 2^n). IPs: backtrack over 4 segments of length 1..=3;
/// a segment is valid iff it has no leading zero (unless exactly "0") and is <= 255; prune on remaining length
/// (`remaining` must be in `[segments_left, 3 * segments_left]`). At most 81 candidates, so O(1).
/// RUST. Slice `&s[start..end]` on ASCII digits. Validate by `parse::<u16>()` plus the leading-zero check; do not trust
/// parse alone because "01" parses as 1.
/// INTERVIEWER IS TESTING: leading zeros, the precomputed palindrome table, pruning by remaining length.
fn palindrome_partitions(s: &str) -> Vec<Vec<String>> {
    fn go(
        start: usize,
        s: &[u8],
        pal: &[Vec<bool>],
        cur: &mut Vec<String>,
        out: &mut Vec<Vec<String>>,
    ) {
        if start == s.len() {
            out.push(cur.clone());
            return;
        }
        for end in start..s.len() {
            if pal[start][end] {
                cur.push(String::from_utf8(s[start..=end].to_vec()).unwrap());
                go(end + 1, s, pal, cur, out);
                cur.pop();
            }
        }
    }
    let b = s.as_bytes();
    let n = b.len();
    let mut pal = vec![vec![false; n]; n];
    for i in (0..n).rev() {
        for j in i..n {
            pal[i][j] = b[i] == b[j] && (j - i < 2 || pal[i + 1][j - 1]);
        }
    }
    let mut out = vec![];
    go(0, b, &pal, &mut vec![], &mut out);
    out
}
fn valid_octet(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 3
        && (seg == "0" || !seg.starts_with('0'))
        && seg.parse::<u16>().is_ok_and(|v| v <= 255)
}
fn restore_ip(s: &str) -> Vec<String> {
    fn go(start: usize, parts: &mut Vec<String>, s: &str, out: &mut Vec<String>) {
        let left = 4 - parts.len();
        let remaining = s.len() - start;
        if remaining < left || remaining > 3 * left {
            return; // prune: cannot fill the remaining segments
        }
        if left == 1 {
            if valid_octet(&s[start..]) {
                parts.push(s[start..].to_string());
                out.push(parts.join("."));
                parts.pop();
            }
            return;
        }
        for len in 1..=3.min(remaining) {
            let seg = &s[start..start + len];
            if valid_octet(seg) {
                parts.push(seg.to_string());
                go(start + len, parts, s, out);
                parts.pop();
            }
        }
    }
    let mut out = vec![];
    if s.len() >= 4 && s.bytes().all(|b| b.is_ascii_digit()) {
        go(0, &mut vec![], s, &mut out);
    }
    out
}
fn q25() {
    let mut rng = Rng::new(25);
    for _ in 0..CASES {
        let s = rng.string("ab", 9);
        let b = s.as_bytes();
        let n = b.len();
        let mut want: BTreeSet<Vec<String>> = BTreeSet::new();
        for mask in 0u32..1 << n.saturating_sub(1) {
            let mut parts = vec![];
            let mut start = 0;
            for i in 0..n {
                if i + 1 == n || mask >> i & 1 == 1 {
                    parts.push(s[start..=i].to_string());
                    start = i + 1;
                }
            }
            if parts.iter().all(|p| p.bytes().eq(p.bytes().rev())) {
                want.insert(parts);
            }
        }
        if n == 0 {
            want.insert(vec![]);
        }
        let got = palindrome_partitions(&s);
        assert_eq!(got.iter().cloned().collect::<BTreeSet<_>>(), want, "{s:?}");
        assert_eq!(got.len(), want.len());
        // IP oracle: try all 3 cut points
        let digits = rng.string("0125", 8);
        let mut ips = BTreeSet::new();
        let len = digits.len();
        for a in 1..len {
            for b2 in a + 1..len {
                for c in b2 + 1..len {
                    let segs = [&digits[..a], &digits[a..b2], &digits[b2..c], &digits[c..]];
                    if segs.iter().all(|x| valid_octet(x)) {
                        ips.insert(segs.join("."));
                    }
                }
            }
        }
        assert_eq!(
            restore_ip(&digits).into_iter().collect::<BTreeSet<_>>(),
            ips,
            "{digits:?}"
        );
    }
    assert_eq!(
        palindrome_partitions("aab"),
        vec![vec!["a", "a", "b"], vec!["aa", "b"]]
    );
    assert_eq!(
        restore_ip("25525511135"),
        vec!["255.255.11.135", "255.255.111.35"]
    );
    assert_eq!(restore_ip("0000"), vec!["0.0.0.0"]);
    assert_eq!(restore_ip("101023").len(), 5);
    assert!(
        restore_ip("010010")
            .iter()
            .all(|ip| !ip.split('.').any(|p| p.len() > 1 && p.starts_with('0')))
    );
}

// ================================================================================================
/// Q26. N-QUEENS. Place n queens on an n x n board so none attack each other. Count the solutions; return the boards.
///   n = 4 -> 2 solutions;   n = 8 -> 92;   boards use 'Q' and '.'
///
/// APPROACH. Brute force: all n^n placements, or n! permutations (one queen per row and column) then a diagonal check. Backtracking row
/// by row with three attack sets: columns, "/" diagonals (`row + col`) and "\" diagonals (`row - col + n - 1`). BITMASK version: three
/// integers `cols`, `d1`, `d2` where the diagonals shift one bit per row; `avail = !(cols | d1 | d2) & full`, iterate set bits
/// with `bit = avail & -avail` (lowest set bit). Same O(n!) worst case but a tiny constant, fast enough for n = 14 in
/// milliseconds. Symmetry (mirror the first row) halves the work.
/// RUST. `avail & avail.wrapping_neg()` isolates the lowest set bit; `avail ^= bit` clears it. Mask with `full` after every left
/// shift so bits do not leak past column n - 1. Boards: remember the column per row and render at the leaf.
/// INTERVIEWER IS TESTING: the diagonal indexing formulas (or bit shifting), and unchoose symmetry.
fn count_queens(n: usize) -> u64 {
    fn go(full: u32, cols: u32, d1: u32, d2: u32) -> u64 {
        if cols == full {
            return 1;
        }
        let mut avail = full & !(cols | d1 | d2);
        let mut count = 0;
        while avail != 0 {
            let bit = avail & avail.wrapping_neg();
            avail ^= bit;
            count += go(full, cols | bit, ((d1 | bit) << 1) & full, (d2 | bit) >> 1);
        }
        count
    }
    go((1u32 << n) - 1, 0, 0, 0)
}
fn solve_queens(n: usize) -> Vec<Vec<String>> {
    fn go(
        row: usize,
        n: usize,
        cols: &mut Vec<usize>,
        used: &mut [Vec<bool>; 3],
        out: &mut Vec<Vec<String>>,
    ) {
        if row == n {
            out.push(
                cols.iter()
                    .map(|&c| (0..n).map(|j| if j == c { 'Q' } else { '.' }).collect())
                    .collect(),
            );
            return;
        }
        for c in 0..n {
            let (a, d) = (row + c, row + n - 1 - c);
            if used[0][c] || used[1][a] || used[2][d] {
                continue;
            }
            used[0][c] = true;
            used[1][a] = true;
            used[2][d] = true;
            cols.push(c);
            go(row + 1, n, cols, used, out);
            cols.pop();
            used[0][c] = false;
            used[1][a] = false;
            used[2][d] = false;
        }
    }
    let mut out = vec![];
    go(
        0,
        n,
        &mut vec![],
        &mut [vec![false; n], vec![false; 2 * n], vec![false; 2 * n]],
        &mut out,
    );
    out
}
fn queens_perm_oracle(n: usize) -> u64 {
    fn go(k: usize, p: &mut Vec<usize>, count: &mut u64) {
        if k == p.len() {
            *count += 1;
            return;
        }
        for i in k..p.len() {
            p.swap(k, i);
            if (0..k).all(|r| p[r].abs_diff(p[k]) != k - r) {
                go(k + 1, p, count);
            }
            p.swap(k, i);
        }
    }
    let mut count = 0;
    go(0, &mut (0..n).collect(), &mut count);
    count
}
fn q26() {
    let known = [1u64, 1, 0, 0, 2, 10, 4, 40, 92, 352, 724];
    for n in 0..=10 {
        assert_eq!(count_queens(n), known[n], "n={n}");
    }
    for n in 1..=8 {
        assert_eq!(count_queens(n), queens_perm_oracle(n), "n={n}");
        assert_eq!(solve_queens(n).len() as u64, count_queens(n));
    }
    for board in solve_queens(6) {
        assert_eq!(
            board.iter().map(|r| r.matches('Q').count()).sum::<usize>(),
            6
        );
    }
    assert_eq!(
        solve_queens(4),
        vec![
            vec![".Q..", "...Q", "Q...", "..Q."],
            vec!["..Q.", "Q...", "...Q", ".Q.."]
        ]
    );
}

// ================================================================================================
/// Q27. WORD SEARCH. Does `word` exist in the character grid as a path of horizontally/vertically adjacent cells, using each
/// cell at most once?   board [["A","B","C","E"],["S","F","C","S"],["A","D","E","E"]], "ABCCED" -> true, "ABCB" -> false
///
/// APPROACH. DFS from every cell matching the first letter; mark the current cell as visited by overwriting it with a
/// sentinel (`#`) IN PLACE, recurse into the 4 neighbours, then restore it (unchoose). O(mn * 3^L) worst case: after the
/// first step only 3 directions remain. Pruning: fail fast on a mismatch, count letters first, or search from the
/// rarer end of the word. A separate `visited` matrix works but costs O(mn) space.
/// RUST. Borrowing while mutating: the recursive function takes `&mut Vec<Vec<u8>>` and the cell is read into a local
/// BEFORE the write, so no reference is held across the recursive call. Neighbour coordinates use
/// `r.checked_add_signed(dr)` (returns `None` on underflow) then a `< rows` test; never cast to `i32` and back.
/// INTERVIEWER IS TESTING: restoring the cell on every exit path (including the early `true`), and bounds without underflow.
const DIRS: [(isize, isize); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
fn word_exists(board: &mut Vec<Vec<u8>>, word: &[u8]) -> bool {
    fn dfs(b: &mut Vec<Vec<u8>>, r: usize, c: usize, w: &[u8], k: usize) -> bool {
        if b[r][c] != w[k] {
            return false;
        }
        if k + 1 == w.len() {
            return true;
        }
        let saved = b[r][c];
        b[r][c] = b'#';
        let mut found = false;
        for (dr, dc) in DIRS {
            if let (Some(nr), Some(nc)) = (r.checked_add_signed(dr), c.checked_add_signed(dc)) {
                if nr < b.len() && nc < b[0].len() && dfs(b, nr, nc, w, k + 1) {
                    found = true;
                    break;
                }
            }
        }
        b[r][c] = saved; // restore on every path
        found
    }
    if word.is_empty() {
        return true;
    }
    (0..board.len()).any(|r| (0..board[0].len()).any(|c| dfs(board, r, c, word, 0)))
}
fn all_path_strings(g: &[Vec<u8>]) -> HashSet<Vec<u8>> {
    fn go(
        g: &[Vec<u8>],
        r: usize,
        c: usize,
        seen: &mut Vec<Vec<bool>>,
        cur: &mut Vec<u8>,
        out: &mut HashSet<Vec<u8>>,
    ) {
        seen[r][c] = true;
        cur.push(g[r][c]);
        out.insert(cur.clone());
        for (dr, dc) in DIRS {
            if let (Some(nr), Some(nc)) = (r.checked_add_signed(dr), c.checked_add_signed(dc)) {
                if nr < g.len() && nc < g[0].len() && !seen[nr][nc] {
                    go(g, nr, nc, seen, cur, out);
                }
            }
        }
        cur.pop();
        seen[r][c] = false;
    }
    let mut out = HashSet::new();
    for r in 0..g.len() {
        for c in 0..g[0].len() {
            go(
                g,
                r,
                c,
                &mut vec![vec![false; g[0].len()]; g.len()],
                &mut vec![],
                &mut out,
            );
        }
    }
    out
}
fn q27() {
    let mut rng = Rng::new(27);
    for _ in 0..CASES {
        let (m, n) = (1 + rng.below(3), 1 + rng.below(3));
        let grid: Vec<Vec<u8>> = (0..m)
            .map(|_| (0..n).map(|_| b"ab"[rng.below(2)]).collect())
            .collect();
        let paths = all_path_strings(&grid);
        let word = rng.string("ab", 6).into_bytes();
        let mut g2 = grid.clone();
        let want = word.is_empty() || paths.contains(&word);
        assert_eq!(word_exists(&mut g2, &word), want, "{grid:?} {word:?}");
        assert_eq!(g2, grid, "board must be restored after the search");
    }
    let mut board: Vec<Vec<u8>> = ["ABCE", "SFCS", "ADEE"]
        .iter()
        .map(|r| r.bytes().collect())
        .collect();
    assert!(word_exists(&mut board, b"ABCCED"));
    assert!(word_exists(&mut board, b"SEE"));
    assert!(!word_exists(&mut board, b"ABCB"));
}

// ================================================================================================
/// Q28. SUDOKU SOLVER. Fill the 9 x 9 board so every row, column and 3 x 3 box contains 1..9 once.
///   input is an 81-char string with '.' for blanks; the solver fills it in place.
///
/// APPROACH. Naive backtracking tries digits cell by cell (can take millions of steps on hard puzzles). Improvements: keep three
/// arrays of 9 BITMASKS (used digits per row, column, box) so "is d allowed?" is one AND; and MRV (minimum remaining values):
/// each step pick the empty cell with the fewest candidates, which detects dead ends (0 candidates) immediately and
/// often forces moves (1 candidate). Typical puzzles then solve in well under a millisecond. Complexity is exponential in
/// the worst case; the technique is constraint propagation + search.
/// RUST. Keep solver state in a struct with `&mut self` methods; `u16` masks with bit d (1..=9) set; candidate count is
/// `9 - used.count_ones()`; `box = r / 3 * 3 + c / 3`. Place and unplace by toggling the same bits (CHOOSE / UNCHOOSE).
/// INTERVIEWER IS TESTING: constant-time validity checks and a smarter cell order than row-major.
struct Sudoku {
    b: [[u8; 9]; 9],
    rows: [u16; 9],
    cols: [u16; 9],
    boxes: [u16; 9],
}
impl Sudoku {
    fn parse(s: &str) -> Option<Sudoku> {
        if s.len() != 81 {
            return None;
        }
        let mut sd = Sudoku {
            b: [[0; 9]; 9],
            rows: [0; 9],
            cols: [0; 9],
            boxes: [0; 9],
        };
        for (i, ch) in s.bytes().enumerate() {
            let (r, c) = (i / 9, i % 9);
            let d = if ch == b'.' {
                0
            } else {
                ch.checked_sub(b'0').filter(|d| (1..=9).contains(d))?
            };
            if d != 0 {
                if sd.used(r, c) >> d & 1 == 1 {
                    return None; // given digits already conflict
                }
                sd.set(r, c, d);
            }
        }
        Some(sd)
    }
    fn used(&self, r: usize, c: usize) -> u16 {
        self.rows[r] | self.cols[c] | self.boxes[r / 3 * 3 + c / 3]
    }
    fn set(&mut self, r: usize, c: usize, d: u8) {
        self.b[r][c] = d;
        self.rows[r] |= 1 << d;
        self.cols[c] |= 1 << d;
        self.boxes[r / 3 * 3 + c / 3] |= 1 << d;
    }
    fn clear(&mut self, r: usize, c: usize) {
        let d = self.b[r][c];
        self.b[r][c] = 0;
        self.rows[r] &= !(1 << d);
        self.cols[c] &= !(1 << d);
        self.boxes[r / 3 * 3 + c / 3] &= !(1 << d);
    }
    fn solve(&mut self) -> bool {
        let mut best: Option<(usize, usize)> = None;
        let mut best_count = 10;
        for r in 0..9 {
            for c in 0..9 {
                if self.b[r][c] == 0 {
                    let count = 9 - self.used(r, c).count_ones();
                    if count < best_count {
                        (best, best_count) = (Some((r, c)), count);
                    }
                }
            }
        }
        let Some((r, c)) = best else { return true };
        if best_count == 0 {
            return false;
        }
        let used = self.used(r, c);
        for d in 1..=9u8 {
            if used >> d & 1 == 0 {
                self.set(r, c, d); // choose
                if self.solve() {
                    return true; // explore
                }
                self.clear(r, c); // unchoose
            }
        }
        false
    }
    fn is_solved(&self) -> bool {
        let full = 0b11_1111_1110u16;
        (0..9).all(|i| self.rows[i] == full && self.cols[i] == full && self.boxes[i] == full)
            && (0..9).all(|r| (0..9).all(|c| self.b[r][c] != 0))
    }
}
fn q28() {
    let hard = "8..........36......7..9.2...5...7.......457.....1...3...1....68..85...1..9....4..";
    let mut sd = Sudoku::parse(hard).unwrap();
    assert!(sd.solve() && sd.is_solved());
    assert_eq!(sd.b[0], [8, 1, 2, 7, 5, 3, 6, 4, 9]); // the unique solution's first row
    assert!(Sudoku::parse(&format!("55{}", ".".repeat(79))).is_none()); // two 5s in row 0
    // row 0 holds 1..8 so cell (0,8) needs a 9, but a 9 already sits below it in column 8: no solution
    let bad = format!("12345678.{}9{}", ".".repeat(8), ".".repeat(63));
    assert!(!Sudoku::parse(&bad).unwrap().solve());
    // random puzzles: a valid solved grid, digits relabelled, 45 cells blanked; the solver must return SOME valid completion
    let mut rng = Rng::new(28);
    for _ in 0..300 {
        let mut perm: Vec<u8> = (1..=9).collect();
        rng.shuffle(&mut perm);
        let mut cells: Vec<u8> = (0..81)
            .map(|i| perm[((i / 9) * 3 + (i / 9) / 3 + i % 9) % 9])
            .collect();
        let solved = cells.clone();
        for _ in 0..45 {
            let i = rng.below(81);
            cells[i] = 0;
        }
        let text: String = cells
            .iter()
            .map(|&d| if d == 0 { '.' } else { (b'0' + d) as char })
            .collect();
        let mut sd = Sudoku::parse(&text).unwrap();
        assert!(sd.solve() && sd.is_solved(), "{text}");
        for (i, &d) in cells.iter().enumerate() {
            assert!(
                d == 0 || sd.b[i / 9][i % 9] == d,
                "givens must be preserved"
            );
        }
        assert!(solved.len() == 81);
    }
}

// ================================================================================================
/// Q29. PARTITION TO K EQUAL SUM SUBSETS. Can `nums` be split into k non-empty subsets with equal sums?
///   [4,3,2,3,5,2,1], k = 4 -> true;   [1,2,3,4], k = 3 -> false
///
/// APPROACH. Necessary: `sum % k == 0` and `max <= sum / k`. Backtracking assigns each number (sorted DESCENDING so big items
/// fail early) to one of k buckets whose running sum must not exceed the target. Key prunings: skip a bucket whose current sum equals an
/// earlier bucket's (identical states, symmetric), and stop after the first EMPTY bucket is tried. Worst case O(k^n)
/// but fast in practice. Optimal alternative: BITMASK DP over subsets, `dp[mask]` = sum of chosen items modulo target
/// (or -1 if unreachable), O(2^n * n), the oracle here.
/// RUST. Sort with `sort_unstable_by(|a, b| b.cmp(a))`. The "equal to an earlier bucket" test is
/// `buckets[..b].contains(&buckets[b])`. `usize` sums never go negative because we add before the recursive call and
/// subtract after (symmetric unchoose).
/// INTERVIEWER IS TESTING: the symmetry pruning, and recognising the bitmask-DP alternative for n <= 16.
fn can_partition_k(nums: &[usize], k: usize) -> bool {
    fn go(i: usize, nums: &[usize], buckets: &mut Vec<usize>, target: usize) -> bool {
        if i == nums.len() {
            return true;
        }
        for b in 0..buckets.len() {
            if buckets[..b].contains(&buckets[b]) || buckets[b] + nums[i] > target {
                continue;
            }
            buckets[b] += nums[i]; // choose
            if go(i + 1, nums, buckets, target) {
                return true; // explore
            }
            buckets[b] -= nums[i]; // unchoose
        }
        false
    }
    let total: usize = nums.iter().sum();
    if k == 0 || nums.len() < k || total % k != 0 {
        return false;
    }
    let target = total / k;
    let mut v = nums.to_vec();
    v.sort_unstable_by(|a, b| b.cmp(a));
    if v[0] > target {
        return false;
    }
    go(0, &v, &mut vec![0; k], target)
}
fn can_partition_k_mask(nums: &[usize], k: usize) -> bool {
    let total: usize = nums.iter().sum();
    if k == 0 || nums.len() < k || total % k != 0 {
        return false;
    }
    let target = total / k;
    let n = nums.len();
    let mut dp: Vec<Option<usize>> = vec![None; 1 << n]; // sum of the mask modulo target, None = unreachable
    dp[0] = Some(0);
    for mask in 0..1usize << n {
        let Some(cur) = dp[mask] else { continue };
        for i in 0..n {
            if mask >> i & 1 == 0 && cur + nums[i] <= target {
                dp[mask | 1 << i] = Some((cur + nums[i]) % target);
            }
        }
    }
    dp[(1 << n) - 1] == Some(0)
}
fn q29() {
    let mut rng = Rng::new(29);
    for _ in 0..CASES {
        let n = 1 + rng.below(10);
        let v: Vec<usize> = (0..n).map(|_| 1 + rng.below(6)).collect();
        let k = 1 + rng.below(4);
        assert_eq!(
            can_partition_k(&v, k),
            can_partition_k_mask(&v, k),
            "{v:?} k={k}"
        );
    }
    assert!(can_partition_k(&[4, 3, 2, 3, 5, 2, 1], 4));
    assert!(!can_partition_k(&[1, 2, 3, 4], 3));
    assert!(!can_partition_k(&[2, 2, 2, 2, 3, 4, 5], 4)); // the 4 would need a 1
    assert!(can_partition_k(&[1, 1, 1, 1], 4));
}

fn main() {
    let qs: [(&str, fn()); 29] = [
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
        ("Q28", q28),
        ("Q29", q29),
    ];
    for (name, q) in qs {
        q();
        println!("{name}: ok");
    }
}
