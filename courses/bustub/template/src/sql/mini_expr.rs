//! A tiny arithmetic expression language: the tree, a parser and a printer.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Num(i64),
    Neg(Box<Expr>),
    Bin(Box<Expr>, Op, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    pub fn prec(self) -> u8 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div => 2,
        }
    }

    pub fn symbol(self) -> char {
        match self {
            Op::Add => '+',
            Op::Sub => '-',
            Op::Mul => '*',
            Op::Div => '/',
        }
    }
}

/// Prints `e` with the fewest parentheses that keep its meaning.
pub fn print_expr(e: &Expr) -> String {
    todo!("3d-c2: print children, parenthesising by precedence and associativity")
}

/// Parses text into a tree (precedence climbing). [`print_expr`] must agree with it.
pub fn parse(s: &str) -> Option<Expr> {
    let toks: Vec<String> = {
        let mut v = Vec::new();
        let mut cur = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                cur.push(c);
            } else {
                if !cur.is_empty() {
                    v.push(std::mem::take(&mut cur));
                }
                if !c.is_whitespace() {
                    v.push(c.to_string());
                }
            }
        }
        if !cur.is_empty() {
            v.push(cur);
        }
        v
    };
    fn atom(t: &[String], i: &mut usize) -> Option<Expr> {
        let tok = t.get(*i)?;
        *i += 1;
        if tok == "(" {
            let e = expr(t, i, 0)?;
            if t.get(*i)? != ")" {
                return None;
            }
            *i += 1;
            Some(e)
        } else if tok == "-" {
            Some(Expr::Neg(Box::new(atom(t, i)?)))
        } else {
            tok.parse::<i64>().ok().map(Expr::Num)
        }
    }
    fn expr(t: &[String], i: &mut usize, min: u8) -> Option<Expr> {
        let mut lhs = atom(t, i)?;
        loop {
            let op = match t.get(*i).map(String::as_str) {
                Some("+") => Op::Add,
                Some("-") => Op::Sub,
                Some("*") => Op::Mul,
                Some("/") => Op::Div,
                _ => return Some(lhs),
            };
            if op.prec() < min.max(1) {
                return Some(lhs);
            }
            *i += 1;
            let rhs = expr(t, i, op.prec() + 1)?;
            lhs = Expr::Bin(Box::new(lhs), op, Box::new(rhs));
        }
    }
    let mut i = 0;
    let e = expr(&toks, &mut i, 0)?;
    if i == toks.len() {
        Some(e)
    } else {
        None
    }
}
