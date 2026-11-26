//! RECIPE 20 (parsing): Pratt parser: precedence and associativity with "binding power".
//! LEVEL: advanced
//! Use in: the DSL / expression language (conditions, control flow).   Run: cargo run --example pratt_parser
//! COMMON USES: expression languages: calculators, filter expressions in query languages, spreadsheet formulas.
//!
//! Pattern: each infix operator has a (left, right) binding power. Parse a prefix item, then loop: if the next
//! operator binds tighter than the current minimum, consume it and parse its right side with ITS right power.
//! Left-associative: right > left. Right-associative (`^`): right < left.
//! Read: https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
#![allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
    L,
    R,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = vec![];
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            let t: String = cs[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number {t}"))?));
        } else if "+-*/^".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else if c == '(' {
            out.push(Tok::L);
            i += 1;
        } else if c == ')' {
            out.push(Tok::R);
            i += 1;
        } else {
            return Err(format!("unexpected `{c}`"));
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

fn infix_bp(op: char) -> (u8, u8) {
    match op {
        '+' | '-' => (1, 2),
        '*' | '/' => (3, 4),
        '^' => (8, 7), // right-associative
        _ => unreachable!(),
    }
}

impl Parser {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.peek();
        self.pos += 1;
        t
    }
    fn expr(&mut self, min_bp: u8) -> Result<f64, String> {
        let mut lhs = match self.bump() {
            Some(Tok::Num(n)) => n,
            Some(Tok::Op('-')) => -self.expr(5)?, // prefix minus binds looser than ^
            Some(Tok::L) => {
                let v = self.expr(0)?;
                if self.bump() != Some(Tok::R) {
                    return Err("expected `)`".into());
                }
                v
            }
            other => return Err(format!("unexpected {other:?}")),
        };
        while let Some(Tok::Op(op)) = self.peek() {
            let (l, r) = infix_bp(op);
            if l < min_bp {
                break;
            }
            self.bump();
            let rhs = self.expr(r)?;
            lhs = match op {
                '+' => lhs + rhs,
                '-' => lhs - rhs,
                '*' => lhs * rhs,
                '/' => lhs / rhs,
                _ => lhs.powf(rhs),
            };
        }
        Ok(lhs)
    }
}

fn eval(s: &str) -> Result<f64, String> {
    let mut p = Parser {
        toks: lex(s)?,
        pos: 0,
    };
    let v = p.expr(0)?;
    if p.pos < p.toks.len() {
        return Err("trailing input".into());
    }
    Ok(v)
}

fn main() {
    for (src, want) in [
        ("1+2*3", 7.0),
        ("(1+2)*3", 9.0),
        ("2^3^2", 512.0),
        ("-2^2", -4.0),
        ("10-4-3", 3.0),
        ("8/2/2", 2.0),
    ] {
        let got = eval(src).unwrap();
        println!("{src:>8} = {got}");
        assert_eq!(got, want, "{src}");
    }
    println!("{:?}", eval("1 + (2"));
}
