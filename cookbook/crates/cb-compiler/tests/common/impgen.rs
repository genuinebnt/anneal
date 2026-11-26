//! A generator of small *imperative* programs: mutable variables, arrays,
//! struct fields, nested loops with break/continue, matches and calls. It
//! exercises what the type-directed expression generator does not (stores,
//! loops, aliasing), so it is the main stress test for the optimiser.
#![allow(dead_code)]

use super::Rng;

pub struct ImpGen {
    pub rng: Rng,
    out: String,
    loops: usize,
    next_loop_var: usize,
}

impl ImpGen {
    pub fn new(seed: u64) -> ImpGen {
        ImpGen {
            rng: Rng::new(seed ^ 0xABCDEF),
            out: String::new(),
            loops: 0,
            next_loop_var: 0,
        }
    }

    fn var(&mut self) -> String {
        format!("v{}", self.rng.below(4))
    }

    fn safe_index(&mut self, depth: u32) -> String {
        format!("(({} % 5 + 5) % 5)", self.int(depth))
    }

    fn int(&mut self, depth: u32) -> String {
        let leaf = depth == 0 || self.rng.below(3) == 0;
        if leaf {
            return match self.rng.below(6) {
                0 => self.rng.below(50).to_string(),
                1 | 2 => self.var(),
                3 => "p.a".into(),
                4 => "len(xs)".into(),
                _ => "p.b".into(),
            };
        }
        let d = depth - 1;
        match self.rng.below(9) {
            0 => format!("({} + {})", self.int(d), self.int(d)),
            1 => format!("({} - {})", self.int(d), self.int(d)),
            2 => format!("({} * {} % 997)", self.int(d), self.int(d)),
            3 => format!("xs[{}]", self.safe_index(d)),
            4 => format!("h({}, {})", self.int(d), self.int(d)),
            5 => format!("({} / {})", self.int(d), 1 + self.rng.below(9)),
            6 => format!(
                "if {} {{ {} }} else {{ {} }}",
                self.boolean(d),
                self.int(d),
                self.int(d)
            ),
            7 => format!("(({}) % 100000)", self.int(d)),
            _ => format!("({} as int)", self.boolean(d)),
        }
    }

    fn boolean(&mut self, depth: u32) -> String {
        let d = depth.saturating_sub(1);
        match self.rng.below(5) {
            0 => format!("({} < {})", self.int(d), self.int(d)),
            1 => format!("({} == {})", self.int(d), self.int(d)),
            2 => format!("({} >= {})", self.int(d), self.int(d)),
            3 => format!("({} && {})", self.boolean(d), self.boolean(d)),
            _ => format!("({} || {})", self.boolean(d), self.boolean(d)),
        }
    }

    fn line(&mut self, indent: usize, s: String) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(&s);
        self.out.push('\n');
    }

    fn stmts(&mut self, indent: usize, depth: u32, n: usize, in_loop: bool) {
        for _ in 0..n {
            self.stmt(indent, depth, in_loop);
        }
    }

    fn stmt(&mut self, indent: usize, depth: u32, in_loop: bool) {
        match self.rng.below(if depth == 0 { 9 } else { 14 }) {
            0 | 1 => {
                let (v, e) = (self.var(), self.int(2));
                self.line(indent, format!("{v} = {e};"));
            }
            2 => {
                let (v, e) = (self.var(), self.int(2));
                self.line(indent, format!("{v} = ({v} + {e}) % 100003;"));
            }
            3 => {
                let (i, e) = (self.safe_index(1), self.int(2));
                self.line(indent, format!("xs[{i}] = {e};"));
            }
            4 => {
                let (i, e) = (self.safe_index(1), self.int(1));
                self.line(indent, format!("xs[{i}] += {e};"));
            }
            5 => {
                let e = self.int(2);
                let f = ["a", "b"][self.rng.below(2)];
                self.line(indent, format!("p.{f} = {e};"));
            }
            6 => {
                let e = self.int(2);
                self.line(indent, format!("println(to_str({e}));"));
            }
            7 => {
                let e = self.int(1);
                self.line(indent, format!("push(xs, {e});"));
            }
            8 => self.line(indent, "if len(xs) > 1 { pop(xs); }".to_string()),
            9 | 10 => {
                let c = self.boolean(2);
                self.line(indent, format!("if {c} {{"));
                let k = 1 + self.rng.below(2);
                self.stmts(indent + 1, depth - 1, k, in_loop);
                if self.rng.below(2) == 0 {
                    self.line(indent, "} else {".to_string());
                    let k = 1 + self.rng.below(2);
                    self.stmts(indent + 1, depth - 1, k, in_loop);
                }
                self.line(indent, "}".to_string());
            }
            11 | 12 if self.loops < 3 => {
                self.loops += 1;
                let id = self.next_loop_var;
                self.next_loop_var += 1;
                let bound = 1 + self.rng.below(5);
                let kind = self.rng.below(3);
                self.line(indent, format!("let mut i{id} = 0;"));
                // Variants: constant bound, array-length bound, hoisted length.
                let cond = match kind {
                    0 => format!("i{id} < {bound}"),
                    1 => format!("i{id} < len(xs)"),
                    _ => {
                        self.line(indent, format!("let n{id} = len(xs);"));
                        format!("i{id} < n{id}")
                    }
                };
                self.line(indent, format!("while {cond} {{"));
                if kind != 0 {
                    let v = self.rng.below(4);
                    self.line(indent + 1, format!("v{v} += xs[i{id}];"));
                }
                let k = 1 + self.rng.below(3);
                self.stmts(indent + 1, depth - 1, k, true);
                if self.rng.below(3) == 0 {
                    let c = self.boolean(1);
                    let kw = ["break", "continue"][self.rng.below(2)];
                    self.line(indent + 1, format!("if {c} {{ i{id} += 1; {kw}; }}"));
                }
                self.line(indent + 1, format!("i{id} += 1;"));
                self.line(indent, "}".to_string());
                self.loops -= 1;
            }
            _ => {
                let (e, v) = (self.int(1), self.var());
                self.line(indent, format!("match E::B({e}) {{ E::A => {{ {v} = 1; }}, E::B(k) => {{ {v} = k % 1000; }} }}"));
            }
        }
    }

    pub fn program(&mut self) -> String {
        self.out
            .push_str("struct P { a: int, b: int }\nenum E { A, B(int) }\n");
        self.out.push_str("fn h(x: int, y: int) -> int {\n    let t = x * 3 + y;\n    if t > 1000 { t % 1000 } else { t }\n}\n");
        self.out.push_str("fn main() {\n    let mut xs = [3, 1, 4, 1, 5];\n    let mut p = P { a: 2, b: 7 };\n    let mut v0 = 1;\n    let mut v1 = 2;\n    let mut v2 = 3;\n    let mut v3 = 4;\n");
        let n = 4 + self.rng.below(6);
        self.stmts(1, 3, n, false);
        self.out.push_str("    println(to_str(v0 + v1 + v2 + v3 + p.a + p.b + len(xs)));\n    let mut k = 0;\n    while k < len(xs) { println(to_str(xs[k])); k += 1; }\n}\n");
        std::mem::take(&mut self.out)
    }
}
