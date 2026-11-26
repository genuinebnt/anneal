//! RECIPE 640 (interview): interval, greedy and bit-manipulation interview questions in Rust (sort + sweep, exchange arguments, XOR/mask tricks), each checked against a brute-force oracle.
//! LEVEL: intermediate
//! Use in: coding interviews (LeetCode "intervals", "greedy", "bit manipulation" tags), calendar and scheduling code, low-level protocol and flag handling.   Run: cargo run --release --example intervals_greedy_bit_questions
//! COMMON USES: calendar and meeting-room booking, merging log or time windows, capacity planning (car pooling, rooms), schedulers and cooldown queues, parity/checksum and flag fields, subset enumeration, UTF-8 and varint decoders.
#![allow(dead_code)]
//! Read: https://www.techinterviewhandbook.org/grind75 (confirmed reachable, HTTP 200) , https://neetcode.io/practice (HTTP 200) , https://graphics.stanford.edu/~seander/bithacks.html ,
//! https://doc.rust-lang.org/std/primitive.u32.html , https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by_key
//! FREQUENCY TAGS ARE EDITORIAL JUDGMENT: the LeetCode problem-list pages answered HTTP 403 to curl, so per-company counts could not be confirmed. The question SELECTION follows the
//! interval/greedy/bit sections of the Grind 75 and NeetCode lists linked above (pages reachable; the exact membership of each problem was not machine-checked), and `[asked: ...]` is my own estimate.
//! See also: examples/algorithms/greedy_and_interval_algorithms.rs (the proofs and counterexamples behind these), examples/algorithms/bit_manipulation_real_world.rs and bit_tricks.rs (the bit toolbox),
//! examples/interview/stacks_queues_heaps_questions.rs (Q18/Q19 task scheduler and meeting rooms with heaps), examples/interview/tries_advanced_questions.rs (Maximum XOR lives there, deliberately not here).
//!
//! TRY NEXT:
//!   - [S] In `q5` replace `sort_by_key(|p| p.1)` by a comparator `|a, b| (a.1 - b.1).cmp(&0)`-style subtraction and run the extreme-coordinate test: which input panics in debug or silently misorders in release?
//!   - [S] Predict, then measure: for `q18` compare `count_ones`, Kernighan's loop and a 16-bit lookup table over 10 million random u32; which wins on sparse inputs (`x & rng & rng`) and why?
//!   - [M] Extend `q7`'s `Booker` into a K-booking calendar that returns WHICH existing booking blocks a rejected request; keep the naive-list oracle and add a test with 10,000 random operations.
//!   - [M] Solve `q16` (Single Number II) a third way with a HashMap of counts, then a fourth with the two-variable state machine generalised to "every other number appears 5 times"; derive the state count yourself.
//!   - [A] PROJECT: write a tiny shift-scheduler: parse shifts into intervals, use `q1`/`q6` to find coverage gaps, `q4` for the rooms needed, and `q8` for capacity checks, then fuzz it against the `greedy_and_interval_algorithms` oracles (copy `depth_brute`).
//!
//! QUESTION BANK
//! Q1. Merge intervals (LC 56): given a list of [start, end] intervals, merge all overlapping ones. [Medium] [asked: very common]
//! Q2. Insert interval (LC 57): insert a new interval into a sorted, non-overlapping list and merge if needed. [Medium] [asked: very common]
//! Q3. Non-overlapping intervals (LC 435): the minimum number of intervals to remove so the rest do not overlap. [Medium] [asked: common]
//! Q4. Meeting rooms I and II (LC 252, 253): can one person attend all meetings, and how many rooms are needed? [Medium] [asked: very common]
//! Q5. Minimum number of arrows to burst balloons (LC 452). [Medium] [asked: common]
//! Q6. Employee free time (LC 759): the common free intervals of several employees' schedules. [Hard] [asked: common]
//! Q7. My Calendar I, II, III (LC 729, 731, 732): booking with no double, no triple, and k-booking queries. [Medium] [asked: common]
//! Q8. Car pooling (LC 1094): can all trips be served without exceeding the capacity? [Medium] [asked: common]
//! Q9. Interval list intersections (LC 986): intersect two sorted lists of disjoint intervals. [Medium] [asked: common]
//! Q10. Jump game I and II (LC 55, 45): can you reach the end, and in how few jumps? [Medium] [asked: very common]
//! Q11. Gas station (LC 134): which starting station completes the circuit? [Medium] [asked: common]
//! Q12. Task scheduler (LC 621): the least time to run all tasks with a cooldown between equal tasks. [Medium] [asked: very common]
//! Q13. Partition labels (LC 763): split a string into as many parts as possible so each letter appears in only one part. [Medium] [asked: common]
//! Q14. Candy (LC 135): distribute minimum candies so higher-rated neighbours get more. [Hard] [asked: common]
//! Q15. Single number (LC 136): every element appears twice except one. [Easy] [asked: very common]
//! Q16. Single number II (LC 137): every element appears three times except one. [Medium] [asked: common]
//! Q17. Single number III (LC 260): two elements appear once, the rest twice. [Medium] [asked: common]
//! Q18. Number of 1 bits (LC 191): the Hamming weight of an integer. [Easy] [asked: very common]
//! Q19. Counting bits (LC 338): the popcount of every number 0..=n in O(n). [Easy] [asked: common]
//! Q20. Reverse bits (LC 190): reverse the bits of a 32-bit integer, and optimise for repeated calls. [Easy] [asked: common]
//! Q21. Power of two (LC 231), and power of four: is n a power of two, with no loops? [Easy] [asked: very common]
//! Q22. Missing number (LC 268): find the missing number in 0..=n. [Easy] [asked: very common]
//! Q23. Sum of two integers (LC 371): add without + or -. [Medium] [asked: common]
//! Q24. Subsets (LC 78) by bitmask: all subsets of a distinct-element array. [Medium] [asked: very common]
//! Q25. Hamming distance (LC 461): the number of differing bit positions. [Easy] [asked: common]
//! Q26. Total Hamming distance (LC 477): the sum over all pairs in O(32 n). [Medium] [asked: occasional]
//! Q27. Bitwise AND of numbers range (LC 201): AND of every integer in [m, n]. [Medium] [asked: common]
//! Q28. UTF-8 validation (LC 393): is a byte sequence valid under the problem's UTF-8 rules? [Medium] [asked: occasional]
//! Q29. Rust: sorting intervals correctly: sort_by_key with Reverse, comparators, `a - b` overflow, and f64 keys. [Medium] [asked: very common]
//! Q30. Rust: shift counts and zero inputs: `1u32 << 32`, `31 - leading_zeros`, `ilog2(0)`, checked_/wrapping_ shifts. [Medium] [asked: common]

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

// ---------------------------------------------------------------------------------------------------------------------
// Plumbing: seeded xorshift and helpers.
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
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
}
type Iv = (i32, i32);
fn rand_ivs(r: &mut Rng, n: usize, max_start: i32, max_len: i32, min_len: i32) -> Vec<Iv> {
    (0..n)
        .map(|_| {
            let s = r.int(0, max_start);
            (s, s + r.int(min_len, max_len))
        })
        .collect()
}
fn overlap_open((a, b): Iv, (c, d): Iv) -> bool {
    a < d && c < b
}
fn overlap_closed((a, b): Iv, (c, d): Iv) -> bool {
    a <= d && c <= b
}
/// Oracle shared by several questions: the most intervals alive at one integer point (half-open).
fn depth_brute(v: &[Iv]) -> usize {
    v.iter()
        .map(|p| v.iter().filter(|x| x.0 <= p.0 && p.0 < x.1).count())
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Merge intervals. Interviewer: "Given intervals [[1,3],[2,6],[8,10],[15,18]] return [[1,6],[8,10],[15,18]]. What if the input is unsorted? What about [1,4],[4,5]?"
// Approach: sort by start, sweep keeping the last output interval; if next.start <= last.end extend last.end = max(last.end, next.end) else push. O(n log n) time, O(n) output.
// Rust wrinkle: `sort_by_key(|x| x.0)` (no subtraction comparator, see Q29); `out.last_mut()` gives `&mut` to extend in place with no index juggling. The `max` matters: [1,10],[2,3].
// Interviewer looks for: asking closed vs half-open (touching merges here), the `max`, empty input, a single interval. Follow-ups: streaming merges (BTreeMap keyed by start), merge in place, count components.
// ---------------------------------------------------------------------------------------------------------------------
fn merge(mut v: Vec<Iv>) -> Vec<Iv> {
    v.sort_by_key(|x| x.0);
    let mut out: Vec<Iv> = Vec::with_capacity(v.len());
    for x in v {
        match out.last_mut() {
            Some(last) if x.0 <= last.1 => last.1 = last.1.max(x.1),
            _ => out.push(x),
        }
    }
    out
}
/// Oracle: repeatedly merge ANY overlapping pair until none remain.
fn merge_oracle(v: &[Iv]) -> Vec<Iv> {
    let mut v = v.to_vec();
    'again: loop {
        for i in 0..v.len() {
            for j in i + 1..v.len() {
                if overlap_closed(v[i], v[j]) {
                    let m = (v[i].0.min(v[j].0), v[i].1.max(v[j].1));
                    v.swap_remove(j);
                    v[i] = m;
                    continue 'again;
                }
            }
        }
        v.sort();
        return v;
    }
}
fn q1() {
    assert_eq!(
        merge(vec![(1, 3), (2, 6), (8, 10), (15, 18)]),
        [(1, 6), (8, 10), (15, 18)]
    );
    assert_eq!(merge(vec![(1, 4), (4, 5)]), [(1, 5)]);
    assert_eq!(merge(vec![(1, 10), (2, 3)]), [(1, 10)]);
    assert_eq!(merge(vec![]), []);
    let mut r = Rng(0x6001);
    for _ in 0..2000 {
        let n = r.below(12);
        let v = rand_ivs(&mut r, n, 25, 6, 0);
        assert_eq!(merge(v.clone()), merge_oracle(&v), "{v:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. Insert interval. Interviewer: "The list is sorted and non-overlapping; insert one more in O(n) without re-sorting."
// Approach: three phases with one index: copy intervals that end before the new one starts; absorb every interval that starts at or before new.end (min of starts, max of ends); copy the rest.
// O(n) time. Rust wrinkle: `extend_from_slice(&list[i..])` copies the tail in one call; avoid `Vec::insert` in a loop (O(n) each). Mistakes: using `<` where touching intervals must merge; forgetting the tail.
// Follow-ups: do it with binary search to find the window (still O(n) to write out, but O(log n) to locate: use a BTreeMap for O(log n) overall), delete an interval range.
// ---------------------------------------------------------------------------------------------------------------------
fn insert_interval(list: &[Iv], new: Iv) -> Vec<Iv> {
    let mut out = Vec::with_capacity(list.len() + 1);
    let (mut cur, mut i) = (new, 0);
    while i < list.len() && list[i].1 < cur.0 {
        out.push(list[i]);
        i += 1;
    }
    while i < list.len() && list[i].0 <= cur.1 {
        cur = (cur.0.min(list[i].0), cur.1.max(list[i].1));
        i += 1;
    }
    out.push(cur);
    out.extend_from_slice(&list[i..]);
    out
}
fn q2() {
    assert_eq!(insert_interval(&[(1, 3), (6, 9)], (2, 5)), [(1, 5), (6, 9)]);
    assert_eq!(insert_interval(&[], (5, 7)), [(5, 7)]);
    assert_eq!(insert_interval(&[(1, 5)], (2, 3)), [(1, 5)]);
    let mut r = Rng(0x6002);
    for _ in 0..2000 {
        let n = r.below(10);
        let base = merge(rand_ivs(&mut r, n, 30, 5, 0));
        let new = rand_ivs(&mut r, 1, 30, 6, 0)[0];
        let mut all = base.clone();
        all.push(new);
        assert_eq!(insert_interval(&base, new), merge_oracle(&all));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. Non-overlapping intervals. Interviewer: "Minimum removals so no two intervals overlap?" Approach: this is activity selection in disguise: keep the maximum number of compatible
// intervals = sort by END, take an interval if start >= last kept end; answer = n - kept. EXCHANGE ARGUMENT: the interval ending first leaves the most room. O(n log n).
// Rust wrinkle: sort by end with `sort_by_key(|x| x.1)`; initialise `last_end` with `i32::MIN` (not 0, coordinates may be negative). Touching intervals ([1,2],[2,3]) do NOT overlap here.
// Wrong greedies interviewers probe: sort by start (a long interval blocks many), or shortest first. Follow-ups: weighted version needs DP; return which intervals to remove.
// ---------------------------------------------------------------------------------------------------------------------
fn erase_overlap(mut v: Vec<Iv>) -> usize {
    v.sort_by_key(|x| x.1);
    let (mut last_end, mut kept) = (i32::MIN, 0);
    for x in &v {
        if x.0 >= last_end {
            kept += 1;
            last_end = x.1;
        }
    }
    v.len() - kept
}
fn erase_overlap_brute(v: &[Iv]) -> usize {
    let n = v.len();
    let best = (0u32..1 << n)
        .filter(|&m| {
            let sel: Vec<Iv> = (0..n).filter(|&i| m >> i & 1 == 1).map(|i| v[i]).collect();
            sel.iter()
                .enumerate()
                .all(|(i, &a)| sel[i + 1..].iter().all(|&b| !overlap_open(a, b)))
        })
        .map(|m| m.count_ones() as usize)
        .max()
        .unwrap();
    n - best
}
fn q3() {
    assert_eq!(erase_overlap(vec![(1, 2), (2, 3), (3, 4), (1, 3)]), 1);
    assert_eq!(erase_overlap(vec![(1, 2), (1, 2), (1, 2)]), 2);
    assert_eq!(erase_overlap(vec![(1, 2), (2, 3)]), 0);
    let mut r = Rng(0x6003);
    for _ in 0..1500 {
        let n = r.below(10);
        let v = rand_ivs(&mut r, n, 14, 6, 1);
        assert_eq!(erase_overlap(v.clone()), erase_overlap_brute(&v), "{v:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. Meeting rooms. I (252): can one person attend all? Sort by start, any next.start < prev.end means no. II (253): minimum rooms = maximum overlap. Approach: sort STARTS and ENDS
// separately and walk two pointers: a start before the earliest unreleased end needs a new room, otherwise reuse (end <= start frees it). O(n log n). No heap needed.
// Rust wrinkle: two `Vec<i32>` sorted with `sort_unstable()`; the comparison `starts[i] < ends[j]` (strict) encodes half-open meetings. Mistake: `<=` double-counts back-to-back meetings.
// Follow-ups: return the room assignment (heap of (end, room), see greedy_and_interval_algorithms.rs), the busiest room, online arrivals (BTreeMap of +1/-1 deltas).
// ---------------------------------------------------------------------------------------------------------------------
fn can_attend_all(mut v: Vec<Iv>) -> bool {
    v.sort_by_key(|x| x.0);
    v.windows(2).all(|w| w[0].1 <= w[1].0)
}
fn min_meeting_rooms(v: &[Iv]) -> usize {
    let mut starts: Vec<i32> = v.iter().map(|x| x.0).collect();
    let mut ends: Vec<i32> = v.iter().map(|x| x.1).collect();
    starts.sort_unstable();
    ends.sort_unstable();
    let (mut rooms, mut best, mut j) = (0usize, 0usize, 0);
    for &s in &starts {
        while ends[j] <= s {
            j += 1; // a meeting that already ended frees its room (j never passes the number of starts handled)
            rooms -= 1;
        }
        rooms += 1;
        best = best.max(rooms);
    }
    best
}
fn q4() {
    assert!(
        can_attend_all(vec![(7, 10), (2, 4)]) && !can_attend_all(vec![(0, 30), (5, 10), (15, 20)])
    );
    assert!(can_attend_all(vec![(1, 2), (2, 3)]), "back to back is fine");
    assert_eq!(min_meeting_rooms(&[(0, 30), (5, 10), (15, 20)]), 2);
    assert_eq!(min_meeting_rooms(&[(7, 10), (2, 4)]), 1);
    assert_eq!(min_meeting_rooms(&[(1, 2), (2, 3)]), 1);
    let mut r = Rng(0x6004);
    for _ in 0..2000 {
        let n = 1 + r.below(15);
        let v = rand_ivs(&mut r, n, 20, 7, 1);
        assert_eq!(min_meeting_rooms(&v), depth_brute(&v), "{v:?}");
        let pairwise_ok = v
            .iter()
            .enumerate()
            .all(|(i, &a)| v[i + 1..].iter().all(|&b| !overlap_open(a, b)));
        assert_eq!(can_attend_all(v.clone()), pairwise_ok);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. Minimum arrows. Interviewer: "Balloons are horizontal [xstart, xend]; an arrow at x bursts every balloon with xstart <= x <= xend. Fewest arrows?" Approach: sort by END; shoot at the
// first balloon's end; skip every balloon that starts at or before that x; the next uncovered balloon needs a new arrow. EXCHANGE: shifting an arrow right to the earliest end loses nothing.
// Rust wrinkle: the famous trap: coordinates reach i32::MIN and i32::MAX, so a comparator `|a, b| a.1 - b.1` OVERFLOWS (panics in debug, wrong in release). `sort_by_key(|p| p.1)` is immune.
// Note closed intervals: touching balloons share an arrow, unlike meeting rooms. Follow-ups: weighted balloons, 2-D, return the arrow positions.
// ---------------------------------------------------------------------------------------------------------------------
fn find_min_arrow_shots(mut pts: Vec<Iv>) -> usize {
    pts.sort_by_key(|p| p.1);
    let (mut arrows, mut last) = (0, None::<i32>);
    for (s, e) in pts {
        if last.is_none_or(|x| x < s) {
            arrows += 1;
            last = Some(e);
        }
    }
    arrows
}
fn arrows_brute(pts: &[Iv]) -> usize {
    (0u32..1 << 11)
        .filter(|&m| pts.iter().all(|&(s, e)| (s..=e).any(|x| m >> x & 1 == 1)))
        .map(|m| m.count_ones() as usize)
        .min()
        .unwrap()
}
fn q5() {
    assert_eq!(
        find_min_arrow_shots(vec![(10, 16), (2, 8), (1, 6), (7, 12)]),
        2
    );
    assert_eq!(
        find_min_arrow_shots(vec![(1, 2), (3, 4), (5, 6), (7, 8)]),
        4
    );
    assert_eq!(
        find_min_arrow_shots(vec![(1, 2), (2, 3), (3, 4), (4, 5)]),
        2
    );
    assert_eq!(
        find_min_arrow_shots(vec![(i32::MIN, i32::MAX), (0, 6)]),
        1,
        "extreme coordinates: no subtraction in the comparator"
    );
    assert_eq!(find_min_arrow_shots(vec![]), 0);
    let mut r = Rng(0x6005);
    for _ in 0..1500 {
        let n = 1 + r.below(8);
        let v: Vec<Iv> = (0..n)
            .map(|_| {
                let s = r.int(0, 10);
                (s, (s + r.int(0, 4)).min(10))
            })
            .collect();
        assert_eq!(find_min_arrow_shots(v.clone()), arrows_brute(&v), "{v:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Employee free time. Interviewer: "Each employee has a sorted list of busy intervals. Return the finite intervals when EVERYONE is free." Approach: flatten all busy intervals, sort by
// start, merge; the gaps between consecutive merged intervals are the answer (half-open: a gap exists only if next.start > cur.end). O(N log N), N = total intervals.
// Better for k sorted lists: k-way merge with a heap, O(N log k). Rust wrinkle: `flatten().copied().collect()` over `Vec<Vec<Iv>>`; `windows(2)` over the merged list yields the gaps directly.
// Mistakes: reporting the infinite regions before the first or after the last meeting; treating a zero-length gap as free. Follow-ups: heap version, the longest common free slot of length >= d.
// ---------------------------------------------------------------------------------------------------------------------
fn employee_free_time(schedules: &[Vec<Iv>]) -> Vec<Iv> {
    let all: Vec<Iv> = schedules.iter().flatten().copied().collect();
    let mut merged = {
        let mut v = all;
        v.sort_by_key(|x| x.0);
        let mut out: Vec<Iv> = Vec::new();
        for x in v {
            match out.last_mut() {
                Some(l) if x.0 <= l.1 => l.1 = l.1.max(x.1), // touching busy blocks leave no free time, so merge them too
                _ => out.push(x),
            }
        }
        out
    };
    merged.shrink_to_fit();
    merged.windows(2).map(|w| (w[0].1, w[1].0)).collect()
}
fn free_time_oracle(schedules: &[Vec<Iv>]) -> Vec<Iv> {
    // Unit cells 0..60 that are busy for someone; free runs strictly between the first and last busy cell.
    let busy: Vec<bool> = (0..60)
        .map(|t| schedules.iter().flatten().any(|&(s, e)| s <= t && t < e))
        .collect();
    let (first, last) = match (busy.iter().position(|&b| b), busy.iter().rposition(|&b| b)) {
        (Some(a), Some(b)) => (a, b),
        _ => return vec![],
    };
    let mut out = Vec::new();
    let mut t = first;
    while t <= last {
        if busy[t] {
            t += 1;
            continue;
        }
        let start = t;
        while !busy[t] {
            t += 1;
        }
        out.push((start as i32, t as i32));
    }
    out
}
fn q6() {
    let s = vec![vec![(1, 2), (5, 6)], vec![(1, 3)], vec![(4, 10)]];
    assert_eq!(employee_free_time(&s), [(3, 4)]);
    let mut r = Rng(0x6006);
    for _ in 0..1500 {
        let people = 1 + r.below(4);
        let sch: Vec<Vec<Iv>> = (0..people)
            .map(|_| {
                let n = r.below(5);
                merge(rand_ivs(&mut r, n, 40, 6, 1))
            })
            .collect();
        assert_eq!(employee_free_time(&sch), free_time_oracle(&sch), "{sch:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. My Calendar I / II / III. Interviewer: "book(start, end) returns true if it causes no double booking (I) / no triple booking (II); III returns the max k-booking after each call."
// Approach: a BTreeMap of +1/-1 deltas per time point; booking adds two deltas, walking the map in key order yields the running overlap. I = limit 1, II = limit 2, III = no limit (return the
// max). One struct covers all three; reject by rolling the two deltas back. O(n) per booking with a plain map walk; a segment tree or balanced tree with lazy counts gives O(log n).
// Rust wrinkle: `entry(k).or_default()` for counters; remove keys that fall to 0 on rollback (otherwise the map grows with phantom points). Half-open [s, e): touching bookings are fine.
// Follow-ups: cancel a booking, "find the first free slot of length L", sharded calendars per room.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Default)]
struct Booker {
    delta: BTreeMap<i32, i32>,
}
impl Booker {
    fn bump(&mut self, k: i32, d: i32) {
        let e = self.delta.entry(k).or_default();
        *e += d;
        if *e == 0 {
            self.delta.remove(&k);
        }
    }
    fn max_overlap(&self) -> usize {
        let (mut cur, mut best) = (0, 0);
        for d in self.delta.values() {
            cur += d;
            best = best.max(cur);
        }
        best as usize
    }
    /// Book unless the overlap would exceed `limit`; returns whether it was booked.
    fn try_book(&mut self, s: i32, e: i32, limit: usize) -> bool {
        self.bump(s, 1);
        self.bump(e, -1);
        if self.max_overlap() > limit {
            self.bump(s, -1);
            self.bump(e, 1);
            return false;
        }
        true
    }
}
fn q7() {
    let mut b = Booker::default();
    assert!(b.try_book(10, 20, 1) && !b.try_book(15, 25, 1) && b.try_book(20, 30, 1));
    let mut b2 = Booker::default();
    assert!(
        b2.try_book(10, 20, 2)
            && b2.try_book(50, 60, 2)
            && b2.try_book(10, 40, 2)
            && !b2.try_book(5, 15, 2)
            && b2.try_book(5, 10, 2)
            && b2.try_book(25, 55, 2)
    );
    let mut r = Rng(0x6007);
    for limit in [1usize, 2, 3] {
        for _ in 0..300 {
            let (mut b, mut booked): (Booker, Vec<Iv>) = (Booker::default(), vec![]);
            for _ in 0..30 {
                let s = r.int(0, 30);
                let e = s + r.int(1, 8);
                let mut trial = booked.clone();
                trial.push((s, e));
                let want = depth_brute(&trial) <= limit;
                assert_eq!(b.try_book(s, e, limit), want);
                if want {
                    booked.push((s, e));
                }
                assert_eq!(b.max_overlap(), depth_brute(&booked));
            }
            assert!(
                b.delta.values().all(|&d| d != 0),
                "no phantom zero entries after rollbacks"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. Car pooling. Interviewer: "trips[i] = [passengers, from, to]; a car with `capacity` seats drives one way. Can all trips be served?" Approach: difference array / sorted map: +p at `from`,
// -p at `to`; sweep in order; at the same coordinate dropoffs (negative deltas) happen BEFORE pickups, which the signed-key sort gives for free if events are sorted as (position, delta).
// O(n log n), or O(max coordinate) with a counting array. Rust wrinkle: `Vec<(i32, i32)>` of (pos, delta) sorts lexicographically, -p < +q so drop-offs come first; use i64 for the running sum if p can be large.
// Follow-ups: same idea answers "minimum capacity needed", and the room problem Q4 is the p = 1 case.
// ---------------------------------------------------------------------------------------------------------------------
fn car_pooling(trips: &[(i32, i32, i32)], capacity: i32) -> bool {
    let mut ev: Vec<(i32, i32)> = trips
        .iter()
        .flat_map(|&(p, from, to)| [(from, p), (to, -p)])
        .collect();
    ev.sort();
    let mut load = 0i64;
    ev.into_iter().all(|(_, d)| {
        load += d as i64;
        load <= capacity as i64
    })
}
fn car_pooling_brute(trips: &[(i32, i32, i32)], capacity: i32) -> bool {
    (0..=60).all(|x| {
        trips
            .iter()
            .filter(|&&(_, f, t)| f <= x && x < t)
            .map(|&(p, _, _)| p)
            .sum::<i32>()
            <= capacity
    })
}
fn q8() {
    assert!(!car_pooling(&[(2, 1, 5), (3, 3, 7)], 4) && car_pooling(&[(2, 1, 5), (3, 3, 7)], 5));
    assert!(car_pooling(&[(3, 2, 7), (3, 7, 9), (8, 3, 9)], 11));
    let mut r = Rng(0x6008);
    for _ in 0..2000 {
        let trips: Vec<(i32, i32, i32)> = (0..1 + r.below(6))
            .map(|_| {
                let f = r.int(0, 40);
                (r.int(1, 5), f, f + r.int(1, 15))
            })
            .collect();
        let cap = r.int(1, 12);
        assert_eq!(
            car_pooling(&trips, cap),
            car_pooling_brute(&trips, cap),
            "{trips:?} cap {cap}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. Interval list intersections. Interviewer: "Two lists of closed, sorted, pairwise-disjoint intervals; return their intersection." Approach: two pointers; the overlap of A[i] and B[j] is
// [max(starts), min(ends)] when non-empty (<=: a shared endpoint counts); then advance the pointer whose interval ends first (it cannot meet anything else). O(m + n).
// Rust wrinkle: `if a[i].1 < b[j].1 { i += 1 } else { j += 1 }` as one expression statement; no `unwrap`/index panics because the loop condition guards both indices.
// Follow-ups: union of two lists, difference, k lists (heap), intervals on a BTreeMap with range queries.
// ---------------------------------------------------------------------------------------------------------------------
fn interval_intersection(a: &[Iv], b: &[Iv]) -> Vec<Iv> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        let (lo, hi) = (a[i].0.max(b[j].0), a[i].1.min(b[j].1));
        if lo <= hi {
            out.push((lo, hi));
        }
        if a[i].1 < b[j].1 { i += 1 } else { j += 1 }
    }
    out
}
fn q9() {
    assert_eq!(
        interval_intersection(
            &[(0, 2), (5, 10), (13, 23), (24, 25)],
            &[(1, 5), (8, 12), (15, 24), (25, 26)]
        ),
        [(1, 2), (5, 5), (8, 10), (15, 23), (24, 24), (25, 25)]
    );
    assert_eq!(interval_intersection(&[], &[(1, 2)]), []);
    let mut r = Rng(0x6009);
    for _ in 0..2000 {
        let (na, nb) = (r.below(7), r.below(7));
        // Closed and strictly separated: merge treats touching as overlapping, so the result has gaps.
        let a = merge(rand_ivs(&mut r, na, 30, 5, 0));
        let b = merge(rand_ivs(&mut r, nb, 30, 5, 0));
        let mut brute: Vec<Iv> = a
            .iter()
            .flat_map(|&x| {
                b.iter().filter_map(move |&y| {
                    let (lo, hi) = (x.0.max(y.0), x.1.min(y.1));
                    (lo <= hi).then_some((lo, hi))
                })
            })
            .collect();
        brute.sort();
        assert_eq!(interval_intersection(&a, &b), brute);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Jump game I / II. Interviewer: "nums[i] is the max jump length from i. (I) Can you reach the last index? (II) Minimum jumps?" Approach I: track the farthest reachable index; if i > far you are
// stuck. O(n), O(1). Approach II: implicit BFS: `end` = reach of the current jump count, `far` = reach with one more jump; crossing `end` costs a jump. O(n). A DP is O(n^2) (the oracle here).
// Rust wrinkle: indices are `usize`, so `far = far.max(i + x)` is fine but `i - 1` on i = 0 panics in debug; keep everything additive. Edge: a single-element array needs 0 jumps (loop to len-1).
// Follow-ups: jump game III (BFS), IV (BFS on equal values), V (DP with memo), return the path.
// ---------------------------------------------------------------------------------------------------------------------
fn can_jump(a: &[usize]) -> bool {
    let mut far = 0;
    for (i, &x) in a.iter().enumerate() {
        if i > far {
            return false;
        }
        far = far.max(i + x);
    }
    true
}
fn jump(a: &[usize]) -> usize {
    let (mut jumps, mut end, mut far) = (0, 0, 0);
    for i in 0..a.len().saturating_sub(1) {
        far = far.max(i + a[i]);
        if i == end {
            jumps += 1;
            end = far;
        }
    }
    jumps
}
fn jump_dp(a: &[usize]) -> Option<usize> {
    let n = a.len();
    let mut dp = vec![usize::MAX; n];
    dp[0] = 0;
    for i in 0..n {
        if dp[i] < usize::MAX {
            for j in i + 1..=(i + a[i]).min(n - 1) {
                dp[j] = dp[j].min(dp[i] + 1);
            }
        }
    }
    (dp[n - 1] != usize::MAX).then_some(dp[n - 1])
}
fn q10() {
    assert!(can_jump(&[2, 3, 1, 1, 4]) && !can_jump(&[3, 2, 1, 0, 4]) && can_jump(&[0]));
    assert_eq!(jump(&[2, 3, 1, 1, 4]), 2);
    assert_eq!(jump(&[1]), 0);
    let mut r = Rng(0x600A);
    for _ in 0..3000 {
        let n = 1 + r.below(9);
        let a: Vec<usize> = (0..n).map(|_| r.below(4)).collect();
        assert_eq!(can_jump(&a), jump_dp(&a).is_some(), "{a:?}");
        if let Some(d) = jump_dp(&a) {
            assert_eq!(jump(&a), d, "{a:?}"); // `jump` assumes reachability, as the problem guarantees
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Gas station. Interviewer: "gas[i] is gained at station i, cost[i] to reach i+1 on a circle. Return the starting index that completes the lap, or -1." Approach: if total gas < total cost
// there is no answer. Otherwise scan once keeping the tank; when it drops below 0 at i, no start in (start..=i) works, so restart at i+1. PROOF: every skipped start had a non-negative tank on
// reaching i (that is how it got past earlier stations), so starting later only has LESS fuel; hence the failure at i kills them all. O(n), O(1).
// Rust wrinkle: sum in i64 (i32 differences can overflow when summed); return `Option<usize>` instead of -1. Follow-ups: all valid starts (prefix minima), multiple laps.
// ---------------------------------------------------------------------------------------------------------------------
fn can_complete_circuit(gas: &[i32], cost: &[i32]) -> Option<usize> {
    let (mut total, mut tank, mut start) = (0i64, 0i64, 0);
    for i in 0..gas.len() {
        let d = gas[i] as i64 - cost[i] as i64;
        total += d;
        tank += d;
        if tank < 0 {
            start = i + 1;
            tank = 0;
        }
    }
    (total >= 0 && start < gas.len()).then_some(start)
}
fn gas_valid_starts(gas: &[i32], cost: &[i32]) -> Vec<usize> {
    let n = gas.len();
    (0..n)
        .filter(|&s| {
            let mut t = 0i64;
            (0..n).all(|k| {
                let i = (s + k) % n;
                t += gas[i] as i64 - cost[i] as i64;
                t >= 0
            })
        })
        .collect()
}
fn q11() {
    assert_eq!(
        can_complete_circuit(&[1, 2, 3, 4, 5], &[3, 4, 5, 1, 2]),
        Some(3)
    );
    assert_eq!(can_complete_circuit(&[2, 3, 4], &[3, 4, 3]), None);
    let mut r = Rng(0x600B);
    for _ in 0..3000 {
        let n = 1 + r.below(7);
        let gas: Vec<i32> = (0..n).map(|_| r.int(0, 6)).collect();
        let cost: Vec<i32> = (0..n).map(|_| r.int(0, 6)).collect();
        let valid = gas_valid_starts(&gas, &cost);
        match can_complete_circuit(&gas, &cost) {
            Some(s) => assert!(valid.contains(&s)),
            None => assert!(valid.is_empty()),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. Task scheduler. Interviewer: "Tasks A-Z, one per time unit; two equal tasks must be at least n units apart (idle allowed). Minimum total time?" Approach (closed form): let f = the max
// frequency and k = how many tasks have frequency f. The frame (f-1)*(n+1) + k is forced; other tasks fill the idle slots; if there are more tasks than the frame has slots, no idling is
// needed, so the answer is max(len, frame). Alternative: simulate with a max-heap and a cooldown queue (see stacks_queues_heaps_questions.rs Q18). O(len) time, O(26) space.
// Rust wrinkle: count with `[u32; 26]` indexed by `b - b'A'`; `saturating_sub(1)` guards an empty input. Follow-up: output the actual schedule; rearrange a string so no two equal are adjacent.
// ---------------------------------------------------------------------------------------------------------------------
fn least_interval(tasks: &str, n: u32) -> u32 {
    let mut cnt = [0u32; 26];
    for b in tasks.bytes() {
        cnt[(b - b'A') as usize] += 1;
    }
    let f = *cnt.iter().max().unwrap();
    let k = cnt.iter().filter(|&&c| c == f).count() as u32;
    (tasks.len() as u32).max(f.saturating_sub(1) * (n + 1) + k)
}
fn least_interval_bfs(tasks: &str, n: u32) -> u32 {
    let mut counts = [0u32; 26];
    tasks.bytes().for_each(|b| counts[(b - b'A') as usize] += 1);
    let kinds: Vec<u32> = counts.iter().copied().filter(|&c| c > 0).collect();
    let k = kinds.len();
    let mut layer: HashSet<(Vec<u32>, Vec<u32>)> = HashSet::from([(kinds, vec![0; k])]);
    let mut steps = 0;
    loop {
        if layer.iter().any(|(rem, _)| rem.iter().all(|&c| c == 0)) {
            return steps;
        }
        let mut next = HashSet::new();
        for (rem, cd) in &layer {
            let tick: Vec<u32> = cd.iter().map(|&c| c.saturating_sub(1)).collect();
            next.insert((rem.clone(), tick.clone()));
            for i in 0..k {
                if rem[i] > 0 && cd[i] == 0 {
                    let (mut r2, mut c2) = (rem.clone(), tick.clone());
                    r2[i] -= 1;
                    c2[i] = n;
                    next.insert((r2, c2));
                }
            }
        }
        layer = next;
        steps += 1;
    }
}
fn q12() {
    assert_eq!(least_interval("AAABBB", 2), 8);
    assert_eq!(least_interval("AAABBB", 0), 6);
    assert_eq!(least_interval("AAAAAABCDEFG", 2), 16);
    let mut r = Rng(0x600C);
    for _ in 0..400 {
        let len = 1 + r.below(7);
        let s: String = (0..len)
            .map(|_| (b'A' + r.below(3) as u8) as char)
            .collect();
        let n = r.below(4) as u32;
        assert_eq!(
            least_interval(&s, n),
            least_interval_bfs(&s, n),
            "{s} n={n}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. Partition labels. Interviewer: "Split s into as many parts as possible so that each letter appears in at most one part; return the sizes." Approach: record each letter's LAST index;
// sweep, extending the current part's end to max(last[c]); when i reaches the end, close the part. Greedy: closing as early as forced yields the most parts. O(n), O(26).
// Rust wrinkle: `[usize; 26]` of last positions via `enumerate` (later writes win); bytes not chars since the alphabet is lowercase ASCII. Follow-ups: Unicode (HashMap), minimum parts, merge intervals view.
// ---------------------------------------------------------------------------------------------------------------------
fn partition_labels(s: &str) -> Vec<usize> {
    let b = s.as_bytes();
    let mut last = [0usize; 26];
    for (i, &c) in b.iter().enumerate() {
        last[(c - b'a') as usize] = i;
    }
    let (mut out, mut start, mut end) = (Vec::new(), 0, 0);
    for (i, &c) in b.iter().enumerate() {
        end = end.max(last[(c - b'a') as usize]);
        if i == end {
            out.push(end - start + 1);
            start = i + 1;
        }
    }
    out
}
fn partition_brute(s: &str) -> usize {
    let b = s.as_bytes();
    let n = b.len();
    (0u32..1 << (n - 1))
        .filter(|&cuts| {
            let mut part_of = HashMap::new();
            let mut part = 0;
            b.iter().enumerate().all(|(i, &c)| {
                if i > 0 && cuts >> (i - 1) & 1 == 1 {
                    part += 1;
                }
                *part_of.entry(c).or_insert(part) == part
            })
        })
        .map(|c| c.count_ones() as usize + 1)
        .max()
        .unwrap()
}
fn q13() {
    assert_eq!(partition_labels("ababcbacadefegdehijhklij"), [9, 7, 8]);
    assert_eq!(partition_labels("eccbbbbdec"), [10]);
    let mut r = Rng(0x600D);
    for _ in 0..1500 {
        let n = 1 + r.below(11);
        let s: String = (0..n).map(|_| (b'a' + r.below(5) as u8) as char).collect();
        let parts = partition_labels(&s);
        assert_eq!(parts.iter().sum::<usize>(), n);
        assert_eq!(parts.len(), partition_brute(&s), "{s}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. Candy. Interviewer: "Each child has a rating; everyone gets at least one candy; a child with a higher rating than a neighbour gets more than that neighbour. Minimum total?" Approach:
// two one-sided greedy passes: left-to-right enforces "more than the left neighbour", right-to-left enforces the right one with `max`; sum. O(n), O(n). (O(1) space version counts slope runs.)
// Rust wrinkle: `for i in (0..n-1).rev()` underflows for n = 0 (`n - 1`): use `saturating_sub`. Sum in i64/usize. Equal ratings impose NO constraint (a common bug: `>=`).
// Oracle: raise violators to a fixpoint (the least valid assignment). Follow-ups: circular children, ratings with ties, the O(1) space version.
// ---------------------------------------------------------------------------------------------------------------------
fn candy(r: &[i32]) -> usize {
    let n = r.len();
    let mut c = vec![1usize; n];
    for i in 1..n {
        if r[i] > r[i - 1] {
            c[i] = c[i - 1] + 1;
        }
    }
    for i in (0..n.saturating_sub(1)).rev() {
        if r[i] > r[i + 1] {
            c[i] = c[i].max(c[i + 1] + 1);
        }
    }
    c.iter().sum()
}
fn candy_fixpoint(r: &[i32]) -> usize {
    let n = r.len();
    let mut c = vec![1usize; n];
    loop {
        let mut changed = false;
        for i in 0..n {
            for j in [i.wrapping_sub(1), i + 1] {
                if j < n && r[i] > r[j] && c[i] <= c[j] {
                    c[i] = c[j] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            return c.iter().sum();
        }
    }
}
fn q14() {
    assert_eq!(candy(&[1, 0, 2]), 5);
    assert_eq!(candy(&[1, 2, 2]), 4);
    assert_eq!(candy(&[]), 0);
    let mut r = Rng(0x600E);
    for _ in 0..3000 {
        let v: Vec<i32> = (0..r.below(10)).map(|_| r.int(0, 5)).collect();
        assert_eq!(candy(&v), candy_fixpoint(&v), "{v:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. Single number. Interviewer: "Every element appears twice except one; find it in O(n) time and O(1) space." Approach: XOR everything: x ^ x = 0 and x ^ 0 = x, and XOR is commutative and
// associative, so pairs cancel in any order. Rust wrinkle: `fold(0, |a, &x| a ^ x)`; works for negative i32 unchanged (XOR is bitwise, no overflow possible).
// Interviewer looks for: the algebraic reason (not "magic"), why a HashSet is O(n) space, and the follow-up that breaks it. Follow-ups: Q16, Q17; missing number (Q22).
// ---------------------------------------------------------------------------------------------------------------------
fn single_number(a: &[i32]) -> i32 {
    a.iter().fold(0, |x, &y| x ^ y)
}
fn q15() {
    assert_eq!(single_number(&[4, 1, 2, 1, 2]), 4);
    assert_eq!(single_number(&[-1]), -1);
    let mut r = Rng(0x600F);
    for _ in 0..2000 {
        let mut a: Vec<i32> = (0..r.below(10)).map(|_| r.int(-50, 50)).collect();
        let lone = r.int(100, 200);
        let mut all: Vec<i32> = a.iter().chain(a.iter()).copied().chain([lone]).collect();
        for i in (1..all.len()).rev() {
            all.swap(i, r.below(i + 1));
        }
        a.clear();
        let mut counts: HashMap<i32, u32> = HashMap::new();
        all.iter().for_each(|&x| *counts.entry(x).or_default() += 1);
        let oracle = counts.into_iter().find(|&(_, c)| c == 1).unwrap().0;
        assert_eq!(single_number(&all), oracle);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q16. Single number II. Interviewer: "Every element appears three times except one, which appears once. O(n) time, O(1) space." Approach A: for each of the 32 bit positions, count the ones across
// the array; that count mod 3 is the lone number's bit. Approach B (state machine): two words `ones`, `twos` hold a 2-bit counter per bit position cycling 0 -> 1 -> 2 -> 0:
// ones = (ones ^ x) & !twos; twos = (twos ^ x) & !ones. After all numbers, `ones` is the answer. Both O(n), A is easier to explain, B is one pass over all 32 columns at once.
// Rust wrinkle: work in `u32` so shifts and negative numbers are well defined, then `as i32` at the end (two's complement round trip); `1 << bit` with bit <= 31 on u32 is fine, bit = 32 would panic.
// Follow-ups: appears k times except one (generalise the counter), two lone numbers.
// ---------------------------------------------------------------------------------------------------------------------
fn single_number_ii_bits(a: &[i32]) -> i32 {
    let mut out = 0u32;
    for bit in 0..32 {
        let ones = a.iter().filter(|&&x| (x as u32) >> bit & 1 == 1).count();
        if ones % 3 == 1 {
            out |= 1 << bit;
        }
    }
    out as i32
}
fn single_number_ii_state(a: &[i32]) -> i32 {
    let (mut ones, mut twos) = (0u32, 0u32);
    for &x in a {
        let x = x as u32;
        ones = (ones ^ x) & !twos;
        twos = (twos ^ x) & !ones;
    }
    ones as i32
}
fn q16() {
    assert_eq!(single_number_ii_bits(&[2, 2, 3, 2]), 3);
    assert_eq!(single_number_ii_state(&[0, 1, 0, 1, 0, 1, 99]), 99);
    assert_eq!(
        single_number_ii_state(&[-2, -2, 1, 1, -3, 1, -3, -3, -4, -2]),
        -4
    );
    let mut r = Rng(0x6010);
    for _ in 0..2000 {
        let base: Vec<i32> = (0..r.below(6)).map(|_| r.next() as i32).collect();
        let lone = (r.next() as i32) | 1 << 31 & r.next() as i32; // exercise the sign bit sometimes
        let mut all: Vec<i32> = base.iter().flat_map(|&x| [x, x, x]).collect();
        if base.contains(&lone) {
            continue; // the lone value must differ from the tripled ones
        }
        all.push(lone);
        for i in (1..all.len()).rev() {
            all.swap(i, r.below(i + 1));
        }
        assert_eq!(single_number_ii_bits(&all), lone);
        assert_eq!(single_number_ii_state(&all), lone);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q17. Single number III. Interviewer: "Exactly two elements appear once, all others twice. Find both in O(n) time and O(1) space." Approach: XOR everything gives d = a ^ b, nonzero. Pick any set
// bit of d (the lowest: d & -d); a and b differ at that bit, so partitioning the array by it separates a from b, and each half is a Q15 instance (pairs stay together). O(n), O(1).
// Rust wrinkle: `d & d.wrapping_neg()`: plain `-d` panics in debug if d == i32::MIN (negation overflow); `wrapping_neg` is exactly two's complement negation. Return sorted for determinism.
// Follow-ups: three unique numbers (hard), proof that pairs never split across halves.
// ---------------------------------------------------------------------------------------------------------------------
fn single_number_iii(a: &[i32]) -> (i32, i32) {
    let d = a.iter().fold(0, |x, &y| x ^ y);
    let low = d & d.wrapping_neg();
    let (mut x, mut y) = (0, 0);
    for &v in a {
        if v & low == 0 { x ^= v } else { y ^= v }
    }
    (x.min(y), x.max(y))
}
fn q17() {
    assert_eq!(single_number_iii(&[1, 2, 1, 3, 2, 5]), (3, 5));
    assert_eq!(
        single_number_iii(&[i32::MIN, 0]),
        (i32::MIN, 0),
        "d = i32::MIN: -d would overflow"
    );
    let mut r = Rng(0x6011);
    for _ in 0..2000 {
        let base: Vec<i32> = (0..r.below(8)).map(|_| r.next() as i32).collect();
        let (p, q) = (r.next() as i32, r.next() as i32);
        if p == q || base.contains(&p) || base.contains(&q) {
            continue;
        }
        let mut all: Vec<i32> = base.iter().flat_map(|&x| [x, x]).chain([p, q]).collect();
        for i in (1..all.len()).rev() {
            all.swap(i, r.below(i + 1));
        }
        assert_eq!(single_number_iii(&all), (p.min(q), p.max(q)));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q18. Number of 1 bits. Interviewer: "Return the number of set bits of a 32-bit unsigned integer. Faster than 32 iterations?" Approach: Kernighan: `n &= n - 1` clears the lowest set bit, so the loop runs
// popcount times. Then: `n.count_ones()` (one POPCNT instruction where available); SWAR 5-step; 8/16-bit table. Rust wrinkle: `n >> i` with i = 32 PANICS in debug for u32 ("shift right with
// overflow"); loop `0..32` only. `u32::count_ones()` is the answer a Rust interviewer wants to hear last, after you can derive the others.
// Follow-ups: parity, count over a range (Q19), sparse vs dense inputs and when the table wins.
// ---------------------------------------------------------------------------------------------------------------------
fn hamming_weight_kernighan(mut n: u32) -> u32 {
    let mut c = 0;
    while n != 0 {
        n &= n - 1;
        c += 1;
    }
    c
}
fn hamming_weight_swar(mut n: u32) -> u32 {
    n -= (n >> 1) & 0x5555_5555;
    n = (n & 0x3333_3333) + ((n >> 2) & 0x3333_3333);
    n = (n + (n >> 4)) & 0x0F0F_0F0F;
    n.wrapping_mul(0x0101_0101) >> 24
}
fn q18() {
    let loop_count = |n: u32| (0..32).filter(|&i| n >> i & 1 == 1).count() as u32; // oracle: 32 iterations, never shifting by 32
    for n in (0..=u16::MAX as u32).chain([u32::MAX, 1 << 31, 0x8000_0001]) {
        assert_eq!(hamming_weight_kernighan(n), loop_count(n));
        assert_eq!(hamming_weight_swar(n), loop_count(n));
        assert_eq!(n.count_ones(), loop_count(n));
    }
    let mut r = Rng(0x6012);
    for _ in 0..20_000 {
        let n = r.next() as u32;
        assert_eq!(hamming_weight_swar(n), n.count_ones());
        assert_eq!(hamming_weight_kernighan(n), n.count_ones());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q19. Counting bits. Interviewer: "For each i in 0..=n return its popcount; O(n) total, no per-number popcount call." Approach: DP on a smaller number: bits[i] = bits[i >> 1] + (i & 1), or
// bits[i] = bits[i & (i - 1)] + 1 (drop the lowest set bit). O(n) time and space. Rust wrinkle: `Vec<u32>` with `push` inside a loop from 1; index with usize and shift on usize (`i >> 1`)
// is fine; `i & (i - 1)` is safe for i >= 1 only (i = 0 would underflow in usize), start the loop at 1.
// Follow-ups: sum of popcounts 0..=n in O(log n) (per-bit period counting), total set bits in a range.
// ---------------------------------------------------------------------------------------------------------------------
fn count_bits(n: usize) -> Vec<u32> {
    let mut b = vec![0u32; n + 1];
    for i in 1..=n {
        b[i] = b[i >> 1] + (i & 1) as u32;
    }
    b
}
fn count_bits_lowbit(n: usize) -> Vec<u32> {
    let mut b = vec![0u32; n + 1];
    for i in 1..=n {
        b[i] = b[i & (i - 1)] + 1;
    }
    b
}
fn q19() {
    assert_eq!(count_bits(5), [0, 1, 1, 2, 1, 2]);
    let n = 1 << 16;
    let (a, b) = (count_bits(n), count_bits_lowbit(n));
    assert_eq!(a, b);
    assert!(
        a.iter()
            .enumerate()
            .all(|(i, &c)| c == (i as u32).count_ones())
    );
    assert_eq!(count_bits(0), [0]);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q20. Reverse bits. Interviewer: "Reverse the bits of a 32-bit unsigned integer. What if this is called millions of times?" Approach: loop 32 times shifting `out << 1 | n & 1`; or the swap ladder
// (halves, pairs, nibbles, bytes: five masked swaps, branch-free); for repeated calls cache a 256-entry byte table and reverse byte order. Rust wrinkle: `u32::reverse_bits()` is the
// std one-liner (a single RBIT on ARM); the loop must run exactly 32 times and `out << 1` on u32 only drops the top bit, never panics. `swap_bytes` handles the byte-order half of the table trick.
// Follow-ups: reverse bits of u64, of a byte stream, bit-reversal permutation for FFT.
// ---------------------------------------------------------------------------------------------------------------------
fn reverse_bits_loop(mut n: u32) -> u32 {
    let mut out = 0;
    for _ in 0..32 {
        out = out << 1 | n & 1;
        n >>= 1;
    }
    out
}
fn reverse_bits_ladder(mut n: u32) -> u32 {
    n = (n >> 16) | (n << 16);
    n = ((n & 0xFF00_FF00) >> 8) | ((n & 0x00FF_00FF) << 8);
    n = ((n & 0xF0F0_F0F0) >> 4) | ((n & 0x0F0F_0F0F) << 4);
    n = ((n & 0xCCCC_CCCC) >> 2) | ((n & 0x3333_3333) << 2);
    ((n & 0xAAAA_AAAA) >> 1) | ((n & 0x5555_5555) << 1)
}
fn q20() {
    let table: Vec<u8> = (0..=255u8)
        .map(|b| reverse_bits_loop(b as u32).to_be_bytes()[0])
        .collect(); // byte reversal derived from the loop
    let by_table = |n: u32| u32::from_be_bytes(n.to_le_bytes().map(|b| table[b as usize]));
    for n in (0..=u16::MAX as u32).chain([u32::MAX, 1, 1 << 31, 0xDEAD_BEEF]) {
        assert_eq!(reverse_bits_loop(n), n.reverse_bits());
        assert_eq!(reverse_bits_ladder(n), n.reverse_bits());
        assert_eq!(by_table(n), n.reverse_bits());
    }
    assert_eq!(
        reverse_bits_loop(0b0000_0010_1001_0100_0001_1110_1001_1100),
        0b0011_1001_0111_1000_0010_1001_0100_0000
    );
    let mut r = Rng(0x6014);
    for _ in 0..20_000 {
        let n = r.next() as u32;
        assert_eq!(
            reverse_bits_ladder(reverse_bits_ladder(n)),
            n,
            "reversal is an involution"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q21. Power of two (and four). Interviewer: "Is n a power of two, without loops or recursion?" Approach: n > 0 && n & (n - 1) == 0 (a power of two has exactly one set bit; subtracting 1 flips it and
// everything below). Power of four: additionally the bit sits at an even index: n & 0x5555_5555 != 0. Power of three has no bit trick: 3^19 % n == 0 for 32-bit n > 0.
// Rust wrinkle: the `n > 0 &&` must come FIRST: for n = i32::MIN, `n - 1` overflows and panics in debug (short-circuit avoids evaluating it); negatives and zero are not powers of two.
// Follow-ups: round up to the next power of two (`next_power_of_two`, panics/wraps on overflow: `checked_next_power_of_two`), the log2 via `trailing_zeros`.
// ---------------------------------------------------------------------------------------------------------------------
fn is_power_of_two(n: i32) -> bool {
    n > 0 && n & (n - 1) == 0
}
fn is_power_of_four(n: i32) -> bool {
    is_power_of_two(n) && n & 0x5555_5555 != 0
}
fn is_power_of_three(n: i32) -> bool {
    n > 0 && 1_162_261_467 % n == 0
} // 3^19, the largest power of three that fits in i32
fn q21() {
    let pow2: HashSet<i64> = (0..31).map(|i| 1i64 << i).collect();
    let pow4: HashSet<i64> = (0..16).map(|i| 1i64 << (2 * i)).collect();
    let pow3: HashSet<i64> = (0..20).map(|i| 3i64.pow(i)).collect();
    let mut probes: Vec<i32> = (-70_000..70_000).collect();
    probes.extend([
        i32::MIN,
        i32::MIN + 1,
        i32::MAX,
        1 << 30,
        (1 << 30) + 1,
        (1 << 30) - 1,
        1_162_261_467,
        1_162_261_466,
    ]);
    for n in probes {
        assert_eq!(is_power_of_two(n), pow2.contains(&(n as i64)), "{n}");
        assert_eq!(is_power_of_four(n), pow4.contains(&(n as i64)), "{n}");
        assert_eq!(is_power_of_three(n), pow3.contains(&(n as i64)), "{n}");
    }
    assert!(u32::MAX.checked_next_power_of_two().is_none());
}

// ---------------------------------------------------------------------------------------------------------------------
// Q22. Missing number. Interviewer: "nums contains n distinct numbers from 0..=n; find the missing one. O(n) time, O(1) space." Approaches: sum formula n(n+1)/2 - sum; XOR of all indices and values;
// sort + scan; binary search on a sorted array; HashSet (O(n) space). Rust wrinkle: n(n+1)/2 overflows i32 near n = 65_536 (use u64/i64 or the XOR); XOR is overflow-immune. Walk indices with `zip`.
// Follow-ups: two missing numbers (sum and sum of squares, or XOR + partition as in Q17), the first missing positive (cyclic placement).
// ---------------------------------------------------------------------------------------------------------------------
fn missing_number_xor(a: &[u32]) -> u32 {
    a.iter()
        .enumerate()
        .fold(a.len() as u32, |x, (i, &v)| x ^ i as u32 ^ v)
}
fn missing_number_sum(a: &[u32]) -> u32 {
    let n = a.len() as u64;
    (n * (n + 1) / 2 - a.iter().map(|&x| x as u64).sum::<u64>()) as u32
}
fn q22() {
    assert_eq!(missing_number_xor(&[3, 0, 1]), 2);
    assert_eq!(missing_number_sum(&[9, 6, 4, 2, 3, 5, 7, 0, 1]), 8);
    assert_eq!(missing_number_xor(&[0]), 1);
    let mut r = Rng(0x6016);
    for _ in 0..2000 {
        let n = 1 + r.below(300) as u32;
        let hole = r.below(n as usize + 1) as u32;
        let mut v: Vec<u32> = (0..=n).filter(|&x| x != hole).collect();
        for i in (1..v.len()).rev() {
            v.swap(i, r.below(i + 1));
        }
        assert_eq!(missing_number_xor(&v), hole);
        assert_eq!(missing_number_sum(&v), hole);
    }
    let big: Vec<u32> = (0..=200_000u32).filter(|&x| x != 123_456).collect();
    assert_eq!(
        missing_number_sum(&big),
        123_456,
        "the u64 accumulator avoids i32 overflow of n(n+1)/2"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q23. Sum of two integers. Interviewer: "Add a and b without + or -." Approach: XOR is addition without carries, AND shifted left by one is the carries; repeat until no carry remains:
// (a, b) = (a ^ b, (a & b) << 1). Terminates within 32 rounds for i32 (carries move up one place per round and fall off the top). Rust wrinkle: `+` panics on overflow in debug, so the
// reference result is `wrapping_add`; `(a & b) << 1` on i32 never panics (only the shift COUNT is checked), the lost top bit is the two's complement wrap. Negatives work with no special case.
// Follow-ups: subtraction (add the two's complement), multiplication by shift-add, why this does not terminate with arbitrary-precision negatives (Python).
// ---------------------------------------------------------------------------------------------------------------------
fn get_sum(mut a: i32, mut b: i32) -> i32 {
    while b != 0 {
        let carry = (a & b) << 1;
        a ^= b;
        b = carry;
    }
    a
}
fn q23() {
    for a in -300..300 {
        for b in -300..300 {
            assert_eq!(get_sum(a, b), a + b);
        }
    }
    assert_eq!(get_sum(i32::MAX, 1), i32::MIN, "wraps like wrapping_add");
    let mut r = Rng(0x6017);
    for _ in 0..50_000 {
        let (a, b) = (r.next() as i32, r.next() as i32);
        assert_eq!(get_sum(a, b), a.wrapping_add(b));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q24. Subsets by bitmask. Interviewer: "Return all subsets of a list of distinct integers (the power set)." Approach: every mask in 0..2^n selects a subset (bit i = take nums[i]); O(n 2^n) time, which
// is optimal since that is the output size. Alternatives: backtracking, iterative doubling (`for each x: extend with copies + x`). Rust wrinkle: `1u32 << n` is fine for n <= 31 and panics in debug at
// n = 32; for n <= 63 use u64, beyond that the output cannot fit in memory anyway. `(0..n).filter(|i| mask >> i & 1 == 1)` builds each subset.
// Follow-ups: subsets with duplicates (sort + skip), only k-element subsets (Gosper's hack: bit_manipulation_real_world.rs), order of output.
// ---------------------------------------------------------------------------------------------------------------------
fn subsets_mask(nums: &[i32]) -> Vec<Vec<i32>> {
    assert!(nums.len() < 32, "mask would not fit in u32");
    (0u32..1 << nums.len())
        .map(|m| {
            (0..nums.len())
                .filter(|&i| m >> i & 1 == 1)
                .map(|i| nums[i])
                .collect()
        })
        .collect()
}
fn subsets_doubling(nums: &[i32]) -> Vec<Vec<i32>> {
    let mut out = vec![vec![]];
    for &x in nums {
        let more: Vec<Vec<i32>> = out
            .iter()
            .map(|s| {
                let mut t = s.clone();
                t.push(x);
                t
            })
            .collect();
        out.extend(more);
    }
    out
}
fn q24() {
    assert_eq!(
        subsets_mask(&[1, 2]),
        [vec![], vec![1], vec![2], vec![1, 2]]
    );
    for n in 0..=10 {
        let nums: Vec<i32> = (0..n).collect();
        let (a, b) = (subsets_mask(&nums), subsets_doubling(&nums));
        assert_eq!(a.len(), 1 << n);
        assert_eq!(
            a, b,
            "both enumerations agree and share the same order for distinct inputs"
        );
        let uniq: HashSet<Vec<i32>> = a.into_iter().collect();
        assert_eq!(uniq.len(), 1 << n);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q25. Hamming distance. Interviewer: "Number of bit positions where x and y differ." Approach: `(x ^ y).count_ones()`: XOR marks the differing bits, popcount counts them. O(1).
// Rust wrinkle: use unsigned or `as u32` before shifting if you hand-roll the loop (arithmetic shift of a negative i32 never reaches 0 and loops forever); `count_ones` exists on signed types too and
// counts the two's complement bits. Follow-ups: Q26, the nearest codeword (error-correcting codes), Hamming distance of byte strings.
// ---------------------------------------------------------------------------------------------------------------------
fn hamming_distance(x: i32, y: i32) -> u32 {
    (x ^ y).count_ones()
}
fn q25() {
    assert_eq!(hamming_distance(1, 4), 2);
    assert_eq!(hamming_distance(3, 1), 1);
    assert_eq!(hamming_distance(-1, 0), 32);
    let mut r = Rng(0x6019);
    for _ in 0..5000 {
        let (x, y) = (r.next() as i32, r.next() as i32);
        let loop_oracle = (0..32)
            .filter(|&i| (x as u32 >> i & 1) != (y as u32 >> i & 1))
            .count() as u32;
        assert_eq!(hamming_distance(x, y), loop_oracle);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q26. Total Hamming distance. Interviewer: "Sum of Hamming distances over all pairs, in better than O(n^2)." Approach: bit positions are independent. For each of 32 positions with `ones`
// numbers having a 1 there, exactly ones * (n - ones) pairs differ. Sum over positions. O(32 n). Rust wrinkle: accumulate in u64: with n up to 10^4 and 32 bits the total is ~1.6e9 and would
// overflow i32; the product `ones * (n - ones)` is computed in u64 before adding. Follow-ups: pairs within a sliding window, weighted bits, the same trick for XOR-sum over pairs.
// ---------------------------------------------------------------------------------------------------------------------
fn total_hamming_distance(a: &[u32]) -> u64 {
    let n = a.len() as u64;
    (0..32)
        .map(|bit| {
            let ones = a.iter().filter(|&&x| x >> bit & 1 == 1).count() as u64;
            ones * (n - ones)
        })
        .sum()
}
fn q26() {
    assert_eq!(total_hamming_distance(&[4, 14, 2]), 6);
    let mut r = Rng(0x601A);
    for _ in 0..500 {
        let a: Vec<u32> = (0..r.below(30))
            .map(|_| r.next() as u32 >> r.below(32))
            .collect();
        let brute: u64 = (0..a.len())
            .flat_map(|i| (i + 1..a.len()).map(move |j| (i, j)))
            .map(|(i, j)| (a[i] ^ a[j]).count_ones() as u64)
            .sum();
        assert_eq!(total_hamming_distance(&a), brute);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q27. Bitwise AND of numbers range. Interviewer: "Return the AND of all integers in [m, n]." Approach: the AND keeps only the COMMON PREFIX of m and n in binary (any lower bit position flips
// somewhere in the range, so it ANDs to 0). Shift both right until equal, counting shifts, then shift back. Or: `while n > m { n &= n - 1 }` (clear lowest set bits of n until it drops to m's prefix).
// O(32). Rust wrinkle: the shift-back `m << shift` where shift <= 31 is safe; if you instead computed `!0 << shift` with shift == 32 it would panic in debug. m = 0 or m == n are the edges.
// Follow-ups: OR of a range, XOR of a range 0..=n (period 4 pattern).
// ---------------------------------------------------------------------------------------------------------------------
fn range_bitwise_and(mut m: u32, mut n: u32) -> u32 {
    let mut shift = 0;
    while m != n {
        m >>= 1;
        n >>= 1;
        shift += 1;
    }
    m << shift
}
fn range_bitwise_and_clear(m: u32, mut n: u32) -> u32 {
    while n > m {
        n &= n - 1;
    }
    n
}
fn q27() {
    assert_eq!(range_bitwise_and(5, 7), 4);
    assert_eq!(range_bitwise_and(0, 0), 0);
    assert_eq!(range_bitwise_and(1, i32::MAX as u32), 0);
    for m in 0..200u32 {
        for n in m..200 {
            let brute = (m..=n).fold(u32::MAX, |a, x| a & x);
            assert_eq!(range_bitwise_and(m, n), brute, "[{m},{n}]");
            assert_eq!(range_bitwise_and_clear(m, n), brute);
        }
    }
    assert_eq!(
        range_bitwise_and(i32::MAX as u32 - 3, i32::MAX as u32),
        i32::MAX as u32 - 3
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q28. UTF-8 validation. Interviewer: "data is a list of integers, only the least significant 8 bits of each matter. Is it a valid UTF-8 encoding (1 to 4 bytes per character)?" Approach: scan
// with a counter of remaining continuation bytes: if 0, the byte must be 0xxxxxxx (1), 110xxxxx (2), 1110xxxx (3) or 11110xxx (4), set the count; else it must match 10xxxxxx. At the end the
// counter must be 0. O(n). Rust wrinkle: `b & 0xFF` first; mask tests like `b >> 5 == 0b110`. The problem's rules are LOOSER than real UTF-8: it accepts overlong encodings (C0 80) and surrogates
// that `std::str::from_utf8` rejects. So the oracle is an implication (from_utf8 ok => accepted) plus an independent range-based parser; know the difference when asked "is this real UTF-8?".
// Follow-ups: the strict version (reject overlong, surrogates, > U+10FFFF: see the Unicode standard table 3-7), streaming validation across chunk boundaries.
// ---------------------------------------------------------------------------------------------------------------------
fn valid_utf8(data: &[i32]) -> bool {
    let mut need = 0;
    for &x in data {
        let b = (x & 0xFF) as u8;
        if need == 0 {
            need = match b {
                _ if b >> 7 == 0 => 0,
                _ if b >> 5 == 0b110 => 1,
                _ if b >> 4 == 0b1110 => 2,
                _ if b >> 3 == 0b11110 => 3,
                _ => return false,
            };
        } else if b >> 6 == 0b10 {
            need -= 1;
        } else {
            return false;
        }
    }
    need == 0
}
/// Independent oracle: parse greedily with byte RANGES (a different formulation of the same loose rules).
fn utf8_loose_oracle(b: &[u8]) -> bool {
    let cont = |x: &u8| (0x80..=0xBF).contains(x);
    match b.split_first() {
        None => true,
        Some((0x00..=0x7F, rest)) => utf8_loose_oracle(rest),
        Some((0xC0..=0xDF, rest)) => {
            rest.len() >= 1 && cont(&rest[0]) && utf8_loose_oracle(&rest[1..])
        }
        Some((0xE0..=0xEF, rest)) => {
            rest.len() >= 2 && rest[..2].iter().all(cont) && utf8_loose_oracle(&rest[2..])
        }
        Some((0xF0..=0xF7, rest)) => {
            rest.len() >= 3 && rest[..3].iter().all(cont) && utf8_loose_oracle(&rest[3..])
        }
        _ => false,
    }
}
fn q28() {
    assert!(
        valid_utf8(&[197, 130, 1])
            && !valid_utf8(&[235, 140, 4])
            && valid_utf8(&[])
            && !valid_utf8(&[0xC3])
    );
    assert!(valid_utf8(&[0x1_00 | 0x41]), "only the low 8 bits matter");
    let mut r = Rng(0x601C);
    for _ in 0..20_000 {
        let bytes: Vec<u8> = if r.below(2) == 0 {
            let s: String = (0..r.below(6))
                .map(|_| char::from_u32([0x41, 0xE9, 0x20AC, 0x1F600][r.below(4)]).unwrap())
                .collect();
            s.into_bytes() // valid by construction
        } else {
            (0..r.below(5))
                .map(|_| [0x41u8, 0x80, 0xBF, 0xC3, 0xE2, 0xF0, 0xF8, 0xC0][r.below(8)])
                .collect()
        };
        let data: Vec<i32> = bytes
            .iter()
            .map(|&b| b as i32 | (r.below(3) as i32) << 8)
            .collect(); // junk above bit 7
        assert_eq!(valid_utf8(&data), utf8_loose_oracle(&bytes), "{bytes:02x?}");
        if std::str::from_utf8(&bytes).is_ok() {
            assert!(
                valid_utf8(&data),
                "real UTF-8 must always pass the looser check"
            );
        }
    }
    assert!(
        valid_utf8(&[0xC0, 0x80])
            && std::str::from_utf8(std::hint::black_box(&[0xC0u8, 0x80])).is_err(),
        "overlong NUL: accepted by LC 393, rejected by Rust"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q29. Rust: sorting intervals correctly. Interviewer: "Sort intervals by start; ties by larger end first. Now sort by descending start. Your comparator `a.0 - b.0` passed my samples; why do I
// reject it?" Answer: subtraction overflows for extreme i32 (and returns a number, not an Ordering). Use `sort_by_key(|x| (x.0, Reverse(x.1)))`: tuples compare lexicographically and `Reverse`
// flips one component. For descending: `sort_by_key(|x| Reverse(x.0))`. f64 keys are not `Ord` (NaN): `sort_by(|a, b| a.total_cmp(b))` or `partial_cmp().unwrap()` (panics on NaN).
// `sort_unstable_by_key` is faster and fine when equal keys are indistinguishable; `sort_by_key` is STABLE (keeps input order for equal keys), which matters for multi-pass sorting. `sort_by_cached_key`
// when the key is expensive. Interviewer looks for: stability awareness, no subtraction comparators, total order for floats. Follow-ups: sort by a key borrowed from the element (lifetime issue with sort_by_key).
// ---------------------------------------------------------------------------------------------------------------------
fn q29() {
    let bad_cmp = |a: &i32, b: &i32| a.wrapping_sub(*b).cmp(&0); // what `a - b` computes in release builds (debug would panic)
    assert_eq!(
        bad_cmp(&i32::MIN, &1),
        std::cmp::Ordering::Greater,
        "i32::MIN - 1 wraps to MAX: the broken comparator says MIN > 1"
    );
    assert!(i32::MIN.cmp(&1).is_lt());
    let mut v = vec![(3, 5), (1, 2), (3, 9), (1, 7), (2, 2)];
    v.sort_by_key(|x| (x.0, Reverse(x.1)));
    assert_eq!(v, [(1, 7), (1, 2), (2, 2), (3, 9), (3, 5)]);
    v.sort_by_key(|x| Reverse(x.0));
    assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), [3, 3, 2, 1, 1]);
    // Stability: equal keys keep their relative order under sort_by_key. Oracle: sort pairs by (key, original index).
    let mut r = Rng(0x601D);
    for _ in 0..500 {
        let items: Vec<(i32, usize)> = (0..r.below(20)).map(|i| (r.int(0, 3), i)).collect();
        let mut stable = items.clone();
        stable.sort_by_key(|x| x.0);
        let mut oracle = items.clone();
        oracle.sort_by_key(|x| (x.0, x.1));
        assert_eq!(stable, oracle, "sort_by_key is stable");
    }
    // Floats: total_cmp gives a total order (NaN sorts last), partial_cmp().unwrap() would panic on NaN.
    let mut f = vec![2.5, f64::NAN, -1.0, 0.0, 1.0];
    f.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(&f[..4], [-1.0, 0.0, 1.0, 2.5]);
    assert!(f[4].is_nan());
    assert!(f64::NAN.partial_cmp(&1.0).is_none());
}

// ---------------------------------------------------------------------------------------------------------------------
// Q30. Rust: shift counts and zero inputs. Interviewer: "You compute the position of the highest set bit as `31 - n.leading_zeros()` and a mask as `1u32 << k`. What breaks?" Answer: (1) for n = 0,
// `leading_zeros()` is 32 and `31 - 32` underflows u32: PANIC in debug (wraps in release); use `n.checked_ilog2()` (None for 0) or `32 - leading_zeros` as a bit LENGTH. `ilog2(0)` itself panics.
// (2) `1u32 << k` with k >= 32 panics in debug and in release silently becomes `1 << (k % 32)`: use `checked_shl` (None), `wrapping_shl` (masks the count, rarely what you want), or branch.
// (3) `<<` never checks the VALUE overflowing, only the count: `0xFFu8 << 4` is 0xF0. (4) `>>` is arithmetic on signed, logical on unsigned. (5) `rotate_left` keeps bits; counts are taken modulo the width.
// Interviewer looks for: knowing debug-vs-release differences, picking checked_/wrapping_ deliberately. Follow-ups: `overflowing_shl`, `u128`, `NonZeroU32::ilog2` (no zero case by type).
// ---------------------------------------------------------------------------------------------------------------------
fn highest_set_bit(n: u32) -> Option<u32> {
    n.checked_ilog2()
}
fn mask_below(k: u32) -> u32 {
    1u32.checked_shl(k).map_or(u32::MAX, |m| m - 1)
} // k >= 32 means "all bits"
fn q30() {
    assert_eq!(0u32.leading_zeros(), 32);
    assert_eq!(
        0u32.checked_sub(1),
        None,
        "what 31 - 32 would be on a u32 zero case: no value, a panic in debug"
    );
    assert_eq!(highest_set_bit(0), None);
    assert_eq!(highest_set_bit(1), Some(0));
    assert_eq!(highest_set_bit(u32::MAX), Some(31));
    for n in (1..=u16::MAX as u32).chain([u32::MAX, 1 << 31]) {
        assert_eq!(highest_set_bit(n), Some(31 - n.leading_zeros()));
        assert_eq!(n.ilog2(), 31 - n.leading_zeros());
        assert_eq!(
            32 - n.leading_zeros(),
            (0..32).filter(|&i| n >> i != 0).count() as u32,
            "bit length"
        );
    }
    assert_eq!(1u32.checked_shl(32), None);
    assert_eq!(
        1u32.wrapping_shl(33),
        2,
        "wrapping_shl masks the COUNT: 33 acts as 1"
    );
    assert_eq!(1u32.overflowing_shl(32), (1, true));
    assert_eq!(mask_below(0), 0);
    assert_eq!(mask_below(5), 0b11111);
    assert_eq!(mask_below(31), 0x7FFF_FFFF);
    assert_eq!(mask_below(32), u32::MAX);
    assert_eq!(mask_below(1000), u32::MAX);
    assert_eq!(
        0xFFu8 << std::hint::black_box(4),
        0xF0,
        "bits shifted out are lost without any panic"
    );
    assert_eq!((-16i32) >> 2, -4);
    assert_eq!(((-16i32) as u32) >> 2, 0x3FFF_FFFC);
    assert_eq!(0x8000_0001u32.rotate_left(1), 3);
    assert_eq!(0x1234_5678u32.swap_bytes(), 0x7856_3412);
    assert_eq!(
        0x1234_5678u32.rotate_left(32 + 4),
        0x1234_5678u32.rotate_left(4),
        "rotate counts wrap modulo the width"
    );
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
    q19();
    println!("Q19: ok");
    q20();
    println!("Q20: ok");
    q21();
    println!("Q21: ok");
    q22();
    println!("Q22: ok");
    q23();
    println!("Q23: ok");
    q24();
    println!("Q24: ok");
    q25();
    println!("Q25: ok");
    q26();
    println!("Q26: ok");
    q27();
    println!("Q27: ok");
    q28();
    println!("Q28: ok");
    q29();
    println!("Q29: ok");
    q30();
    println!("Q30: ok");
}
