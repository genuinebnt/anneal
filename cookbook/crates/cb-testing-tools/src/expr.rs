//! Arithmetic expressions: `-a * (b + 2) / 3`. Precedence: unary minus > `* /` > `+ -`,
//! binary operators are left-associative. `format_expr` emits the *minimal* parentheses that
//! preserve the tree, so `parse_expr(&format_expr(&e)) == Ok(e)` for every tree.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn prec(self) -> u8 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div => 2,
        }
    }
    fn sym(self) -> char {
        match self {
            Op::Add => '+',
            Op::Sub => '-',
            Op::Mul => '*',
            Op::Div => '/',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Num(i64),
    Var(String),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub pos: usize,
    pub kind: ErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    UnexpectedEnd,
    UnexpectedChar(char),
    NumberTooLarge,
    TooDeep,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {:?}", self.pos, self.kind)
    }
}
impl std::error::Error for ParseError {}

/// Nesting limit: keeps recursion (and `Drop`) bounded on hostile input like `((((((...`.
pub const MAX_DEPTH: usize = 64;
/// Total operator-node limit: left-associative chains (`1+1+1+...`) nest deeply WITHOUT parens,
/// so the depth limit alone would not bound recursion in `format_expr`, `eval` or `Drop`.
pub const MAX_NODES: usize = 2000;

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    depth: usize,
    nodes: usize,
}

type PResult<T> = Result<T, ParseError>;

impl Parser<'_> {
    fn err<T>(&self, kind: ErrorKind) -> PResult<T> {
        Err(ParseError {
            pos: self.pos,
            kind,
        })
    }
    fn peek(&mut self) -> Option<u8> {
        let b = self.src.as_bytes();
        while matches!(b.get(self.pos), Some(b' ' | b'\t' | b'\n')) {
            self.pos += 1;
        }
        b.get(self.pos).copied()
    }
    fn unexpected<T>(&self) -> PResult<T> {
        match self.src[self.pos..].chars().next() {
            Some(c) => self.err(ErrorKind::UnexpectedChar(c)),
            None => self.err(ErrorKind::UnexpectedEnd),
        }
    }
    /// Precedence climbing: parse operators with precedence >= `min`.
    fn expr(&mut self, min: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(b'+') => Op::Add,
                Some(b'-') => Op::Sub,
                Some(b'*') => Op::Mul,
                Some(b'/') => Op::Div,
                _ => return Ok(lhs),
            };
            if op.prec() < min {
                return Ok(lhs);
            }
            self.pos += 1;
            self.nodes += 1;
            if self.nodes > MAX_NODES {
                return self.err(ErrorKind::TooDeep);
            }
            let rhs = self.expr(op.prec() + 1)?; // +1 => left associative
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
    }
    fn unary(&mut self) -> PResult<Expr> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.err(ErrorKind::TooDeep);
        }
        let r = self.atom();
        self.depth -= 1;
        r
    }
    fn atom(&mut self) -> PResult<Expr> {
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                self.nodes += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some(b'(') => {
                self.pos += 1;
                let e = self.expr(1)?;
                if self.peek() == Some(b')') {
                    self.pos += 1;
                    Ok(e)
                } else {
                    self.unexpected()
                }
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                while self
                    .src
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(u8::is_ascii_digit)
                {
                    self.pos += 1;
                }
                match self.src[start..self.pos].parse() {
                    Ok(n) => Ok(Expr::Num(n)),
                    Err(_) => Err(ParseError {
                        pos: start,
                        kind: ErrorKind::NumberTooLarge,
                    }),
                }
            }
            Some(c) if c.is_ascii_lowercase() || c == b'_' => {
                let start = self.pos;
                while self
                    .src
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                {
                    self.pos += 1;
                }
                Ok(Expr::Var(self.src[start..self.pos].to_string()))
            }
            _ => self.unexpected(),
        }
    }
}

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    let mut p = Parser {
        src,
        pos: 0,
        depth: 0,
        nodes: 0,
    };
    let e = p.expr(1)?;
    match p.peek() {
        None => Ok(e),
        Some(_) => p.unexpected(),
    }
}

fn fmt_prec(e: &Expr, min: u8, out: &mut String, buggy: bool) {
    match e {
        Expr::Num(n) => out.push_str(&n.to_string()),
        Expr::Var(v) => out.push_str(v),
        Expr::Neg(inner) if min > 3 => unreachable!("unary never needs min > 3: got {inner:?}"),
        Expr::Neg(inner) => {
            out.push('-');
            fmt_prec(inner, 3, out, buggy);
        }
        Expr::Bin(op, l, r) => {
            let p = op.prec();
            let paren = p < min;
            if paren {
                out.push('(');
            }
            fmt_prec(l, p, out, buggy);
            out.push_str(&format!(" {} ", op.sym()));
            // Correct: the right child must bind strictly tighter (left associativity).
            // Buggy variant forgets the `+ 1`, so `a - (b - c)` loses its parentheses.
            fmt_prec(r, if buggy { p } else { p + 1 }, out, buggy);
            if paren {
                out.push(')');
            }
        }
    }
}

/// Pretty-prints with minimal parentheses.
pub fn format_expr(e: &Expr) -> String {
    let mut s = String::new();
    fmt_prec(e, 0, &mut s, false);
    s
}

/// DELIBERATELY WRONG printer, used by the shrinking demo. Never use outside tests.
#[doc(hidden)]
pub fn format_expr_buggy(e: &Expr) -> String {
    let mut s = String::new();
    fmt_prec(e, 0, &mut s, true);
    s
}

/// Wrapping evaluation; `None` on unbound variable or division by zero.
pub fn eval(e: &Expr, env: &dyn Fn(&str) -> Option<i64>) -> Option<i64> {
    Some(match e {
        Expr::Num(n) => *n,
        Expr::Var(v) => env(v)?,
        Expr::Neg(x) => eval(x, env)?.wrapping_neg(),
        Expr::Bin(op, l, r) => {
            let (a, b) = (eval(l, env)?, eval(r, env)?);
            match op {
                Op::Add => a.wrapping_add(b),
                Op::Sub => a.wrapping_sub(b),
                Op::Mul => a.wrapping_mul(b),
                Op::Div => a.checked_div(b)?,
            }
        }
    })
}

/// Bottom-up constant folding and identity removal (`x+0`, `x*1`, `--x`). Idempotent and
/// semantics-preserving; both are property-tested.
pub fn simplify(e: &Expr) -> Expr {
    match e {
        Expr::Neg(x) => match simplify(x) {
            Expr::Neg(inner) => *inner,
            s => Expr::Neg(Box::new(s)),
        },
        Expr::Bin(op, l, r) => {
            let (l, r) = (simplify(l), simplify(r));
            let folded = match (op, &l, &r) {
                (Op::Add, Expr::Num(a), Expr::Num(b)) => a.checked_add(*b),
                (Op::Sub, Expr::Num(a), Expr::Num(b)) => a.checked_sub(*b).filter(|v| *v >= 0),
                (Op::Mul, Expr::Num(a), Expr::Num(b)) => a.checked_mul(*b),
                _ => None,
            };
            match (op, folded, &l, &r) {
                (_, Some(n), _, _) => Expr::Num(n),
                (Op::Add, _, x, Expr::Num(0)) | (Op::Add, _, Expr::Num(0), x) => x.clone(),
                (Op::Sub, _, x, Expr::Num(0)) => x.clone(),
                (Op::Mul, _, x, Expr::Num(1)) | (Op::Mul, _, Expr::Num(1), x) => x.clone(),
                (Op::Div, _, x, Expr::Num(1)) => x.clone(),
                _ => Expr::Bin(*op, Box::new(l), Box::new(r)),
            }
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_and_assoc() {
        assert_eq!(
            format_expr(&parse_expr("1 - (2 - 3)").unwrap()),
            "1 - (2 - 3)"
        );
        assert_eq!(
            format_expr(&parse_expr("((1 - 2) - 3)").unwrap()),
            "1 - 2 - 3"
        );
        assert_eq!(format_expr(&parse_expr("-(a*b)").unwrap()), "-(a * b)");
        assert!(parse_expr("1 +").is_err());
    }
}
