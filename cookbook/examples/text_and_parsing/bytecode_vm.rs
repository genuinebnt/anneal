//! RECIPE 97 (advanced): a mini compiler pipeline: lex -> Pratt parse -> AST -> optimise -> bytecode -> stack VM.
//! LEVEL: advanced
//! Use in: the DSL + bytecode-VM roadmap item; learning how every interpreter/compiler is shaped.   Run: cargo run --release --example bytecode_vm
//! COMMON USES: scripting/DSL engines, query evaluators, config languages, WASM-like VMs; differential testing against an oracle.
#![allow(dead_code)]
//!
//! Stages (each is a plain function you can test alone):
//!   lex      text -> tokens                       parse     tokens -> `Expr` tree (Pratt precedence, see pratt_parser)
//!   fold     `Expr` -> `Expr` (constant folding)  compile   `Expr` -> `Vec<Op>` by POST-ORDER traversal (operands first, then op)
//!   run      a loop over `Op`s with a value stack; errors (stack underflow, divide by zero, unknown variable) are values, not panics
//! Verification by DIFFERENTIAL TESTING: an obviously-correct tree-walking interpreter and the VM must agree on thousands of
//! random programs, with and without the optimiser. Study: https://craftinginterpreters.com/a-virtual-machine.html and
//! https://github.com/oxc-project/oxc/tree/main/crates/oxc_parser and https://github.com/bytecodealliance/wasmtime (Cranelift IR)
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Num(i64),
    Var(String),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Push(i64),
    Load(usize), // index into the variable table
    Neg,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, PartialEq)]
enum VmError {
    StackUnderflow,
    DivByZero,
    UnknownVar(String),
    Overflow,
    Parse(String),
}

// ---------- lexer + parser ----------
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(i64),
    Id(String),
    Sym(char),
}
fn lex(s: &str) -> Result<Vec<Tok>, VmError> {
    let cs: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let st = i;
            while i < cs.len() && cs[i].is_ascii_digit() {
                i += 1;
            }
            let t: String = cs[st..i].iter().collect();
            out.push(Tok::Num(
                t.parse()
                    .map_err(|_| VmError::Parse(format!("number too large: {t}")))?,
            ));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(cs[st..i].iter().collect()));
        } else if "+-*/()".contains(c) {
            out.push(Tok::Sym(c));
            i += 1;
        } else {
            return Err(VmError::Parse(format!("unexpected `{c}`")));
        }
    }
    Ok(out)
}
struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}
impl Parser {
    fn expr(&mut self, min_bp: u8) -> Result<Expr, VmError> {
        let t = self
            .toks
            .get(self.pos)
            .cloned()
            .ok_or(VmError::Parse("unexpected end".into()))?;
        self.pos += 1;
        let mut lhs = match t {
            Tok::Num(n) => Expr::Num(n),
            Tok::Id(v) => Expr::Var(v),
            Tok::Sym('-') => Expr::Neg(Box::new(self.expr(5)?)),
            Tok::Sym('(') => {
                let e = self.expr(0)?;
                if self.toks.get(self.pos) != Some(&Tok::Sym(')')) {
                    return Err(VmError::Parse("expected `)`".into()));
                }
                self.pos += 1;
                e
            }
            other => return Err(VmError::Parse(format!("unexpected {other:?}"))),
        };
        while let Some(Tok::Sym(op @ ('+' | '-' | '*' | '/'))) = self.toks.get(self.pos).cloned() {
            let (l, r) = if op == '+' || op == '-' {
                (1, 2)
            } else {
                (3, 4)
            };
            if l < min_bp {
                break;
            }
            self.pos += 1;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(self.expr(r)?));
        }
        Ok(lhs)
    }
}
fn parse(src: &str) -> Result<Expr, VmError> {
    let mut p = Parser {
        toks: lex(src)?,
        pos: 0,
    };
    let e = p.expr(0)?;
    if p.pos != p.toks.len() {
        return Err(VmError::Parse("trailing tokens".into()));
    }
    Ok(e)
}

// ---------- optimiser: constant folding ----------
fn fold(e: Expr) -> Expr {
    match e {
        Expr::Neg(inner) => match fold(*inner) {
            Expr::Num(n) => n
                .checked_neg()
                .map_or(Expr::Neg(Box::new(Expr::Num(n))), Expr::Num),
            other => Expr::Neg(Box::new(other)),
        },
        Expr::Bin(op, a, b) => {
            let (a, b) = (fold(*a), fold(*b));
            if let (Expr::Num(x), Expr::Num(y)) = (&a, &b) {
                let r = match op {
                    '+' => x.checked_add(*y),
                    '-' => x.checked_sub(*y),
                    '*' => x.checked_mul(*y),
                    '/' if *y != 0 => x.checked_div(*y), // keep `1/0` unfolded so the runtime error is preserved
                    _ => None,
                };
                if let Some(v) = r {
                    return Expr::Num(v);
                }
            }
            Expr::Bin(op, Box::new(a), Box::new(b))
        }
        other => other,
    }
}

// ---------- compiler + VM ----------
struct Program {
    ops: Vec<Op>,
    vars: Vec<String>, // variable table: Load(i) refers to vars[i]
}
fn compile(e: &Expr, p: &mut Program) {
    match e {
        Expr::Num(n) => p.ops.push(Op::Push(*n)),
        Expr::Var(v) => {
            let idx = p.vars.iter().position(|x| x == v).unwrap_or_else(|| {
                p.vars.push(v.clone());
                p.vars.len() - 1
            });
            p.ops.push(Op::Load(idx));
        }
        Expr::Neg(a) => {
            compile(a, p);
            p.ops.push(Op::Neg);
        }
        Expr::Bin(op, a, b) => {
            compile(a, p); // operands first (post-order) ...
            compile(b, p);
            p.ops.push(match op {
                '+' => Op::Add,
                '-' => Op::Sub,
                '*' => Op::Mul,
                _ => Op::Div,
            }); // ... then the operator
        }
    }
}
fn run(p: &Program, env: &HashMap<String, i64>) -> Result<i64, VmError> {
    let mut stack: Vec<i64> = Vec::with_capacity(16);
    for op in &p.ops {
        match *op {
            Op::Push(n) => stack.push(n),
            Op::Load(i) => stack.push(
                *env.get(&p.vars[i])
                    .ok_or_else(|| VmError::UnknownVar(p.vars[i].clone()))?,
            ),
            Op::Neg => {
                let a = stack.pop().ok_or(VmError::StackUnderflow)?;
                stack.push(a.checked_neg().ok_or(VmError::Overflow)?);
            }
            bin => {
                let b = stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = stack.pop().ok_or(VmError::StackUnderflow)?;
                stack.push(
                    match bin {
                        Op::Add => a.checked_add(b),
                        Op::Sub => a.checked_sub(b),
                        Op::Mul => a.checked_mul(b),
                        Op::Div => {
                            if b == 0 {
                                return Err(VmError::DivByZero);
                            }
                            a.checked_div(b)
                        }
                        _ => unreachable!(),
                    }
                    .ok_or(VmError::Overflow)?,
                );
            }
        }
    }
    stack.pop().ok_or(VmError::StackUnderflow)
}

// ---------- reference tree-walking interpreter (the oracle for differential tests) ----------
fn eval(e: &Expr, env: &HashMap<String, i64>) -> Result<i64, VmError> {
    Ok(match e {
        Expr::Num(n) => *n,
        Expr::Var(v) => *env.get(v).ok_or_else(|| VmError::UnknownVar(v.clone()))?,
        Expr::Neg(a) => eval(a, env)?.checked_neg().ok_or(VmError::Overflow)?,
        Expr::Bin(op, a, b) => {
            let (x, y) = (eval(a, env)?, eval(b, env)?);
            match op {
                '+' => x.checked_add(y),
                '-' => x.checked_sub(y),
                '*' => x.checked_mul(y),
                _ => {
                    if y == 0 {
                        return Err(VmError::DivByZero);
                    }
                    x.checked_div(y)
                }
            }
            .ok_or(VmError::Overflow)?
        }
    })
}

fn build(src: &str, optimise: bool) -> Result<Program, VmError> {
    let mut e = parse(src)?;
    if optimise {
        e = fold(e);
    }
    let mut p = Program {
        ops: vec![],
        vars: vec![],
    };
    compile(&e, &mut p);
    Ok(p)
}

fn rnd(rng: &mut u64, n: u64) -> u64 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    *rng % n
}

fn random_expr(rng: &mut u64, depth: u32) -> String {
    if depth == 0 || rnd(rng, 4) == 0 {
        return match rnd(rng, 3) {
            0 => format!("{}", rnd(rng, 20)),
            1 => ["x", "y"][rnd(rng, 2) as usize].to_string(),
            _ => format!("{}", rnd(rng, 1000)),
        };
    }
    let (a, b) = (random_expr(rng, depth - 1), random_expr(rng, depth - 1));
    match rnd(rng, 6) {
        0 => format!("({a} + {b})"),
        1 => format!("({a} - {b})"),
        2 => format!("({a} * {b})"),
        3 => format!("({a} / {b})"),
        4 => format!("(-{a})"),
        _ => format!("({a} + {b} * {a})"),
    }
}

fn main() {
    let src = "2 * (3 + 4) - x / 5 + (10 - 4 * 2)";
    let env: HashMap<String, i64> = [("x".to_string(), 20)].into();
    for optimise in [false, true] {
        let p = build(src, optimise).unwrap();
        println!(
            "optimise={optimise}: {} ops {:?} -> {:?}",
            p.ops.len(),
            p.ops,
            run(&p, &env)
        );
    }
    println!(
        "{:?} {:?} {:?}",
        run(&build("1 / (x - x)", true).unwrap(), &env),
        run(&build("y + 1", false).unwrap(), &env),
        build("2 +", false).err()
    );

    // differential test: tree interpreter == VM == optimised VM, including identical errors
    let mut rng = 0x1234_5678_9ABC_DEF1u64;
    let (mut ok, mut errs, mut shrunk) = (0, 0, 0);
    for _ in 0..5000 {
        let src = random_expr(&mut rng, 5);
        let env: HashMap<String, i64> = [("x".to_string(), 7), ("y".to_string(), -3)].into();
        let oracle = eval(&parse(&src).unwrap(), &env);
        let plain = build(&src, false).unwrap();
        let opt = build(&src, true).unwrap();
        assert_eq!(run(&plain, &env), oracle, "VM vs oracle on {src}");
        assert_eq!(run(&opt, &env), oracle, "optimised VM vs oracle on {src}");
        shrunk += (opt.ops.len() < plain.ops.len()) as u32;
        if oracle.is_ok() { ok += 1 } else { errs += 1 }
    }
    println!(
        "5000 random programs agree: {ok} ok, {errs} runtime errors (all identical); optimiser shrank {shrunk} programs"
    );
}
