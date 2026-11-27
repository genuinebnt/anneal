use solution::*;

#[allow(dead_code)]
mod build {
    use solution::*;
    pub fn n(x: i64) -> Expr {
        Expr::Number(x)
    }
    pub fn v(x: &str) -> Expr {
        Expr::Name(x.into())
    }
    pub fn add(a: Expr, b: Expr) -> Expr {
        Expr::Add(Box::new(a), Box::new(b))
    }
    pub fn sub(a: Expr, b: Expr) -> Expr {
        Expr::Sub(Box::new(a), Box::new(b))
    }
    pub fn le(x: &str, e: Expr) -> Stmt {
        Stmt::Let(x.into(), e)
    }
    pub fn set(x: &str, e: Expr) -> Stmt {
        Stmt::Assign(x.into(), e)
    }
    pub fn print(e: Expr) -> Stmt {
        Stmt::Print(e)
    }
    pub fn block(s: Vec<Stmt>) -> Stmt {
        Stmt::Block(s)
    }
    pub fn forever(s: Vec<Stmt>) -> Stmt {
        Stmt::Loop(s)
    }
    pub fn leave(e: Expr, levels: usize) -> Stmt {
        Stmt::LeaveIf(e, levels)
    }
}

/// What a run gave: the printed values, and the most slots the stack ever held.
#[allow(dead_code)]
fn device(code: &[Instr]) -> Result<(Vec<i64>, usize), String> {
    let (mut stack, mut printed, mut at, mut peak) = (Vec::new(), Vec::new(), 0, 0);
    for _ in 0..200_000 {
        if at == code.len() {
            return if stack.is_empty() {
                Ok((printed, peak))
            } else {
                Err(format!("{} values left on the stack", stack.len()))
            };
        }
        let instr = *code.get(at).ok_or("a jump goes past the end")?;
        at += 1;
        let empty = || format!("the stack is empty at {instr:?}");
        match instr {
            Instr::Push(x) => stack.push(x),
            Instr::Copy(slot) => {
                let value = *stack.get(slot).ok_or(format!("no slot {slot}"))?;
                stack.push(value);
            }
            Instr::Store(slot) => {
                let value = stack.pop().ok_or_else(empty)?;
                *stack.get_mut(slot).ok_or(format!("no slot {slot}"))? = value;
            }
            Instr::Add | Instr::Sub => {
                let b: i64 = stack.pop().ok_or_else(empty)?;
                let a: i64 = stack.pop().ok_or_else(empty)?;
                stack.push(if instr == Instr::Add {
                    a.wrapping_add(b)
                } else {
                    a.wrapping_sub(b)
                });
            }
            Instr::Print => printed.push(stack.pop().ok_or_else(empty)?),
            Instr::Pop => {
                stack.pop().ok_or_else(empty)?;
            }
            Instr::Jump(to) => at = to,
            Instr::JumpIfZero(to) => {
                if stack.pop().ok_or_else(empty)? == 0 {
                    at = to;
                }
            }
        }
        peak = peak.max(stack.len());
    }
    Err("the program does not end".into())
}

#[allow(dead_code)]
fn go(program: &[Stmt]) -> Result<Vec<i64>, String> {
    let code = compile(program).map_err(|e| format!("{e:?}"))?;
    device(&code).map(|(printed, _)| printed)
}

#[test]
fn empty_programs() {
    use build::*;
    check!("nothing", go(&[]), Ok(vec![]));
    check!("empty block", go(&[block(vec![])]), Ok(vec![]));
}

#[test]
fn every_compile_error() {
    use build::*;
    use CompileError::*;
    let unknown = |x: &str| Unknown(x.into());
    check!("own name", compile(&[le("a", v("a"))]), Err(unknown("a")));
    check!(
        "after its block",
        compile(&[block(vec![le("a", n(1))]), print(v("a"))]),
        Err(unknown("a"))
    );
    check!("assign", compile(&[set("q", n(1))]), Err(unknown("q")));
    check!(
        "value first",
        compile(&[set("q", v("r"))]),
        Err(unknown("r"))
    );
    check!(
        "before it exists",
        compile(&[print(v("a")), le("a", n(1))]),
        Err(unknown("a"))
    );
    check!(
        "twice, value first",
        compile(&[le("a", n(1)), le("a", v("zz"))]),
        Err(unknown("zz"))
    );
    check!(
        "twice in a loop",
        compile(&[forever(vec![le("a", n(1)), le("a", n(1))])]),
        Err(Twice("a".into()))
    );
    check!(
        "zero loops",
        compile(&[forever(vec![leave(n(1), 0)])]),
        Err(NoSuchLoop)
    );
    check!(
        "one too many",
        compile(&[forever(vec![forever(vec![leave(n(1), 3)])])]),
        Err(NoSuchLoop)
    );
    check!(
        "test first",
        compile(&[leave(v("t"), 5)]),
        Err(unknown("t"))
    );
    check!(
        "unreachable",
        compile(&[forever(vec![leave(n(1), 1), print(v("u"))])]),
        Err(unknown("u"))
    );
    check!(
        "huge count",
        compile(&[forever(vec![leave(n(1), usize::MAX)])]),
        Err(NoSuchLoop)
    );
}

#[test]
fn an_inner_block_may_declare_the_name_again() {
    use build::*;
    let p = [
        le("a", n(1)),
        block(vec![
            set("a", n(2)),
            le("a", n(3)),
            set("a", n(4)),
            print(v("a")),
        ]),
        print(v("a")),
    ];
    check!("shadow after assign", go(&p), Ok(vec![4, 2]));
}

#[test]
fn a_loop_body_starts_fresh_each_round() {
    use build::*;
    // Counts 0, 1, 2 with a variable that is made again each round.
    let p = [
        le("i", n(0)),
        forever(vec![
            forever(vec![
                le("seen", v("i")),
                print(v("seen")),
                set("i", add(v("i"), n(1))),
                leave(n(1), 1),
            ]),
            leave(sub(v("i"), n(1)), 1),
        ]),
    ];
    check!("rounds", go(&p), Ok(vec![0, 1]));
}

#[test]
fn many_rounds_do_not_fill_the_stack() {
    use build::*;
    // Doubles x each round until it wraps to 0; every round declares variables, some in an inner block that is left early.
    let p = [
        le("x", n(1)),
        forever(vec![
            le("a", v("x")),
            forever(vec![
                le("b", v("a")),
                block(vec![le("c", v("b")), leave(v("c"), 1)]),
                leave(n(1), 1),
            ]),
            set("x", add(v("x"), v("a"))),
            forever(vec![le("d", n(0)), leave(v("x"), 1), leave(n(1), 2)]),
        ]),
        print(v("x")),
    ];
    let code = compile(&p).unwrap();
    check!("64 rounds", device(&code), Ok((vec![0], 5)));
}

#[test]
fn many_variables() {
    use build::*;
    let mut p: Vec<Stmt> = (0..2000).map(|i| le(&format!("v{i}"), n(i))).collect();
    p.push(print(add(v("v0"), v("v1999"))));
    check!("2000 variables", go(&p), Ok(vec![1999]));
}

// Random programs without endless loops, compared with a direct interpreter that also works out how small the stack
// can be.

#[allow(dead_code)]
struct Gen {
    rng: anneal_prelude::Rng,
    next: usize,
}

#[allow(dead_code)]
impl Gen {
    fn expr(&mut self, names: &[String], depth: usize) -> Expr {
        if depth > 0 && self.rng.below(3) == 0 {
            let a = Box::new(self.expr(names, depth - 1));
            let b = Box::new(self.expr(names, depth - 1));
            return if self.rng.bool() {
                Expr::Add(a, b)
            } else {
                Expr::Sub(a, b)
            };
        }
        if !names.is_empty() && self.rng.below(3) > 0 {
            Expr::Name(self.rng.pick(names).clone())
        } else {
            Expr::Number(self.rng.int(0, 3))
        }
    }

    /// Every loop ends in an unconditional leave, so that each runs at most once through.
    fn block(&mut self, names: &mut Vec<String>, loops: usize, depth: usize) -> Vec<Stmt> {
        let outer = names.len();
        let mut own: Vec<String> = Vec::new();
        let mut list = Vec::new();
        for _ in 0..self.rng.below(7) {
            match self.rng.below(7) {
                0 | 1 => {
                    let name = self.rng.pick(&["a", "b", "c", "d"]).to_string();
                    if !own.contains(&name) {
                        let value = self.expr(names, 2);
                        list.push(Stmt::Let(name.clone(), value));
                        own.push(name.clone());
                        names.push(name);
                    }
                }
                2 if !names.is_empty() => {
                    let name = self.rng.pick(names).clone();
                    list.push(Stmt::Assign(name, self.expr(names, 2)));
                }
                3 if depth > 0 => list.push(Stmt::Block(self.block(names, loops, depth - 1))),
                4 if depth > 0 => {
                    let mut body = self.block(names, loops + 1, depth - 1);
                    body.push(Stmt::LeaveIf(Expr::Number(1), 1));
                    list.push(Stmt::Loop(body));
                }
                5 if loops > 0 => {
                    let test = self.expr(names, 1);
                    list.push(Stmt::LeaveIf(test, 1 + self.rng.below(loops)));
                }
                _ => list.push(Stmt::Print(self.expr(names, 2))),
            }
        }
        names.truncate(outer);
        list
    }
}

#[allow(dead_code)]
#[derive(Default)]
struct Direct {
    vars: Vec<(String, i64)>,
    printed: Vec<i64>,
    /// The most slots needed: live variables plus the values of the expression being worked out.
    peak: usize,
}

#[allow(dead_code)]
impl Direct {
    /// `below` values of this expression are already waiting.
    fn eval(&mut self, e: &Expr, below: usize) -> i64 {
        self.peak = self.peak.max(self.vars.len() + below + 1);
        match e {
            Expr::Number(x) => *x,
            Expr::Name(x) => self.vars.iter().rev().find(|v| v.0 == *x).unwrap().1,
            Expr::Add(a, b) => {
                let a = self.eval(a, below);
                a.wrapping_add(self.eval(b, below + 1))
            }
            Expr::Sub(a, b) => {
                let a = self.eval(a, below);
                a.wrapping_sub(self.eval(b, below + 1))
            }
        }
    }

    /// `Some(n)`: leaving n more loops.
    fn block(&mut self, list: &[Stmt]) -> Option<usize> {
        let outer = self.vars.len();
        let mut leaving = None;
        for s in list {
            match s {
                Stmt::Let(x, e) => {
                    let value = self.eval(e, 0);
                    self.vars.push((x.clone(), value));
                }
                Stmt::Assign(x, e) => {
                    let value = self.eval(e, 0);
                    self.vars.iter_mut().rev().find(|v| v.0 == *x).unwrap().1 = value;
                }
                Stmt::Print(e) => {
                    let value = self.eval(e, 0);
                    self.printed.push(value);
                }
                Stmt::Block(inner) => leaving = self.block(inner),
                Stmt::Loop(inner) => loop {
                    if let Some(n) = self.block(inner) {
                        leaving = (n > 1).then(|| n - 1);
                        break;
                    }
                },
                Stmt::LeaveIf(e, n) => leaving = (self.eval(e, 0) != 0).then_some(*n),
            }
            if leaving.is_some() {
                break;
            }
        }
        self.vars.truncate(outer);
        leaving
    }
}

#[test]
fn random_programs() {
    for seed in 0..3000 {
        let mut gen = Gen {
            rng: anneal_prelude::Rng::new(6800 + seed),
            next: 0,
        };
        let program = gen.block(&mut Vec::new(), 0, 4);
        let mut direct = Direct::default();
        direct.block(&program);
        let text = format!("{program:?}");
        let code = compile(&program).map_err(|e| format!("{e:?}"));
        let got = code.and_then(|c| device(&c));
        // The peak may not be above what is needed.
        let got = got.map(|(printed, peak)| (printed, peak <= direct.peak));
        check!(text, got, Ok((direct.printed.clone(), true)));
    }
}

#[test]
fn a_leave_that_is_not_taken_changes_nothing() {
    use build::*;
    let p = [forever(vec![
        le("a", n(5)),
        block(vec![
            le("b", n(6)),
            leave(n(0), 1),
            print(add(v("a"), v("b"))),
        ]),
        print(v("a")),
        leave(n(1), 1),
    ])];
    check!("test is 0", go(&p), Ok(vec![11, 5]));
}
