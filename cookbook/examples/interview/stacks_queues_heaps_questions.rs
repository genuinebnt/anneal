//! RECIPE 612 (interview): stack, queue and heap interview questions in Rust (monotonic stacks, deques, BinaryHeap with Reverse, k-way merge).
//! LEVEL: intermediate
//! Use in: coding interviews (LeetCode stack/queue/heap tags), schedulers, streaming statistics, top-k and merge pipelines.   Run: cargo run --release --example stacks_queues_heaps_questions
//! COMMON USES: parsers and calculators, next-greater problems, sliding windows, top-k, k-way merge (external sort), task schedulers, medians.
#![allow(dead_code)]
//! Read: https://www.techinterviewhandbook.org/grind75 , https://interviewing.io/rust-interview-questions , https://dev.to/seanchen1991/implementing-an-lru-cache-in-rust-33pp ,
//! https://rust-unofficial.github.io/too-many-lists/ , https://www.techinterview.org/post/3233460609/linked-list-interview-patterns/
//! See also: examples/data_structures/lru_cache.rs (the O(1) LRU), examples/interview/linked_lists_in_rust.rs (merge k lists on Box lists).
//!
//! QUESTION BANK
//! Q1. Valid parentheses: are the brackets ()[]{} in a string balanced and properly nested? [Easy] [asked: very common]
//! Q2. Design a stack that returns its minimum (and maximum) in O(1). [Medium] [asked: very common]
//! Q3. Implement a queue using two stacks, and a stack using two queues. [Easy] [asked: very common]
//! Q4. Evaluate an expression in reverse Polish notation. [Medium] [asked: very common]
//! Q5. Basic calculator: evaluate a string with + - ( ) and spaces (including unary minus). [Hard] [asked: common]
//! Q6. Decode string: "3[a2[c]]" becomes "accaccacc". [Medium] [asked: common]
//! Q7. Simplify a Unix absolute path ("/a/./b/../../c/" becomes "/c"). [Medium] [asked: common]
//! Q8. Next greater element (I, circular II) and daily temperatures with a monotonic stack. [Medium] [asked: very common]
//! Q9. Largest rectangle in a histogram, and the maximal rectangle of 1s in a binary matrix. [Hard] [asked: common]
//! Q10. Trapping rain water: two pointers and monotonic stack. [Hard] [asked: very common]
//! Q11. Sliding window maximum with a deque in O(n). [Hard] [asked: very common]
//! Q12. Design a circular queue (ring buffer) with fixed capacity. [Medium] [asked: common]
//! Q13. Implement an LRU cache the naive way with a VecDeque (the O(n) baseline) and say why it is naive. [Medium] [asked: common]
//! Q14. Kth largest element: min-heap of size k versus quickselect. [Medium] [asked: very common]
//! Q15. Top k frequent elements: heap and bucket sort. [Medium] [asked: very common]
//! Q16. Merge k sorted arrays: heap with Reverse, divide and conquer, and counting comparisons. [Hard] [asked: very common]
//! Q17. Find the median of a data stream (two heaps) and the median of every sliding window. [Hard] [asked: very common]
//! Q18. Task scheduler with cooldown, and reorganize a string so no two neighbours are equal (heap + greedy). [Medium] [asked: common]
//! Q19. Meeting rooms II: the minimum number of rooms for a set of intervals. [Medium] [asked: very common]
//! Q20. K closest points to the origin. [Medium] [asked: very common]
//! Q21. Connect ropes with minimum total cost (Huffman-like merging). [Medium] [asked: common]
//! Q22. Smallest range that includes at least one number from each of k sorted lists. [Hard] [asked: occasional]
//! Q23. Implement a binary heap from scratch: sift up/down, O(n) heapify, and decrease-key with an indexed heap. [Medium] [asked: common]
//! Q24. Rust: BinaryHeap is a max-heap; how do you get a min-heap, order custom structs, and handle f64 (not Ord)? [Medium] [asked: very common]
//! Q25. Rust: what are peek_mut and PeekMut::pop, and why is VecDeque (not Vec) the queue type? [Medium] [asked: common]

use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::binary_heap::PeekMut;
use std::collections::{BTreeSet, BinaryHeap, HashMap, VecDeque};

// ---------------------------------------------------------------------------------------------------------------------
// Plumbing: seeded xorshift and random vector helper.
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
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
}
fn rand_vec(r: &mut Rng, max_len: usize, lo: i32, hi: i32) -> Vec<i32> {
    let n = r.range(max_len + 1);
    (0..n).map(|_| r.int(lo, hi)).collect()
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Valid parentheses. "()[]{}" true, "(]" false, "([)]" false, "{[]}" true, "" true (vacuously). Approach: push the EXPECTED closer when you
// see an opener; on a closer, the stack top must equal it. O(n) time, O(n) space. Rust wrinkle: `Vec<char>` is the stack and `st.pop() !=
// Some(c)` handles "empty stack" and "mismatch" in one comparison (no separate is_empty check). Mistakes: forgetting the final `is_empty()`
// ("((("), popping an empty stack. Follow-ups: longest valid parentheses; minimum insertions to balance; other characters mixed in.
// ---------------------------------------------------------------------------------------------------------------------
fn is_valid(s: &str) -> bool {
    let mut st = Vec::new();
    for c in s.chars() {
        match c {
            '(' => st.push(')'),
            '[' => st.push(']'),
            '{' => st.push('}'),
            ')' | ']' | '}' => {
                if st.pop() != Some(c) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    st.is_empty()
}
fn is_valid_oracle(s: &str) -> bool {
    let mut x = s.to_string(); // repeatedly delete adjacent matched pairs until nothing changes
    loop {
        let y = x.replace("()", "").replace("[]", "").replace("{}", "");
        if y == x {
            return x.is_empty();
        }
        x = y;
    }
}
fn valid_string(r: &mut Rng, pairs: usize) -> String {
    if pairs == 0 {
        return String::new();
    }
    let inner = r.range(pairs);
    let (o, c) = [('(', ')'), ('[', ']'), ('{', '}')][r.range(3)];
    format!(
        "{o}{}{c}{}",
        valid_string(r, inner),
        valid_string(r, pairs - 1 - inner)
    )
}
fn q1() {
    for (s, want) in [
        ("()[]{}", true),
        ("(]", false),
        ("([)]", false),
        ("{[]}", true),
        ("", true),
        ("(((", false),
        (")", false),
    ] {
        assert_eq!(is_valid(s), want, "{s}");
    }
    let mut r = Rng(0x2001);
    let (mut yes, mut no) = (0, 0);
    for _ in 0..2000 {
        let s: String = if r.range(2) == 0 {
            let p = r.range(6);
            let mut v: Vec<char> = valid_string(&mut r, p).chars().collect();
            if !v.is_empty() && r.range(3) == 0 {
                let i = r.range(v.len());
                v[i] = ['(', ')', '[', ']', '{', '}'][r.range(6)]; // corrupt one bracket
            }
            v.into_iter().collect()
        } else {
            (0..r.range(10))
                .map(|_| ['(', ')', '[', ']', '{', '}'][r.range(6)])
                .collect()
        };
        let want = is_valid_oracle(&s);
        assert_eq!(is_valid(&s), want, "{s}");
        if want { yes += 1 } else { no += 1 }
    }
    assert!(yes > 200 && no > 200);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. MinStack: push, pop, top, get_min in O(1) each (and a MaxStack the same way). Approach: store with each element the min/max OF THE STACK
// BELOW AND INCLUDING IT, i.e. `(value, min, max)`; the answers are always on top. O(1) time, O(n) space. Alternative: a second stack
// that only records new minimums (push on <=, pop when equal; the `<=` matters for duplicates). Rust wrinkle: `Vec<(i32,i32,i32)>` and
// `last()` return references, so `get_min` is `Option<i32>` via `map(|t| t.1)`; nothing needs `Rc`.
// Mistakes: popping the min stack on `<` for duplicates; tracking a single global min that is wrong after pop. Follow-up: max-frequency stack.
// ---------------------------------------------------------------------------------------------------------------------
struct MinMaxStack {
    items: Vec<(i32, i32, i32)>, // (value, min so far, max so far)
}
impl MinMaxStack {
    fn new() -> Self {
        MinMaxStack { items: Vec::new() }
    }
    fn push(&mut self, v: i32) {
        let (mn, mx) = self
            .items
            .last()
            .map_or((v, v), |&(_, a, b)| (a.min(v), b.max(v)));
        self.items.push((v, mn, mx));
    }
    fn pop(&mut self) -> Option<i32> {
        self.items.pop().map(|t| t.0)
    }
    fn top(&self) -> Option<i32> {
        self.items.last().map(|t| t.0)
    }
    fn min(&self) -> Option<i32> {
        self.items.last().map(|t| t.1)
    }
    fn max(&self) -> Option<i32> {
        self.items.last().map(|t| t.2)
    }
}
fn q2() {
    let mut r = Rng(0x2002);
    let (mut s, mut model) = (MinMaxStack::new(), Vec::<i32>::new());
    for _ in 0..20000 {
        if r.range(5) < 3 {
            let v = r.int(-5, 5); // many duplicates on purpose
            s.push(v);
            model.push(v);
        } else {
            assert_eq!(s.pop(), model.pop());
        }
        assert_eq!(s.top(), model.last().copied());
        assert_eq!(s.min(), model.iter().min().copied());
        assert_eq!(s.max(), model.iter().max().copied());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. (a) Queue with two stacks: push onto `inp`; to pop/peek, if `out` is empty move EVERYTHING from `inp` to `out` (reversing order).
// Each element moves at most once, so operations are O(1) AMORTISED (a single pop can cost O(n)). (b) Stack with two queues: make push
// O(n) (enqueue to the empty queue, then drain the old queue behind it) so pop is O(1); a single queue with n-1 rotations also works.
// Rust wrinkle: peek on the queue needs `&mut self` (it may shuffle elements), a classic API surprise; returning `Option<&T>` ties the
// reference to that mutable borrow. `VecDeque` provides `pop_front` O(1). Mistakes: moving elements on every pop (O(n) each);
// moving while `out` is non-empty (breaks order). Follow-ups: amortised vs worst-case; implement with a linked list; thread-safe version.
// ---------------------------------------------------------------------------------------------------------------------
struct QueueTwoStacks<T> {
    inp: Vec<T>,
    out: Vec<T>,
}
impl<T> QueueTwoStacks<T> {
    fn new() -> Self {
        QueueTwoStacks {
            inp: vec![],
            out: vec![],
        }
    }
    fn push(&mut self, x: T) {
        self.inp.push(x);
    }
    fn shuffle(&mut self) {
        if self.out.is_empty() {
            while let Some(x) = self.inp.pop() {
                self.out.push(x);
            }
        }
    }
    fn pop(&mut self) -> Option<T> {
        self.shuffle();
        self.out.pop()
    }
    fn peek(&mut self) -> Option<&T> {
        self.shuffle();
        self.out.last()
    }
    fn len(&self) -> usize {
        self.inp.len() + self.out.len()
    }
}
struct StackTwoQueues<T> {
    q: VecDeque<T>,
    aux: VecDeque<T>,
}
impl<T> StackTwoQueues<T> {
    fn new() -> Self {
        StackTwoQueues {
            q: VecDeque::new(),
            aux: VecDeque::new(),
        }
    }
    fn push(&mut self, x: T) {
        self.aux.push_back(x);
        while let Some(y) = self.q.pop_front() {
            self.aux.push_back(y);
        }
        std::mem::swap(&mut self.q, &mut self.aux);
    }
    fn pop(&mut self) -> Option<T> {
        self.q.pop_front()
    }
    fn top(&self) -> Option<&T> {
        self.q.front()
    }
}
fn q3() {
    let mut r = Rng(0x2003);
    let (mut qs, mut model) = (QueueTwoStacks::new(), VecDeque::new());
    let (mut ss, mut smodel) = (StackTwoQueues::new(), Vec::new());
    for i in 0..20000 {
        match r.range(3) {
            0 | 1 => {
                qs.push(i);
                model.push_back(i);
                ss.push(i);
                smodel.push(i);
            }
            _ => {
                assert_eq!(qs.pop(), model.pop_front());
                assert_eq!(ss.pop(), smodel.pop());
            }
        }
        assert_eq!(qs.peek().copied(), model.front().copied());
        assert_eq!(ss.top(), smodel.last());
        assert_eq!(qs.len(), model.len());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. Evaluate reverse Polish notation: ["2","1","+","3","*"] = 9; ["4","13","5","/","+"] = 6; division truncates toward zero. O(n) time,
// O(n) space. Approach: numbers push; an operator pops b THEN a (order matters for - and /), pushes `a op b`. Rust wrinkle: `?` on `Option`
// returns `None` for malformed input (stack underflow, bad token, divide by zero, leftovers) without panics; `i64` `/` truncates toward zero
// like the problem demands (not floor). Mistakes: `a`/`b` swapped, using floor division, forgetting to check a single value remains.
// Follow-ups: infix to RPN (shunting-yard), support functions/unary ops, overflow with checked_*.
// ---------------------------------------------------------------------------------------------------------------------
fn eval_rpn(tokens: &[String]) -> Option<i64> {
    let mut st: Vec<i64> = Vec::new();
    for tok in tokens {
        match tok.as_str() {
            op @ ("+" | "-" | "*" | "/") => {
                let (b, a) = (st.pop()?, st.pop()?);
                st.push(match op {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    _ => a.checked_div(b)?,
                });
            }
            n => st.push(n.parse().ok()?),
        }
    }
    if st.len() == 1 { st.pop() } else { None }
}
#[derive(Clone)]
enum Expr {
    Num(i64),
    Bin(char, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}
impl Expr {
    fn eval(&self) -> Option<i64> {
        match self {
            Expr::Num(n) => Some(*n),
            Expr::Neg(e) => Some(-e.eval()?),
            Expr::Bin(op, a, b) => {
                let (x, y) = (a.eval()?, b.eval()?);
                match op {
                    '+' => Some(x + y),
                    '-' => Some(x - y),
                    '*' => Some(x * y),
                    _ => x.checked_div(y),
                }
            }
        }
    }
    fn rpn(&self, out: &mut Vec<String>) {
        match self {
            Expr::Num(n) => out.push(n.to_string()),
            Expr::Neg(e) => {
                out.push("0".into());
                e.rpn(out);
                out.push("-".into());
            }
            Expr::Bin(op, a, b) => {
                a.rpn(out);
                b.rpn(out);
                out.push(op.to_string());
            }
        }
    }
}
fn rand_expr(r: &mut Rng, depth: usize, ops: &[char], neg: bool) -> Expr {
    if depth == 0 || r.range(4) == 0 {
        return Expr::Num(r.int(0, 20) as i64);
    }
    if neg && r.range(5) == 0 {
        return Expr::Neg(Box::new(rand_expr(r, depth - 1, ops, neg)));
    }
    let op = ops[r.range(ops.len())];
    Expr::Bin(
        op,
        Box::new(rand_expr(r, depth - 1, ops, neg)),
        Box::new(rand_expr(r, depth - 1, ops, neg)),
    )
}
fn q4() {
    let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(eval_rpn(&v(&["2", "1", "+", "3", "*"])), Some(9));
    assert_eq!(eval_rpn(&v(&["4", "13", "5", "/", "+"])), Some(6));
    assert_eq!(
        eval_rpn(&v(&[
            "10", "6", "9", "3", "+", "-11", "*", "/", "*", "17", "+", "5", "+"
        ])),
        Some(22)
    );
    assert_eq!(
        eval_rpn(&v(&["-7", "2", "/"])),
        Some(-3),
        "truncates toward zero"
    );
    assert_eq!(eval_rpn(&v(&["1", "0", "/"])), None);
    assert_eq!(eval_rpn(&v(&["+"])), None);
    assert_eq!(eval_rpn(&v(&["1", "2"])), None);
    let mut r = Rng(0x2004);
    let mut checked = 0;
    for _ in 0..2000 {
        let e = rand_expr(&mut r, 3, &['+', '-', '*', '/'], false);
        let mut toks = vec![];
        e.rpn(&mut toks);
        assert_eq!(eval_rpn(&toks), e.eval());
        checked += e.eval().is_some() as usize;
    }
    assert!(checked > 1000);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. Basic calculator: evaluate "1 + (2 - (3 + 4))" with + - parentheses and spaces, including unary minus "(-2)" and "-(2+3)". No eval().
// Approach: one pass, O(n) time, O(depth) space. Keep `res` (running total of the current bracket level), `sign` (sign of the NEXT number)
// and `num`. On '(' push (res, sign) and reset; on ')' finish the number, then `res = saved_res + saved_sign * res`. Why it works: a
// bracket is just a number whose sign is decided before the bracket opened. Rust wrinkle: `Vec<(i64, i64)>` as the stack, `to_digit` for
// digits, a `match` over `char`; no recursion, so it survives 100k-deep nesting. Mistakes: flushing `num` on ')' and at the end only
// (forgetting one), treating '-' before '(' specially. Follow-ups: add * and / (precedence stack, 'Basic Calculator II'), variables, errors.
// ---------------------------------------------------------------------------------------------------------------------
fn calculate(s: &str) -> i64 {
    let mut stack: Vec<(i64, i64)> = Vec::new();
    let (mut res, mut sign, mut num) = (0i64, 1i64, 0i64);
    for c in s.chars() {
        match c {
            '0'..='9' => num = num * 10 + c.to_digit(10).unwrap() as i64,
            '+' | '-' => {
                res += sign * num;
                num = 0;
                sign = if c == '+' { 1 } else { -1 };
            }
            '(' => {
                stack.push((res, sign));
                res = 0;
                sign = 1;
                num = 0;
            }
            ')' => {
                res += sign * num;
                num = 0;
                let (prev, psign) = stack.pop().expect("unbalanced");
                res = prev + psign * res;
            }
            _ => {}
        }
    }
    res + sign * num
}
fn infix(e: &Expr, r: &mut Rng) -> String {
    let sp = |r: &mut Rng| if r.range(2) == 0 { " " } else { "" };
    match e {
        Expr::Num(n) => n.to_string(),
        Expr::Neg(inner) => match **inner {
            Expr::Num(n) => format!("(-{n})"),
            _ => format!("(-({}))", infix(inner, r)),
        },
        Expr::Bin(op, a, b) => {
            let left = infix(a, r);
            let right = infix(b, r);
            let right_is_bin = matches!(**b, Expr::Bin(..));
            let right = if right_is_bin && (*op == '-' || r.range(2) == 0) {
                format!("({right})")
            } else {
                right
            };
            let left = if matches!(**a, Expr::Bin(..)) && r.range(3) == 0 {
                format!("({left})")
            } else {
                left
            };
            format!("{left}{}{op}{}{right}", sp(r), sp(r))
        }
    }
}
fn q5() {
    assert_eq!(calculate("1 + 1"), 2);
    assert_eq!(calculate(" 2-1 + 2 "), 3);
    assert_eq!(calculate("(1+(4+5+2)-3)+(6+8)"), 23);
    assert_eq!(calculate("-(2+3)"), -5);
    assert_eq!(calculate("1-(-2)"), 3);
    assert_eq!(calculate("10-(2-(3-4))"), 7);
    let mut r = Rng(0x2005);
    for _ in 0..2000 {
        let e = rand_expr(&mut r, 5, &['+', '-'], true);
        let s = infix(&e, &mut r);
        assert_eq!(Some(calculate(&s)), e.eval(), "{s}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Decode string: k[encoded] repeats `encoded` k times, nesting allowed, k may have several digits. "3[a]2[bc]" -> "aaabcbc",
// "3[a2[c]]" -> "accaccacc". Approach: ONE stack of (string_so_far, repeat_count). Digits build k; '[' pushes (current, k) and clears;
// ']' pops (prev, k) and sets current = prev + current.repeat(k). O(output length) time. Rust wrinkle: `std::mem::take(&mut cur)` moves
// the current string onto the stack and leaves an empty one in place (no clone); `String::repeat` does the repetition.
// Mistakes: single-digit counts only ("100[a]"), forgetting to reset k after '[', recursion without index bookkeeping. Follow-up: encode.
// ---------------------------------------------------------------------------------------------------------------------
fn decode_string(s: &str) -> String {
    let mut stack: Vec<(String, usize)> = Vec::new();
    let (mut cur, mut k) = (String::new(), 0usize);
    for c in s.chars() {
        match c {
            '0'..='9' => k = k * 10 + c.to_digit(10).unwrap() as usize,
            '[' => {
                stack.push((std::mem::take(&mut cur), k));
                k = 0;
            }
            ']' => {
                let (prev, rep) = stack.pop().unwrap();
                cur = prev + &cur.repeat(rep);
            }
            _ => cur.push(c),
        }
    }
    cur
}
fn gen_encoded(r: &mut Rng, depth: usize) -> (String, String) {
    let (mut enc, mut dec) = (String::new(), String::new());
    for _ in 0..1 + r.range(3) {
        if depth > 0 && r.range(2) == 0 {
            let k = 1 + r.range(12);
            let (e, d) = gen_encoded(r, depth - 1);
            enc += &format!("{k}[{e}]");
            dec += &d.repeat(k);
        } else {
            let w: String = (0..1 + r.range(3))
                .map(|_| (b'a' + r.range(4) as u8) as char)
                .collect();
            enc += &w;
            dec += &w;
        }
    }
    (enc, dec)
}
fn q6() {
    assert_eq!(decode_string("3[a]2[bc]"), "aaabcbc");
    assert_eq!(decode_string("3[a2[c]]"), "accaccacc");
    assert_eq!(decode_string("2[abc]3[cd]ef"), "abcabccdcdcdef");
    assert_eq!(decode_string("10[a]").len(), 10);
    let mut r = Rng(0x2006);
    for _ in 0..2000 {
        let (e, d) = gen_encoded(&mut r, 3);
        assert_eq!(decode_string(&e), d, "{e}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. Simplify an absolute Unix path: "/home//foo/" -> "/home/foo", "/a/./b/../../c/" -> "/c", "/../" -> "/". Rules: "." = stay,
// ".." = parent (no-op at root), empty segments from "//" ignored, "..." is an ordinary name. Approach: split on '/', process with a stack
// of segment names, join. O(n) time, O(n) space. Rust wrinkle: the stack holds `&str` slices borrowed from the input (zero copying);
// `match seg { "" | "." => .., ".." => pop, s => push }` is the whole algorithm. Mistakes: popping when the stack is empty (panic in
// other languages; `Vec::pop` just returns None here), a trailing slash, treating "..." as parent. Follow-up: relative paths, symlinks.
// ---------------------------------------------------------------------------------------------------------------------
fn simplify_path(p: &str) -> String {
    let mut st: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                st.pop();
            }
            s => st.push(s),
        }
    }
    format!("/{}", st.join("/"))
}
fn simplify_oracle(p: &str) -> String {
    let mut s = format!("{p}/"); // rewrite-until-fixpoint: independent of the stack idea
    loop {
        if s.contains("//") {
            s = s.replace("//", "/");
        } else if s.contains("/./") {
            s = s.replace("/./", "/");
        } else if let Some(i) = s.find("/../") {
            if i == 0 {
                s.replace_range(0..3, "");
            } else {
                let j = s[..i].rfind('/').unwrap();
                s.replace_range(j..i + 3, "");
            }
        } else {
            break;
        }
    }
    if s.len() > 1 {
        s.pop();
    }
    s
}
fn q7() {
    assert_eq!(simplify_path("/home//foo/"), "/home/foo");
    assert_eq!(simplify_path("/a/./b/../../c/"), "/c");
    assert_eq!(simplify_path("/../"), "/");
    assert_eq!(simplify_path("/.../a"), "/.../a");
    let mut r = Rng(0x2007);
    for _ in 0..2000 {
        let segs = ["a", "b", "..", ".", "", "c", "..."];
        let mut p = String::from("/");
        for _ in 0..r.range(8) {
            p += segs[r.range(segs.len())];
            p += "/";
        }
        if r.range(3) == 0 && p.len() > 1 {
            p.pop();
        }
        assert_eq!(simplify_path(&p), simplify_oracle(&p), "{p}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. Monotonic stack, the pattern behind many "next greater/smaller" questions. (a) Next greater element: for each x, the first larger value
// to its right, else -1: [2,1,2,4,3] -> [4,2,4,-1,-1]. (b) Circular version (II): the array wraps around. (c) Daily temperatures: days to wait
// for a warmer day. (d) Next Greater Element I: answer for a subset of values. Approach: keep a stack of INDICES whose answer is pending, values
// decreasing from bottom to top; a new larger element resolves everything it beats. Each index is pushed and popped once: O(n) time and space.
// Circular: iterate 0..2n, push only in the first pass. Rust wrinkle: `while let Some(&j) = st.last()` peeks without popping and the borrow
// ends before `st.pop()`; results are written by index so no map from value is needed (that is only for NGE I, duplicates break it).
// Mistakes: strict vs non-strict comparison with duplicates (next STRICTLY greater), O(n^2) rescans. Follow-ups: previous smaller, stock span.
// ---------------------------------------------------------------------------------------------------------------------
fn next_greater(nums: &[i32], circular: bool) -> Vec<i32> {
    let n = nums.len();
    let mut res = vec![-1; n];
    let mut st: Vec<usize> = Vec::new();
    for i in 0..if circular { 2 * n } else { n } {
        let x = nums[i % n];
        while let Some(&j) = st.last() {
            if nums[j] < x {
                res[j] = x;
                st.pop();
            } else {
                break;
            }
        }
        if i < n {
            st.push(i);
        }
    }
    res
}
fn daily_temperatures(t: &[i32]) -> Vec<usize> {
    let mut res = vec![0; t.len()];
    let mut st: Vec<usize> = Vec::new();
    for i in 0..t.len() {
        while let Some(&j) = st.last() {
            if t[j] < t[i] {
                res[j] = i - j;
                st.pop();
            } else {
                break;
            }
        }
        st.push(i);
    }
    res
}
fn next_greater_i(nums1: &[i32], nums2: &[i32]) -> Vec<i32> {
    let ng = next_greater(nums2, false);
    let at: HashMap<i32, i32> = nums2.iter().copied().zip(ng).collect(); // values are distinct in NGE I
    nums1.iter().map(|x| at[x]).collect()
}
fn q8() {
    assert_eq!(next_greater(&[2, 1, 2, 4, 3], false), vec![4, 2, 4, -1, -1]);
    assert_eq!(next_greater(&[1, 2, 1], true), vec![2, -1, 2]);
    assert_eq!(
        daily_temperatures(&[73, 74, 75, 71, 69, 72, 76, 73]),
        vec![1, 1, 4, 2, 1, 1, 0, 0]
    );
    assert_eq!(next_greater_i(&[4, 1, 2], &[1, 3, 4, 2]), vec![-1, 3, -1]);
    let mut r = Rng(0x2008);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 14, -4, 4);
        let n = v.len();
        let brute = |circ: bool| -> Vec<i32> {
            (0..n)
                .map(|i| {
                    (1..if circ { n } else { n - i })
                        .map(|d| v[(i + d) % n])
                        .find(|&x| x > v[i])
                        .unwrap_or(-1)
                })
                .collect()
        };
        assert_eq!(next_greater(&v, false), brute(false));
        assert_eq!(next_greater(&v, true), brute(true));
        let wait: Vec<usize> = (0..n)
            .map(|i| (i + 1..n).find(|&j| v[j] > v[i]).map_or(0, |j| j - i))
            .collect();
        assert_eq!(daily_temperatures(&v), wait);
        let mut perm: Vec<i32> = (0..8).collect();
        for i in (1..8).rev() {
            perm.swap(i, r.range(i + 1));
        }
        let sub: Vec<i32> = perm.iter().copied().filter(|_| r.range(2) == 0).collect();
        let want: Vec<i32> = sub
            .iter()
            .map(|x| {
                let p = perm.iter().position(|y| y == x).unwrap();
                perm[p + 1..].iter().copied().find(|y| y > x).unwrap_or(-1)
            })
            .collect();
        assert_eq!(next_greater_i(&sub, &perm), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. (a) Largest rectangle in a histogram: [2,1,5,6,2,3] -> 10 (5 and 6, width 2). (b) Maximal rectangle: the largest rectangle containing
// only 1s in a binary matrix. Approach (a): monotonic stack of INCREASING heights; when a bar is shorter than the top, pop: the popped bar is
// the minimum of a maximal span whose left edge is the new top + 1 and right edge is the current index. Append a sentinel height 0 to flush.
// O(n) time, O(n) space (brute force: O(n^2) over all pairs with a running min). (b): turn each row into a histogram of consecutive 1s above
// (heights update in O(cols)) and run (a): O(rows*cols). Rust wrinkle: the sentinel is the loop running to `i == n` with height 0, so no
// second cleanup loop; widths come from `st.last()` (None = reaches the left border). Mistakes: using `>` instead of `>=` (fine) but
// computing width as `i - top` instead of `i - left - 1`; forgetting to flush; using i32 for areas that overflow.
// ---------------------------------------------------------------------------------------------------------------------
fn largest_rect(h: &[usize]) -> usize {
    let (mut st, mut best): (Vec<usize>, usize) = (vec![], 0);
    for i in 0..=h.len() {
        let cur = if i == h.len() { 0 } else { h[i] };
        while let Some(&top) = st.last() {
            if h[top] < cur {
                break;
            }
            st.pop();
            let width = st.last().map_or(i, |&l| i - l - 1);
            best = best.max(h[top] * width);
        }
        st.push(i);
    }
    best
}
fn maximal_rectangle(m: &[Vec<u8>]) -> usize {
    let cols = m.first().map_or(0, |r| r.len());
    let mut heights = vec![0usize; cols];
    let mut best = 0;
    for row in m {
        for (h, &c) in heights.iter_mut().zip(row) {
            *h = if c == 1 { *h + 1 } else { 0 };
        }
        best = best.max(largest_rect(&heights));
    }
    best
}
fn q9() {
    assert_eq!(largest_rect(&[2, 1, 5, 6, 2, 3]), 10);
    assert_eq!(largest_rect(&[]), 0);
    assert_eq!(largest_rect(&[4]), 4);
    assert_eq!(largest_rect(&[2, 2, 2]), 6);
    let m: Vec<Vec<u8>> = ["10100", "10111", "11111", "10010"]
        .iter()
        .map(|s| s.bytes().map(|b| b - b'0').collect())
        .collect();
    assert_eq!(maximal_rectangle(&m), 6);
    let mut r = Rng(0x2009);
    for _ in 0..2000 {
        let h: Vec<usize> = (0..r.range(10)).map(|_| r.range(7)).collect();
        let mut best = 0;
        for i in 0..h.len() {
            let mut mn = usize::MAX;
            for j in i..h.len() {
                mn = mn.min(h[j]);
                best = best.max(mn * (j - i + 1));
            }
        }
        assert_eq!(largest_rect(&h), best);
        let (rows, cols) = (r.range(5), 1 + r.range(5));
        let mat: Vec<Vec<u8>> = (0..rows)
            .map(|_| (0..cols).map(|_| r.range(2) as u8).collect())
            .collect();
        let mut want = 0;
        for r1 in 0..rows {
            for r2 in r1..rows {
                for c1 in 0..cols {
                    for c2 in c1..cols {
                        if (r1..=r2).all(|i| (c1..=c2).all(|j| mat[i][j] == 1)) {
                            want = want.max((r2 - r1 + 1) * (c2 - c1 + 1));
                        }
                    }
                }
            }
        }
        assert_eq!(maximal_rectangle(&mat), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Trapping rain water: heights [0,1,0,2,1,0,1,3,2,1,2,1] -> 6 units of water after rain. Water above bar i = min(max_left, max_right) - h[i].
// Approaches: (1) prefix/suffix max arrays: O(n) time, O(n) space. (2) TWO POINTERS: move the side with the smaller bar; its water is
// decided by its own side's max because the other side's max is at least as high: O(n) time, O(1) space. (3) monotonic stack: when a taller
// bar arrives, pop the valley and add the bounded rectangle layer: O(n) time, O(n) space. Rust wrinkle: pointers are `usize` with `lo < hi`
// (no `hi - 1` underflow because we test before moving); the stack version pops indices. Mistakes: moving the wrong pointer; counting negative water.
// Follow-ups: 2D trapping rain water (priority queue, Dijkstra-like), container with most water.
// ---------------------------------------------------------------------------------------------------------------------
fn trap_two_pointers(h: &[i32]) -> i32 {
    if h.is_empty() {
        return 0;
    }
    let (mut lo, mut hi) = (0usize, h.len() - 1);
    let (mut lmax, mut rmax, mut water) = (0, 0, 0);
    while lo < hi {
        if h[lo] < h[hi] {
            lmax = lmax.max(h[lo]);
            water += lmax - h[lo];
            lo += 1;
        } else {
            rmax = rmax.max(h[hi]);
            water += rmax - h[hi];
            hi -= 1;
        }
    }
    water
}
fn trap_stack(h: &[i32]) -> i32 {
    let (mut st, mut water): (Vec<usize>, i32) = (vec![], 0);
    for i in 0..h.len() {
        while let Some(&top) = st.last() {
            if h[top] >= h[i] {
                break;
            }
            st.pop();
            if let Some(&left) = st.last() {
                let bounded = h[left].min(h[i]) - h[top];
                water += bounded * (i - left - 1) as i32;
            }
        }
        st.push(i);
    }
    water
}
fn q10() {
    let x = [0, 1, 0, 2, 1, 0, 1, 3, 2, 1, 2, 1];
    assert_eq!((trap_two_pointers(&x), trap_stack(&x)), (6, 6));
    assert_eq!(trap_two_pointers(&[4, 2, 0, 3, 2, 5]), 9);
    assert_eq!(trap_two_pointers(&[]), 0);
    let mut r = Rng(0x200A);
    for _ in 0..2000 {
        let h = rand_vec(&mut r, 16, 0, 6);
        let want: i32 = (0..h.len())
            .map(|i| {
                (h[..=i]
                    .iter()
                    .max()
                    .unwrap()
                    .min(h[i..].iter().max().unwrap())
                    - h[i])
                    .max(0)
            })
            .sum();
        assert_eq!(trap_two_pointers(&h), want);
        assert_eq!(trap_stack(&h), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Sliding window maximum: for each window of size k, the maximum. [1,3,-1,-3,5,3,6,7], k=3 -> [3,3,5,5,6,7]. Brute force O(n*k).
// Optimal O(n): a DEQUE of indices with strictly decreasing values; the front is always the window maximum. For each new element: pop the
// front if it left the window (index <= i-k), pop the back while it is <= the new value (it can never be a maximum again), push, and
// record the front once i >= k-1. Each index enters and leaves once. Rust wrinkle: `VecDeque<usize>` holds indices (so expiry is a
// comparison, no element search); `back()`/`front()` return `Option<&usize>`, copied with `&`. Mistakes: storing values (cannot expire),
// `<` instead of `<=` (keeps duplicates, still correct but grows), k > n. Follow-ups: sliding minimum, window median (Q17), max of all subarrays.
// ---------------------------------------------------------------------------------------------------------------------
fn sliding_max(v: &[i32], k: usize) -> Vec<i32> {
    let mut dq: VecDeque<usize> = VecDeque::new();
    let mut out = Vec::with_capacity(v.len().saturating_sub(k) + 1);
    for i in 0..v.len() {
        if dq.front().is_some_and(|&f| f + k <= i) {
            dq.pop_front();
        }
        while dq.back().is_some_and(|&b| v[b] <= v[i]) {
            dq.pop_back();
        }
        dq.push_back(i);
        if i + 1 >= k {
            out.push(v[*dq.front().unwrap()]);
        }
    }
    out
}
fn q11() {
    assert_eq!(
        sliding_max(&[1, 3, -1, -3, 5, 3, 6, 7], 3),
        vec![3, 3, 5, 5, 6, 7]
    );
    assert_eq!(sliding_max(&[1], 1), vec![1]);
    assert_eq!(sliding_max(&[9, 9, 9], 2), vec![9, 9]);
    let mut r = Rng(0x200B);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 20, -5, 5);
        if v.is_empty() {
            continue;
        }
        let k = 1 + r.range(v.len());
        let want: Vec<i32> = v.windows(k).map(|w| *w.iter().max().unwrap()).collect();
        assert_eq!(sliding_max(&v, k), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. Design a circular queue (ring buffer) of fixed capacity k: enqueue, dequeue, front, rear, is_empty, is_full, all O(1), no shifting.
// Approach: a buffer of k slots, a `head` index and a `len`; the write slot is (head + len) % k. Using `len` (not head/tail only) removes the
// classic "full vs empty look the same" ambiguity. Rust wrinkle: `Vec<Option<T>>` lets us `take()` an element out WITHOUT `Default` or
// `unsafe` (a `[MaybeUninit<T>]` is the zero-overhead version); enqueue on a full buffer returns `Err(x)` so the caller gets the value back
// instead of losing it. Mistakes: modulo of a negative index, off-by-one on full, shifting the array on dequeue (O(n)).
// Follow-ups: overwrite-oldest mode (log buffers, below), power-of-two capacity with a bit mask, lock-free SPSC version with atomics.
// ---------------------------------------------------------------------------------------------------------------------
struct Ring<T> {
    buf: Vec<Option<T>>,
    head: usize,
    len: usize,
}
impl<T> Ring<T> {
    fn new(k: usize) -> Self {
        assert!(k > 0);
        Ring {
            buf: (0..k).map(|_| None).collect(),
            head: 0,
            len: 0,
        }
    }
    fn is_full(&self) -> bool {
        self.len == self.buf.len()
    }
    fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn enqueue(&mut self, x: T) -> Result<(), T> {
        if self.is_full() {
            return Err(x);
        }
        let slot = (self.head + self.len) % self.buf.len();
        self.buf[slot] = Some(x);
        self.len += 1;
        Ok(())
    }
    fn dequeue(&mut self) -> Option<T> {
        let x = self.buf[self.head].take()?;
        self.head = (self.head + 1) % self.buf.len();
        self.len -= 1;
        Some(x)
    }
    fn front(&self) -> Option<&T> {
        self.buf[self.head].as_ref()
    }
    fn rear(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            self.buf[(self.head + self.len - 1) % self.buf.len()].as_ref()
        }
    }
    /// overwrite-oldest mode used by log/telemetry buffers
    fn push_overwrite(&mut self, x: T) -> Option<T> {
        let evicted = if self.is_full() { self.dequeue() } else { None };
        let _ = self.enqueue(x);
        evicted
    }
}
fn q12() {
    let mut r = Rng(0x200C);
    for _ in 0..200 {
        let k = 1 + r.range(6);
        let (mut ring, mut model) = (Ring::new(k), VecDeque::new());
        for i in 0..200 {
            match r.range(4) {
                0 | 1 => {
                    let res = ring.enqueue(i);
                    if model.len() < k {
                        assert!(res.is_ok());
                        model.push_back(i);
                    } else {
                        assert_eq!(res, Err(i));
                    }
                }
                2 => assert_eq!(ring.dequeue(), model.pop_front()),
                _ => {
                    let ev = ring.push_overwrite(i);
                    let m = if model.len() == k {
                        model.pop_front()
                    } else {
                        None
                    };
                    model.push_back(i);
                    assert_eq!(ev, m);
                }
            }
            assert_eq!((ring.front(), ring.rear()), (model.front(), model.back()));
            assert_eq!(
                (ring.is_full(), ring.is_empty()),
                (model.len() == k, model.is_empty())
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. LRU cache, NAIVE version: get(key) and put(key, value) with capacity c; evict the least recently used key. Baseline using a
// `VecDeque<K>` for recency order plus a `HashMap<K, V>` for values: a hit must FIND the key in the deque (O(n) scan) and move it to the
// back, so operations are O(n), not the required O(1). The real answer is a HashMap + doubly linked list (arena indices in Rust, see
// examples/data_structures/lru_cache.rs, or the dev.to article in the Read list). Interviewers ask for the naive one first, then
// "can you make get O(1)?". Rust wrinkle: `K: Clone + Hash + Eq` because the key lives in both structures; `VecDeque::remove(i)` is O(min(i, n-i)).
// Mistakes: forgetting that `put` of an existing key also refreshes recency; evicting before checking for an update.
// Verified against a timestamp model (evict the key with the smallest last-used stamp).
// ---------------------------------------------------------------------------------------------------------------------
struct NaiveLru {
    cap: usize,
    order: VecDeque<i32>, // front = least recently used
    map: HashMap<i32, i32>,
}
impl NaiveLru {
    fn new(cap: usize) -> Self {
        NaiveLru {
            cap,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }
    fn touch(&mut self, k: i32) {
        if let Some(p) = self.order.iter().position(|&x| x == k) {
            self.order.remove(p);
        }
        self.order.push_back(k);
    }
    fn get(&mut self, k: i32) -> Option<i32> {
        let v = *self.map.get(&k)?;
        self.touch(k);
        Some(v)
    }
    fn put(&mut self, k: i32, v: i32) {
        if !self.map.contains_key(&k) && self.map.len() == self.cap {
            let old = self.order.pop_front().unwrap();
            self.map.remove(&old);
        }
        self.map.insert(k, v);
        self.touch(k);
    }
}
fn q13() {
    let mut c = NaiveLru::new(2);
    c.put(1, 1);
    c.put(2, 2);
    assert_eq!(c.get(1), Some(1));
    c.put(3, 3);
    assert_eq!(c.get(2), None);
    let mut r = Rng(0x200D);
    for _ in 0..100 {
        let cap = 1 + r.range(5);
        let mut lru = NaiveLru::new(cap);
        let mut model: HashMap<i32, (i32, u32)> = HashMap::new(); // key -> (value, last used stamp)
        let mut stamp = 0;
        for _ in 0..200 {
            stamp += 1;
            let k = r.int(0, 8);
            if r.range(2) == 0 {
                let got = lru.get(k);
                let want = model.get_mut(&k).map(|e| {
                    e.1 = stamp;
                    e.0
                });
                assert_eq!(got, want);
            } else {
                let v = r.int(0, 99);
                lru.put(k, v);
                if !model.contains_key(&k) && model.len() == cap {
                    let victim = *model.iter().min_by_key(|(_, e)| e.1).unwrap().0;
                    model.remove(&victim);
                }
                model.insert(k, (v, stamp));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. Kth LARGEST element of an unsorted array (not the kth distinct): [3,2,1,5,6,4], k=2 -> 5. Approaches: sort O(n log n); MIN-heap of
// size k: push each element, pop when the size exceeds k, the top is the answer: O(n log k) time, O(k) space, works on streams;
// QUICKSELECT: partition around a random pivot and recurse only into the side holding the target: O(n) average, O(n^2) worst, O(1) extra
// space, mutates the input. Use a 3-WAY partition (<, ==, >) so many duplicates do not degrade it. Rust wrinkle: `BinaryHeap` is a MAX-heap,
// so a min-heap is `BinaryHeap<Reverse<T>>`; `v.swap` and index ranges replace pointer juggling; the RNG pivot is seeded for determinism.
// Mistakes: 0- vs 1-based k, Lomuto partition with duplicates (quadratic), a max-heap of size n. Follow-ups: median of medians (worst-case O(n)).
// ---------------------------------------------------------------------------------------------------------------------
fn kth_largest_heap(nums: &[i32], k: usize) -> Option<i32> {
    let mut h = BinaryHeap::with_capacity(k + 1);
    for &x in nums {
        h.push(Reverse(x));
        if h.len() > k {
            h.pop();
        }
    }
    if h.len() == k {
        h.peek().map(|r| r.0)
    } else {
        None
    }
}
fn quickselect(v: &mut [i32], k: usize, rng: &mut Rng) -> i32 {
    let (mut lo, mut hi) = (0usize, v.len() - 1); // find the k-th smallest, 0-indexed
    loop {
        if lo == hi {
            return v[lo];
        }
        let pivot = v[lo + rng.range(hi - lo + 1)];
        let (mut lt, mut i, mut gt) = (lo, lo, hi + 1);
        while i < gt {
            match v[i].cmp(&pivot) {
                Ordering::Less => {
                    v.swap(lt, i);
                    lt += 1;
                    i += 1;
                }
                Ordering::Greater => {
                    gt -= 1;
                    v.swap(i, gt);
                }
                Ordering::Equal => i += 1,
            }
        }
        // now [lo,lt) < pivot, [lt,gt) == pivot, [gt,hi] > pivot
        if k < lt {
            hi = lt - 1;
        } else if k >= gt {
            lo = gt;
        } else {
            return pivot;
        }
    }
}
fn q14() {
    assert_eq!(kth_largest_heap(&[3, 2, 1, 5, 6, 4], 2), Some(5));
    assert_eq!(kth_largest_heap(&[3, 2, 3, 1, 2, 4, 5, 5, 6], 4), Some(4));
    assert_eq!(kth_largest_heap(&[1], 2), None);
    let mut rng = Rng(0x2F);
    let mut r = Rng(0x200E);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 25, -6, 6);
        if v.is_empty() {
            continue;
        }
        let k = 1 + r.range(v.len());
        let mut sorted = v.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(kth_largest_heap(&v, k), Some(sorted[k - 1]));
        let mut w = v.clone();
        assert_eq!(quickselect(&mut w, v.len() - k, &mut rng), sorted[k - 1]);
    }
    let mut same = vec![7; 100_000]; // all equal: 3-way partition finishes in one pass
    assert_eq!(quickselect(&mut same, 500, &mut rng), 7);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. Top k frequent elements: [1,1,1,2,2,3], k=2 -> [1,2] (any order). Better than O(n log n). Approaches: (1) count with a HashMap, keep a
// MIN-heap of (count, value) of size k: O(n log k). (2) BUCKET SORT by frequency: bucket[f] = values appearing f times (f <= n), scan from
// high to low: O(n) time and space. Rust wrinkle: `*map.entry(x).or_insert(0) += 1` counting idiom; the heap holds `Reverse((count, x))` so
// the smallest count is evicted; tuples order lexicographically, which also makes ties deterministic. Mistakes: sorting the whole map,
// buckets of size n instead of n+1. Follow-ups: streaming top-k (count-min sketch), top k frequent words (ties by lexicographic order).
// ---------------------------------------------------------------------------------------------------------------------
fn count_map(nums: &[i32]) -> HashMap<i32, usize> {
    let mut m = HashMap::new();
    for &x in nums {
        *m.entry(x).or_insert(0) += 1;
    }
    m
}
fn top_k_heap(nums: &[i32], k: usize) -> Vec<i32> {
    let mut h = BinaryHeap::new();
    for (x, c) in count_map(nums) {
        h.push(Reverse((c, x)));
        if h.len() > k {
            h.pop();
        }
    }
    h.into_iter().map(|Reverse((_, x))| x).collect()
}
fn top_k_bucket(nums: &[i32], k: usize) -> Vec<i32> {
    let mut buckets: Vec<Vec<i32>> = vec![vec![]; nums.len() + 1];
    for (x, c) in count_map(nums) {
        buckets[c].push(x);
    }
    buckets.into_iter().rev().flatten().take(k).collect()
}
fn q15() {
    for f in [top_k_heap, top_k_bucket] {
        let mut got = f(&[1, 1, 1, 2, 2, 3], 2);
        got.sort();
        assert_eq!(got, vec![1, 2]);
        assert_eq!(f(&[1], 1), vec![1]);
    }
    let mut r = Rng(0x200F);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 30, -5, 5);
        let cm = count_map(&v);
        if cm.is_empty() {
            continue;
        }
        let k = 1 + r.range(cm.len());
        let mut want: Vec<usize> = cm.values().copied().collect();
        want.sort_unstable_by(|a, b| b.cmp(a));
        want.truncate(k);
        for f in [top_k_heap, top_k_bucket] {
            let got = f(&v, k);
            let mut freqs: Vec<usize> = got.iter().map(|x| cm[x]).collect();
            freqs.sort_unstable_by(|a, b| b.cmp(a));
            assert_eq!(
                freqs, want,
                "ties may differ, but the frequencies of the answer must be the top-k frequencies"
            );
            assert_eq!(
                got.iter().collect::<BTreeSet<_>>().len(),
                k,
                "distinct elements"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q16. MERGE K SORTED ARRAYS (the same question as "merge k sorted lists", see linked_lists_in_rust.rs Q5). n = total elements, k lists.
// (1) Merge one list into an accumulator, k times: O(n*k) comparisons. (2) HEAP of the k current heads, `Reverse` makes it a min-heap:
// O(n log k) time, O(k) space, and it STREAMS (works on iterators, files, external sort). (3) DIVIDE AND CONQUER: merge lists pairwise in
// rounds, ceil(log2 k) rounds each touching n elements: O(n log k) time, no heap, simple. (4) A TOURNAMENT / LOSER TREE keeps the k heads in a
// binary tournament so a replacement costs ONE root-to-leaf path of log2 k comparisons, not the ~2 log k of a heap sift-down: the structure
// used by real external sorters and Knuth's replacement selection (described only; the heap is what interviewers expect).
// We COUNT comparisons by wrapping the key in a type whose `Ord` increments a thread-local counter, and assert the bounds: D&C uses at most
// n*ceil(log2 k), and for many lists the one-by-one merge uses clearly more. Rust wrinkle: the heap element is `Reverse<(T, list, idx)>`: tuples
// compare lexicographically, indices break ties deterministically, and NO node pointers are needed (compare Q5 in the linked-list recipe).
// Mistakes: pushing the whole input into the heap (O(n) space); forgetting to push the next element of the list just popped; empty lists.
// ---------------------------------------------------------------------------------------------------------------------
thread_local! { static CMP: Cell<u64> = const { Cell::new(0) }; }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Counted(i32);
impl Ord for Counted {
    fn cmp(&self, o: &Self) -> Ordering {
        CMP.with(|c| c.set(c.get() + 1));
        self.0.cmp(&o.0)
    }
}
impl PartialOrd for Counted {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
fn merge_two<T: Ord + Copy>(a: &[T], b: &[T]) -> Vec<T> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() && j < b.len() {
        if a[i] <= b[j] {
            out.push(a[i]);
            i += 1;
        } else {
            out.push(b[j]);
            j += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}
fn merge_one_by_one<T: Ord + Copy>(lists: &[Vec<T>]) -> Vec<T> {
    lists.iter().fold(Vec::new(), |acc, l| merge_two(&acc, l))
}
fn merge_dc<T: Ord + Copy>(lists: &[Vec<T>]) -> Vec<T> {
    let mut cur: Vec<Vec<T>> = lists.to_vec();
    if cur.is_empty() {
        return vec![];
    }
    while cur.len() > 1 {
        cur = cur
            .chunks(2)
            .map(|c| {
                if c.len() == 2 {
                    merge_two(&c[0], &c[1])
                } else {
                    c[0].clone()
                }
            })
            .collect();
    }
    cur.pop().unwrap()
}
fn merge_heap<T: Ord + Copy>(lists: &[Vec<T>]) -> Vec<T> {
    let mut h = BinaryHeap::new();
    for (li, l) in lists.iter().enumerate() {
        if let Some(&x) = l.first() {
            h.push(Reverse((x, li, 0usize)));
        }
    }
    let mut out = Vec::new();
    while let Some(Reverse((x, li, ix))) = h.pop() {
        out.push(x);
        if let Some(&nx) = lists[li].get(ix + 1) {
            h.push(Reverse((nx, li, ix + 1)));
        }
    }
    out
}
fn count_cmps<R>(f: impl FnOnce() -> R) -> (R, u64) {
    CMP.with(|c| c.set(0));
    let r = f();
    (r, CMP.with(|c| c.get()))
}
fn q16() {
    let mut r = Rng(0x2010);
    for round in 0..2000 {
        let k = if round % 10 == 0 {
            16 + r.range(48)
        } else {
            r.range(9)
        };
        let lists: Vec<Vec<Counted>> = (0..k)
            .map(|_| {
                let mut v = rand_vec(&mut r, if k >= 16 { 40 } else { 8 }, -50, 50);
                v.sort();
                v.into_iter().map(Counted).collect()
            })
            .collect();
        let mut want: Vec<Counted> = lists.concat();
        want.sort();
        let ((a, ca), (b, cb), (c, cc)) = (
            count_cmps(|| merge_one_by_one(&lists)),
            count_cmps(|| merge_dc(&lists)),
            count_cmps(|| merge_heap(&lists)),
        );
        assert_eq!((&a, &b, &c), (&want, &want, &want));
        let n = want.len() as u64;
        let rounds = if k <= 1 {
            0
        } else {
            (usize::BITS - (k - 1).leading_zeros()) as u64
        };
        assert!(
            cb <= n * rounds.max(1),
            "divide and conquer: at most n*ceil(log2 k) comparisons ({cb} vs n={n}, rounds={rounds})"
        );
        assert!(
            cc <= 3 * n * (rounds + 1) + k as u64 * 2,
            "heap: O(n log k)"
        );
        if k >= 32 && n > 400 {
            assert!(
                ca > cb && ca > cc,
                "one-by-one merging does Theta(n*k) work: {ca} vs {cb}/{cc}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q17. (a) Median of a data stream: add_num(x) and find_median() interleaved, 1e5 calls. Two heaps: a MAX-heap `lo` of the smaller half and a
// MIN-heap `hi` of the larger half, sizes equal or `lo` has one more. add: push to lo, move lo's max to hi, then rebalance so lo.len() >=
// hi.len(). median = lo.top (odd) or the average of both tops (even). add O(log n), median O(1), O(n) space. (b) Sliding window median: keep
// the window in a SORTED Vec (binary search to insert/remove, O(k) shifting but a tiny constant) or two heaps with lazy deletion (O(log k)
// amortised, fiddly); the sorted-Vec version is what to write in an interview unless k is huge. Rust wrinkle: `BinaryHeap<i32>` is the
// max-heap, `BinaryHeap<Reverse<i32>>` the min-heap; `partition_point` is the binary search; the result type is f64 (exact for halves).
// Mistakes: unbalanced heaps after adding to the wrong side first; integer-division average (loses .5); lazy deletion bookkeeping errors.
// ---------------------------------------------------------------------------------------------------------------------
struct MedianFinder {
    lo: BinaryHeap<i32>,
    hi: BinaryHeap<Reverse<i32>>,
}
impl MedianFinder {
    fn new() -> Self {
        MedianFinder {
            lo: BinaryHeap::new(),
            hi: BinaryHeap::new(),
        }
    }
    fn add_num(&mut self, x: i32) {
        self.lo.push(x);
        let m = self.lo.pop().unwrap();
        self.hi.push(Reverse(m));
        if self.hi.len() > self.lo.len() {
            self.lo.push(self.hi.pop().unwrap().0);
        }
    }
    fn median(&self) -> Option<f64> {
        let l = *self.lo.peek()? as f64;
        Some(if self.lo.len() > self.hi.len() {
            l
        } else {
            (l + self.hi.peek().unwrap().0 as f64) / 2.0
        })
    }
}
fn sliding_median(v: &[i32], k: usize) -> Vec<f64> {
    let mut win: Vec<i32> = Vec::with_capacity(k);
    let mut out = vec![];
    for i in 0..v.len() {
        let p = win.partition_point(|&x| x < v[i]);
        win.insert(p, v[i]);
        if i >= k {
            let q = win.partition_point(|&x| x < v[i - k]);
            win.remove(q);
        }
        if i + 1 >= k {
            out.push(if k % 2 == 1 {
                win[k / 2] as f64
            } else {
                (win[k / 2 - 1] as f64 + win[k / 2] as f64) / 2.0
            });
        }
    }
    out
}
fn median_of(sorted: &[i32]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2] as f64
    } else {
        (sorted[n / 2 - 1] as f64 + sorted[n / 2] as f64) / 2.0
    }
}
fn q17() {
    let mut m = MedianFinder::new();
    assert_eq!(m.median(), None);
    m.add_num(1);
    m.add_num(2);
    assert_eq!(m.median(), Some(1.5));
    m.add_num(3);
    assert_eq!(m.median(), Some(2.0));
    let mut r = Rng(0x2011);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 25, -9, 9);
        let (mut mf, mut seen) = (MedianFinder::new(), vec![]);
        for &x in &v {
            mf.add_num(x);
            seen.push(x);
            let mut s = seen.clone();
            s.sort();
            assert_eq!(mf.median(), Some(median_of(&s)));
        }
        if v.is_empty() {
            continue;
        }
        let k = 1 + r.range(v.len());
        let want: Vec<f64> = v
            .windows(k)
            .map(|w| {
                let mut s = w.to_vec();
                s.sort();
                median_of(&s)
            })
            .collect();
        assert_eq!(sliding_median(&v, k), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q18. (a) Task scheduler: tasks 'A'..'Z', each takes 1 unit, the SAME task needs at least n units between runs; idle slots allowed; return the
// least total time. ["A","A","A","B","B","B"], n=2 -> 8 (A B _ A B _ A B). Greedy with a max-heap of remaining counts and a cooldown queue:
// each tick run the most frequent ready task; it re-enters the heap after n+1 ticks. O(T log 26). (A closed form also exists:
// max(len, (maxfreq-1)*(n+1) + number_of_tasks_with_maxfreq), used as the oracle.) (b) Reorganize string: rearrange so no two adjacent
// characters are equal or return None: always place the most frequent character that is not the previous one: pop the top, hold it
// out for one step, push the held one back. Possible iff maxfreq <= (len+1)/2. Rust wrinkle: `BinaryHeap<(usize, char)>` orders by count
// then by char, no custom Ord; the held-out element is an `Option` taken with `prev.take()`. Mistakes: re-inserting the same char
// immediately; forgetting idle time when the heap is empty but the cooldown queue is not.
// ---------------------------------------------------------------------------------------------------------------------
fn least_interval(tasks: &[u8], n: usize) -> usize {
    let mut cnt = [0usize; 26];
    for &t in tasks {
        cnt[(t - b'A') as usize] += 1;
    }
    let mut heap: BinaryHeap<usize> = cnt.iter().copied().filter(|&c| c > 0).collect();
    let mut cool: VecDeque<(usize, usize)> = VecDeque::new(); // (ready_at, remaining)
    let mut time = 0;
    while !heap.is_empty() || !cool.is_empty() {
        time += 1;
        if cool.front().is_some_and(|&(t, _)| t <= time) {
            heap.push(cool.pop_front().unwrap().1);
        }
        if let Some(c) = heap.pop() {
            if c > 1 {
                cool.push_back((time + n + 1, c - 1));
            }
        }
    }
    time
}
fn least_interval_formula(tasks: &[u8], n: usize) -> usize {
    let mut cnt = [0usize; 26];
    for &t in tasks {
        cnt[(t - b'A') as usize] += 1;
    }
    let mx = *cnt.iter().max().unwrap();
    let nmax = cnt.iter().filter(|&&c| c == mx).count();
    tasks.len().max((mx.max(1) - 1) * (n + 1) + nmax)
}
fn reorganize(s: &str) -> Option<String> {
    let mut cnt: HashMap<char, usize> = HashMap::new();
    for c in s.chars() {
        *cnt.entry(c).or_insert(0) += 1;
    }
    let mut heap: BinaryHeap<(usize, char)> = cnt.into_iter().map(|(c, n)| (n, c)).collect();
    let (mut out, mut held) = (String::new(), None::<(usize, char)>);
    while let Some((c, ch)) = heap.pop() {
        out.push(ch);
        if let Some(h) = held.take() {
            heap.push(h);
        }
        if c > 1 {
            held = Some((c - 1, ch));
        }
    }
    if out.len() == s.len() {
        Some(out)
    } else {
        None
    }
}
fn q18() {
    assert_eq!(least_interval(b"AAABBB", 2), 8);
    assert_eq!(least_interval(b"AAABBB", 0), 6);
    assert_eq!(least_interval(b"AAAAAABCDEFG", 2), 16);
    assert_eq!(reorganize("aab").map(|s| s.len()), Some(3));
    assert_eq!(reorganize("aaab"), None);
    let mut r = Rng(0x2012);
    let (mut some, mut none) = (0, 0);
    for _ in 0..2000 {
        let len = 1 + r.range(10);
        let alpha = 1 + r.range(5);
        let tasks: Vec<u8> = (0..len).map(|_| b'A' + r.range(alpha) as u8).collect();
        let n = r.range(5);
        assert_eq!(least_interval(&tasks, n), least_interval_formula(&tasks, n));
        let s = String::from_utf8(tasks.iter().map(|b| b + 32).collect()).unwrap();
        let maxf = (b'a'..=b'z')
            .map(|c| s.bytes().filter(|&b| b == c).count())
            .max()
            .unwrap();
        match reorganize(&s) {
            Some(o) => {
                some += 1;
                assert!(maxf <= (len + 1) / 2);
                assert!(o.as_bytes().windows(2).all(|w| w[0] != w[1]));
                let (mut a, mut b): (Vec<u8>, Vec<u8>) = (o.into_bytes(), s.into_bytes());
                a.sort();
                b.sort();
                assert_eq!(a, b, "same letters");
            }
            None => {
                none += 1;
                assert!(maxf > (len + 1) / 2);
            }
        }
    }
    assert!(some > 100 && none > 100);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q19. Meeting rooms II: given intervals [start, end), the minimum number of rooms so no two meetings in one room overlap
// ([[0,30],[5,10],[15,20]] -> 2; [[7,10],[2,4]] -> 1). Approach: sort by start; a MIN-heap of end times of rooms in use; for each meeting,
// if the earliest end is <= start reuse that room (pop), then push this end. The heap size at the end (or its max) is the answer.
// O(n log n) time, O(n) space. Alternative: sort starts and ends separately and sweep (two pointers), or +1/-1 events. Rust wrinkle:
// `BinaryHeap<Reverse<i32>>` and `peek()` returning `Option<&Reverse<i32>>`, compared via `.0`; `sort_unstable_by_key` on tuples.
// Mistakes: `<` vs `<=` (a meeting ending at 5 and another starting at 5 may share), forgetting to sort. Follow-ups: meeting rooms I (any overlap?),
// return the assignment of meetings to rooms, car pooling / minimum platforms (same problem).
// ---------------------------------------------------------------------------------------------------------------------
fn min_rooms(iv: &[(i32, i32)]) -> usize {
    let mut v = iv.to_vec();
    v.sort_unstable();
    let mut ends: BinaryHeap<Reverse<i32>> = BinaryHeap::new();
    for (s, e) in v {
        if ends.peek().is_some_and(|&Reverse(t)| t <= s) {
            ends.pop();
        }
        ends.push(Reverse(e));
    }
    ends.len()
}
fn q19() {
    assert_eq!(min_rooms(&[(0, 30), (5, 10), (15, 20)]), 2);
    assert_eq!(min_rooms(&[(7, 10), (2, 4)]), 1);
    assert_eq!(min_rooms(&[]), 0);
    assert_eq!(
        min_rooms(&[(1, 5), (5, 9)]),
        1,
        "touching intervals share a room"
    );
    let mut r = Rng(0x2013);
    for _ in 0..2000 {
        let iv: Vec<(i32, i32)> = (0..r.range(12))
            .map(|_| {
                let s = r.int(0, 20);
                (s, s + r.int(1, 8))
            })
            .collect();
        let want = (0..30)
            .map(|t| iv.iter().filter(|&&(s, e)| s <= t && t < e).count())
            .max()
            .unwrap();
        assert_eq!(min_rooms(&iv), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q20. K closest points to the origin: [[1,3],[-2,2]], k=1 -> [[-2,2]]. Approaches: sort by distance O(n log n); MAX-heap of size k keyed by
// distance (pop the farthest when the size exceeds k): O(n log k), O(k) space; quickselect O(n) average. Compare SQUARED distances (no sqrt,
// no floating-point error): use i64 because (x*x + y*y) overflows i32 near 1e5. Rust wrinkle: the heap stores `(i64, usize)` (distance,
// index) so tuples order for free and we avoid cloning points; had we used f64 distances we could not put them in a heap (f64 is not Ord, see Q24).
// Mistakes: min-heap of everything (O(n log n)), sqrt rounding ties, i32 overflow. Follow-up: k closest to an arbitrary point; streaming.
// ---------------------------------------------------------------------------------------------------------------------
fn k_closest(points: &[(i32, i32)], k: usize) -> Vec<(i32, i32)> {
    let d = |p: (i32, i32)| (p.0 as i64).pow(2) + (p.1 as i64).pow(2);
    let mut h: BinaryHeap<(i64, usize)> = BinaryHeap::new();
    for (i, &p) in points.iter().enumerate() {
        h.push((d(p), i));
        if h.len() > k {
            h.pop();
        }
    }
    h.into_iter().map(|(_, i)| points[i]).collect()
}
fn q20() {
    assert_eq!(k_closest(&[(1, 3), (-2, 2)], 1), vec![(-2, 2)]);
    assert_eq!(k_closest(&[(100_000, 100_000), (0, 1)], 1), vec![(0, 1)]);
    let mut r = Rng(0x2014);
    for _ in 0..2000 {
        let pts: Vec<(i32, i32)> = (0..r.range(15))
            .map(|_| (r.int(-6, 6), r.int(-6, 6)))
            .collect();
        let k = r.range(pts.len() + 1);
        let d = |p: &(i32, i32)| (p.0 * p.0 + p.1 * p.1) as i64;
        let mut want: Vec<i64> = pts.iter().map(d).collect();
        want.sort();
        want.truncate(k);
        let mut got: Vec<i64> = k_closest(&pts, k).iter().map(d).collect();
        got.sort();
        assert_eq!(got, want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q21. Connect ropes: lengths [4,3,2,6]; joining two ropes costs the sum of their lengths; minimise the total cost to get one rope (= 29).
// Greedy (Huffman coding): always join the two SHORTEST ropes, since every merge cost is paid again by every later merge containing it, so
// small ropes should be merged early (deep in the merge tree). Min-heap: pop two, push the sum, add the sum to the cost. O(n log n).
// Proof sketch: exchange argument; the optimal merge tree is Huffman's tree. Rust wrinkle: `BinaryHeap<Reverse<u64>>` from `into_iter().map(Reverse).collect()`
// (O(n) heapify); `while h.len() > 1`, then two `pop().unwrap()`. Mistakes: max-heap, forgetting that sums go BACK into the heap, u32 overflow.
// Verified against an exhaustive search over every merge order for up to 6 ropes. Follow-ups: Huffman codes themselves; k-ary merge.
// ---------------------------------------------------------------------------------------------------------------------
fn min_cost_connect(ropes: &[u64]) -> u64 {
    let mut h: BinaryHeap<Reverse<u64>> = ropes.iter().copied().map(Reverse).collect();
    let mut cost = 0;
    while h.len() > 1 {
        let (Reverse(a), Reverse(b)) = (h.pop().unwrap(), h.pop().unwrap());
        cost += a + b;
        h.push(Reverse(a + b));
    }
    cost
}
fn min_cost_brute(ropes: Vec<u64>) -> u64 {
    if ropes.len() <= 1 {
        return 0;
    }
    let mut best = u64::MAX;
    for i in 0..ropes.len() {
        for j in i + 1..ropes.len() {
            let mut rest: Vec<u64> = ropes
                .iter()
                .enumerate()
                .filter(|&(k, _)| k != i && k != j)
                .map(|(_, &x)| x)
                .collect();
            let s = ropes[i] + ropes[j];
            rest.push(s);
            best = best.min(s + min_cost_brute(rest));
        }
    }
    best
}
fn q21() {
    assert_eq!(min_cost_connect(&[4, 3, 2, 6]), 29);
    assert_eq!(min_cost_connect(&[5]), 0);
    assert_eq!(min_cost_connect(&[]), 0);
    let mut r = Rng(0x2015);
    for _ in 0..2000 {
        let v: Vec<u64> = (0..r.range(7)).map(|_| 1 + r.range(9) as u64).collect();
        assert_eq!(min_cost_connect(&v), min_cost_brute(v.clone()));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q22. Smallest range covering at least one number from each of k sorted lists: [[4,10,15,24,26],[0,9,12,20],[5,18,22,30]] -> [20,24].
// "Smaller" = smaller b-a, ties broken by smaller a. Approach: keep one pointer per list in a MIN-heap; track the current maximum of the
// pointers. The range [heap min, current max] covers all lists; record it, then advance the list that supplied the minimum (the only way
// to shrink the range from the left); stop when that list is exhausted. O(n log k) time, O(k) space. Rust wrinkle: heap of
// `Reverse<(value, list, index)>`; no struct needed because tuples compare lexicographically. Mistakes: recomputing the max each step (O(k)),
// not stopping when one list ends, ties (use strict `<` to keep the smaller start). Related: merge k sorted (Q16) is the same skeleton.
// ---------------------------------------------------------------------------------------------------------------------
fn smallest_range(lists: &[Vec<i32>]) -> (i32, i32) {
    let mut h = BinaryHeap::new();
    let mut cur_max = i32::MIN;
    for (li, l) in lists.iter().enumerate() {
        h.push(Reverse((l[0], li, 0usize)));
        cur_max = cur_max.max(l[0]);
    }
    let mut best = (h.peek().unwrap().0.0, cur_max);
    while let Some(Reverse((v, li, ix))) = h.pop() {
        if (cur_max as i64 - v as i64) < (best.1 as i64 - best.0 as i64) {
            best = (v, cur_max);
        }
        match lists[li].get(ix + 1) {
            None => break,
            Some(&nv) => {
                cur_max = cur_max.max(nv);
                h.push(Reverse((nv, li, ix + 1)));
            }
        }
    }
    best
}
fn q22() {
    let l = vec![
        vec![4, 10, 15, 24, 26],
        vec![0, 9, 12, 20],
        vec![5, 18, 22, 30],
    ];
    assert_eq!(smallest_range(&l), (20, 24));
    assert_eq!(
        smallest_range(&[vec![1, 2, 3], vec![1, 2, 3], vec![1, 2, 3]]),
        (1, 1)
    );
    let mut r = Rng(0x2016);
    for _ in 0..2000 {
        let k = 1 + r.range(4);
        let lists: Vec<Vec<i32>> = (0..k)
            .map(|_| {
                let mut v = rand_vec(&mut r, 5, -10, 10);
                v.push(r.int(-10, 10));
                v.sort();
                v
            })
            .collect();
        let vals: Vec<i32> = lists.concat();
        let mut best: Option<(i32, i32)> = None;
        for &a in &vals {
            for &b in &vals {
                if a <= b && lists.iter().all(|l| l.iter().any(|&x| a <= x && x <= b)) {
                    let better = best.is_none_or(|(ba, bb)| (b - a, a) < (bb - ba, ba));
                    if better {
                        best = Some((a, b));
                    }
                }
            }
        }
        assert_eq!(Some(smallest_range(&lists)), best);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q23. Implement a binary heap. Array layout: children of i are 2i+1 and 2i+2, parent (i-1)/2. push = append then SIFT UP; pop = swap root
// with last, remove, SIFT DOWN. Both O(log n). HEAPIFY (build from an array): sift down every internal node from n/2-1 to 0: O(n), not
// O(n log n), because most nodes are near the leaves and sift only a few levels (sum of heights = O(n)). Part 2, DECREASE-KEY (Dijkstra,
// Prim, schedulers): an INDEXED heap stores item ids and a `pos[id]` table so an item can be found in O(1) and sifted up in O(log n); std's
// BinaryHeap has no decrease-key (the usual workaround is pushing duplicates and skipping stale entries, as Dijkstra does).
// Rust wrinkle: `Vec::swap` does every exchange (no temp variable, no unsafe); the generic `T: Ord` min-heap needs no clones; `pos` is
// `Vec<usize>` with `NIL` for "absent". Mistakes: wrong child index in sift down (pick the SMALLER child), not updating `pos` on EVERY swap.
// Verified against std::collections::BinaryHeap and a BTreeSet model of (key, id) pairs.
// ---------------------------------------------------------------------------------------------------------------------
struct MinHeap<T: Ord> {
    a: Vec<T>,
}
impl<T: Ord> MinHeap<T> {
    fn new() -> Self {
        MinHeap { a: Vec::new() }
    }
    fn from_vec(a: Vec<T>) -> Self {
        let mut h = MinHeap { a };
        for i in (0..h.a.len() / 2).rev() {
            h.sift_down(i);
        }
        h
    }
    fn push(&mut self, x: T) {
        self.a.push(x);
        self.sift_up(self.a.len() - 1);
    }
    fn pop(&mut self) -> Option<T> {
        if self.a.is_empty() {
            return None;
        }
        let last = self.a.len() - 1;
        self.a.swap(0, last);
        let x = self.a.pop();
        self.sift_down(0);
        x
    }
    fn peek(&self) -> Option<&T> {
        self.a.first()
    }
    fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let p = (i - 1) / 2;
            if self.a[i] < self.a[p] {
                self.a.swap(i, p);
                i = p;
            } else {
                break;
            }
        }
    }
    fn sift_down(&mut self, mut i: usize) {
        let n = self.a.len();
        loop {
            let l = 2 * i + 1;
            if l >= n {
                break;
            }
            let m = if l + 1 < n && self.a[l + 1] < self.a[l] {
                l + 1
            } else {
                l
            };
            if self.a[m] < self.a[i] {
                self.a.swap(i, m);
                i = m;
            } else {
                break;
            }
        }
    }
}
const NIL: usize = usize::MAX;
struct IndexedHeap {
    heap: Vec<usize>, // item ids in heap order
    pos: Vec<usize>,  // id -> index in `heap`, NIL if absent
    key: Vec<i64>,
}
impl IndexedHeap {
    fn new(n: usize) -> Self {
        IndexedHeap {
            heap: vec![],
            pos: vec![NIL; n],
            key: vec![0; n],
        }
    }
    fn contains(&self, id: usize) -> bool {
        self.pos[id] != NIL
    }
    fn swap(&mut self, i: usize, j: usize) {
        self.heap.swap(i, j);
        self.pos[self.heap[i]] = i;
        self.pos[self.heap[j]] = j;
    }
    fn less(&self, i: usize, j: usize) -> bool {
        (self.key[self.heap[i]], self.heap[i]) < (self.key[self.heap[j]], self.heap[j])
    }
    fn push(&mut self, id: usize, key: i64) {
        assert!(!self.contains(id));
        self.key[id] = key;
        self.heap.push(id);
        let i = self.heap.len() - 1;
        self.pos[id] = i;
        self.up(i);
    }
    fn decrease_key(&mut self, id: usize, key: i64) {
        assert!(self.contains(id) && key <= self.key[id]);
        self.key[id] = key;
        self.up(self.pos[id]);
    }
    fn pop_min(&mut self) -> Option<(usize, i64)> {
        let id = *self.heap.first()?;
        let last = self.heap.len() - 1;
        self.swap(0, last);
        self.heap.pop();
        self.pos[id] = NIL;
        self.down(0);
        Some((id, self.key[id]))
    }
    fn up(&mut self, mut i: usize) {
        while i > 0 && self.less(i, (i - 1) / 2) {
            self.swap(i, (i - 1) / 2);
            i = (i - 1) / 2;
        }
    }
    fn down(&mut self, mut i: usize) {
        let n = self.heap.len();
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut m = i;
            if l < n && self.less(l, m) {
                m = l;
            }
            if r < n && self.less(r, m) {
                m = r;
            }
            if m == i {
                break;
            }
            self.swap(i, m);
            i = m;
        }
    }
}
fn q23() {
    let mut r = Rng(0x2017);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 30, -20, 20);
        let mut sorted = v.clone();
        sorted.sort();
        let mut h = MinHeap::from_vec(v.clone());
        let mut g = MinHeap::new();
        for &x in &v {
            g.push(x);
        }
        let mut std_heap: BinaryHeap<Reverse<i32>> = v.iter().copied().map(Reverse).collect();
        for &want in &sorted {
            assert_eq!(h.peek(), Some(&want));
            assert_eq!(h.pop(), Some(want));
            assert_eq!(g.pop(), Some(want));
            assert_eq!(std_heap.pop().map(|x| x.0), Some(want));
        }
        assert_eq!((h.pop(), g.pop()), (None, None));
    }
    for _ in 0..300 {
        let n = 12;
        let (mut ih, mut model): (IndexedHeap, BTreeSet<(i64, usize)>) =
            (IndexedHeap::new(n), BTreeSet::new());
        let mut keys = vec![0i64; n];
        for _ in 0..200 {
            let id = r.range(n);
            match r.range(3) {
                0 if !ih.contains(id) => {
                    let k = r.int(0, 50) as i64;
                    ih.push(id, k);
                    model.insert((k, id));
                    keys[id] = k;
                }
                1 if ih.contains(id) => {
                    let k = keys[id] - r.int(0, 10) as i64;
                    ih.decrease_key(id, k);
                    model.remove(&(keys[id], id));
                    model.insert((k, id));
                    keys[id] = k;
                }
                _ => {
                    let want = model.pop_first().map(|(k, i)| (i, k));
                    assert_eq!(ih.pop_min(), want);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q24. Rust wrinkles of BinaryHeap (all three come up in interviews):
//  (1) It is a MAX-heap. For a min-heap wrap items in `std::cmp::Reverse` (`Reverse(3) < Reverse(2)`), read with `.0`; tuples inside Reverse
//      keep lexicographic order, so `Reverse((dist, node))` is the Dijkstra queue.
//  (2) Custom ordering: implement `Ord` (and `PartialOrd` by delegating to it, plus Eq/PartialEq consistent with it). Reverse the comparison
//      to turn a max-heap into a min-heap for that struct; break ties with a field so the order is total and deterministic.
//  (3) `f64` is NOT `Ord` (NaN breaks totality: `f64::NAN.partial_cmp(&1.0)` is None), so `BinaryHeap<f64>` does not compile. Use a wrapper
//      that orders by `f64::total_cmp` (IEEE total order: -NaN < -inf < ... < -0.0 < +0.0 < ... < +inf < +NaN), or keep integers/scaled values.
// An inconsistent Ord does not cause unsafety, but the heap order becomes unspecified: bugs, not UB. Interviewer is testing: do you know
// why the compiler refuses `BinaryHeap<f64>` and the fix. Mistakes: `partial_cmp(..).unwrap()` in a comparator (panics on NaN).
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Eq)]
struct Task {
    priority: u32,
    name: String,
}
impl Ord for Task {
    fn cmp(&self, o: &Self) -> Ordering {
        self.priority
            .cmp(&o.priority)
            .then_with(|| o.name.cmp(&self.name)) // higher priority first; ties: alphabetical first
    }
}
impl PartialOrd for Task {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
#[derive(Debug, Clone, Copy)]
struct OrdF64(f64);
impl PartialEq for OrdF64 {
    fn eq(&self, o: &Self) -> bool {
        self.0.total_cmp(&o.0) == Ordering::Equal
    }
}
impl Eq for OrdF64 {}
impl Ord for OrdF64 {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}
impl PartialOrd for OrdF64 {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
fn q24() {
    // (1) max-heap and Reverse
    let mut h: BinaryHeap<i32> = [3, 1, 4, 1, 5].into_iter().collect();
    assert_eq!(h.pop(), Some(5));
    let mut m: BinaryHeap<Reverse<i32>> = [3, 1, 4, 1, 5].into_iter().map(Reverse).collect();
    assert_eq!(m.pop(), Some(Reverse(1)));
    assert!(Reverse(3) < Reverse(2));
    // (2) custom Ord
    let mut tasks: BinaryHeap<Task> = BinaryHeap::new();
    let mut r = Rng(0x2018);
    let mut model = vec![];
    for i in 0..200 {
        let t = Task {
            priority: r.range(5) as u32,
            name: format!("t{:03}", r.range(300) + i),
        };
        model.push((t.priority, t.name.clone()));
        tasks.push(t);
    }
    model.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let popped: Vec<(u32, String)> = std::iter::from_fn(|| tasks.pop())
        .map(|t| (t.priority, t.name))
        .collect();
    assert_eq!(popped, model);
    // (3) f64
    assert_eq!(
        f64::NAN.partial_cmp(&1.0),
        None,
        "NaN is unordered: f64 cannot be Ord"
    );
    let mut fh: BinaryHeap<OrdF64> = BinaryHeap::new();
    let vals = [2.5, -1.0, f64::INFINITY, 0.0, -0.0, f64::NAN, 1e-9, -3.25];
    for v in vals {
        fh.push(OrdF64(v));
    }
    let mut want: Vec<f64> = vals.to_vec();
    want.sort_by(|a, b| b.total_cmp(a));
    let got: Vec<f64> = std::iter::from_fn(|| fh.pop()).map(|x| x.0).collect();
    assert!(got[0].is_nan() && got[1] == f64::INFINITY);
    assert_eq!(
        got.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        want.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
    let mut plain = vec![2.5, -1.0, 0.5];
    plain.sort_by(f64::total_cmp); // Vec<f64> sorts fine with a comparator even though f64 is not Ord
    assert_eq!(plain, vec![-1.0, 0.5, 2.5]);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q25. (a) `BinaryHeap::peek_mut()` returns `Option<PeekMut>`, a guard giving `&mut` to the top element; when the guard DROPS the heap sifts
// the (possibly changed) top down. That makes "replace the top" one O(log k) operation instead of pop + push (two). `PeekMut::pop(guard)`
// removes the top through the guard (e.g. pop while a condition holds). Typical use: keep the k smallest items: a MAX-heap of size k whose top
// is the worst of the best; a new item smaller than the top overwrites it through peek_mut. (b) Queue type: `Vec::remove(0)` shifts every
// element (O(n)) so a Vec used as a queue makes BFS O(n^2); `VecDeque` is a ring buffer with O(1) push/pop at both ends and indexing. Cost:
// storage may wrap, so it is not one contiguous slice (use `make_contiguous` or `as_slices`). Mistakes: holding the PeekMut guard while
// touching the heap again (borrow error, by design); expecting the change to be visible in heap order before the guard drops.
// ---------------------------------------------------------------------------------------------------------------------
fn k_smallest(v: &[i32], k: usize) -> Vec<i32> {
    let mut h: BinaryHeap<i32> = BinaryHeap::with_capacity(k);
    for &x in v {
        if h.len() < k {
            h.push(x);
        } else if let Some(mut top) = h.peek_mut() {
            if x < *top {
                *top = x; // one sift-down when the guard drops, instead of pop + push
            }
        }
    }
    h.into_sorted_vec()
}
fn q25() {
    let mut r = Rng(0x2019);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 25, -10, 10);
        let k = r.range(8);
        let mut s = v.clone();
        s.sort();
        s.truncate(k);
        assert_eq!(k_smallest(&v, k), s);
    }
    let mut h: BinaryHeap<i32> = (1..=10).collect();
    while let Some(top) = h.peek_mut() {
        if *top > 6 {
            PeekMut::pop(top); // remove the top through the guard
        } else {
            break;
        }
    }
    assert_eq!(h.into_sorted_vec(), vec![1, 2, 3, 4, 5, 6]);
    // VecDeque vs Vec as a queue: identical behaviour, very different cost (remove(0) shifts everything)
    let (mut dq, mut v): (VecDeque<u32>, Vec<u32>) = (VecDeque::new(), Vec::new());
    let mut shifted = 0usize;
    for i in 0..3000u32 {
        dq.push_back(i);
        v.push(i);
        if i % 3 == 0 {
            shifted += v.len() - 1; // elements moved by remove(0)
            assert_eq!(dq.pop_front(), Some(v.remove(0)));
        }
    }
    assert!(dq.iter().eq(v.iter()));
    assert!(
        shifted > 500_000,
        "Vec::remove(0) moved {shifted} elements; pop_front moved none"
    );
    let (a, b) = dq.as_slices();
    assert_eq!(a.len() + b.len(), dq.len());
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
}
