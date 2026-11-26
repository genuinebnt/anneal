//! RECIPE 500 (parsing): one expression language, four parsers (recursive descent, precedence climbing, Pratt, shunting-yard) that must agree.
//! LEVEL: advanced
//! Use in: the DSL / filter-expression layer, calculators, spreadsheet formulas, any "parse infix text" task.   Run: cargo run --release --example expression_parsing_three_ways
#![allow(dead_code)]
//!
//! WHAT: a rich expression language: integers, variables, `+ - * / %`, `**` (right assoc), `== != < <= > >=` (NON-associative),
//!   `&& ||`, prefix `- !`, ternary `c ? a : b` (right assoc), calls `f(a, b)`, indexing `a[i]`, member access `a.b`.
//!   The same token stream is parsed by four algorithms and the ASTs are compared as fully parenthesised strings.
//! GRAMMAR (lowest to highest precedence; this is what recursive descent mirrors, one function per line):
//!     expr    := or ( '?' expr ':' expr )?                  -- right assoc: the else branch recurses into the same rule
//!     or      := and ( '||' and )*     and := cmp ( '&&' cmp )*
//!     cmp     := add ( CMPOP add )?                         -- `?` not `*`: `a < b < c` is rejected with a clear error
//!     add     := mul ( ('+'|'-') mul )*    mul := unary ( ('*'|'/'|'%') unary )*
//!     unary   := ('-'|'!') unary | power                    -- so `-2 ** 2` is -(2 ** 2), as in Python/maths
//!     power   := postfix ( '**' unary )?                    -- right operand is `unary`: `2 ** -1` works and `a ** b ** c` is right assoc
//!     postfix := primary ( '(' args ')' | '[' expr ']' | '.' ID )*      primary := NUM | ID | '(' expr ')'
//!   LEFT RECURSION: the natural rule `add := add '+' mul | mul` recurses forever in a top-down parser. Rewrite as `mul ('+' mul)*`
//!   and build the tree INSIDE the loop (`lhs = Bin(lhs, rhs)`), which keeps left associativity. RIGHT associativity needs no loop,
//!   just recursion on the right operand. Non-associative operators take `?` and then look for a second operator to reject it.
//! THE FOUR METHODS: (a) recursive descent: one function per level, easiest to read and to give good errors, but adding an operator
//!   means a new function; (b) precedence climbing: one function plus a table `(prec, assoc)`, the min-precedence argument replaces
//!   the per-level functions; (c) Pratt: binding powers (left, right) per operator, prefix/postfix/mixfix (ternary) are first class;
//!   (d) shunting-yard: an explicit operator stack that emits RPN. It has NO recursion at all, so depth is unbounded.
//! ERRORS are `Diag { span, message, expected }` rendered with a caret line; all methods report the same span for the same input
//!   (checked by 20 000 fuzzed strings). RECOVERY: on a bad statement we record the diagnostic, skip to the next `;` (stray `)` are skipped
//!   on the way) and keep parsing, so one run reports many errors. A lexer error inside a statement suppresses the parse cascade for it.
//! SAFETY: recursive parsers share a depth guard (MAX_DEPTH) so `((((...` gives an error instead of a stack overflow; left-assoc chains
//!   are loops, so a 100 000-token flat expression is parsed on a 512 KiB thread; dropping a 50 000-deep tree uses an explicit worklist.
//! EVALUATION is STRICT (`&&`, `||` and `?:` evaluate every operand) because RPN has no jumps; real languages compile to jumps (see
//!   toy_language_interpreter). Reference evaluator = tree walk; it must equal the RPN stack machine and the Rust oracle values.
//! COMMON USES: query-language WHERE clauses, spreadsheet formulas, feature-flag/rule expressions, calculators, template engines,
//!   config languages; learning which parser style to pick (Pratt for rich operator sets, shunting-yard when you want RPN or no recursion).
//! Read: Crafting Interpreters https://craftinginterpreters.com/parsing-expressions.html , Matklad "Simple but Powerful Pratt Parsing"
//!   https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html , Wikipedia "Shunting yard algorithm" https://en.wikipedia.org/wiki/Shunting_yard_algorithm
use std::fmt::Write as _;
use std::time::Instant;

const MAX_DEPTH: usize = 256;

// ---------- tokens, spans, diagnostics ----------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum K {
    Num(i64),
    Id(String),
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    Bang,
    AndAnd,
    OrOr,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Quest,
    Colon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Semi,
    Eof,
}

impl K {
    /// Human wording used inside "unexpected ..." messages.
    fn text(&self) -> String {
        let sym = match self {
            K::Num(n) => return format!("number `{n}`"),
            K::Id(s) => return format!("identifier `{s}`"),
            K::Eof => return "end of input".into(),
            K::Plus => "+",
            K::Minus => "-",
            K::Star => "*",
            K::StarStar => "**",
            K::Slash => "/",
            K::Percent => "%",
            K::Bang => "!",
            K::AndAnd => "&&",
            K::OrOr => "||",
            K::EqEq => "==",
            K::Ne => "!=",
            K::Lt => "<",
            K::Le => "<=",
            K::Gt => ">",
            K::Ge => ">=",
            K::Quest => "?",
            K::Colon => ":",
            K::LParen => "(",
            K::RParen => ")",
            K::LBracket => "[",
            K::RBracket => "]",
            K::Comma => ",",
            K::Dot => ".",
            K::Semi => ";",
        };
        format!("`{sym}`")
    }
}

#[derive(Clone, Debug)]
struct Token {
    kind: K,
    span: Span,
}

#[derive(Clone, Debug, PartialEq)]
struct Diag {
    span: Span,
    message: String,
    expected: Vec<&'static str>,
}

const ATOM_EXPECTED: &[&str] = &["number", "identifier", "`(`", "`-`", "`!`"];
const TRAILING_EXPECTED: &[&str] = &["operator", "`;`", "end of input"];

fn unexpected(t: &Token, expected: &[&'static str]) -> Diag {
    Diag {
        span: t.span,
        message: format!("unexpected {}", t.kind.text()),
        expected: expected.to_vec(),
    }
}
fn too_deep(t: &Token) -> Diag {
    Diag {
        span: t.span,
        message: format!("expression nested more than {MAX_DEPTH} levels deep"),
        expected: vec![],
    }
}
fn chain_err(t: &Token) -> Diag {
    Diag {
        span: t.span,
        message: "comparison operators are non-associative: write `(a < b) && (b < c)` instead of chaining".into(),
        expected: vec![],
    }
}

/// Render compiler-style: `error: msg`, location, the source line and a caret line under the span.
fn render(src: &str, d: &Diag) -> String {
    let ls = src[..d.span.start].rfind('\n').map_or(0, |i| i + 1);
    let le = src[d.span.start..]
        .find('\n')
        .map_or(src.len(), |i| d.span.start + i);
    let line_no = src[..d.span.start].matches('\n').count() + 1;
    let col = src[ls..d.span.start].chars().count() + 1; // columns count chars, spans count bytes
    let width = src[d.span.start..d.span.end.min(le)].chars().count().max(1);
    let gutter = line_no.to_string();
    let pad = " ".repeat(gutter.len());
    let mut s = format!(
        "error: {}\n{pad}--> {line_no}:{col}\n{pad} |\n{gutter} | {}\n{pad} | ",
        d.message,
        &src[ls..le]
    );
    s += &" ".repeat(col - 1);
    s += &"^".repeat(width);
    if !d.expected.is_empty() {
        let _ = write!(s, " expected {}", d.expected.join(", "));
    }
    s
}

// ---------- lexer (shared by all four parsers) ----------
fn punct(b: &[u8], i: usize) -> Option<(K, usize)> {
    let next = b.get(i + 1).copied();
    Some(match (b[i], next) {
        (b'*', Some(b'*')) => (K::StarStar, 2),
        (b'&', Some(b'&')) => (K::AndAnd, 2),
        (b'|', Some(b'|')) => (K::OrOr, 2),
        (b'=', Some(b'=')) => (K::EqEq, 2),
        (b'!', Some(b'=')) => (K::Ne, 2),
        (b'<', Some(b'=')) => (K::Le, 2),
        (b'>', Some(b'=')) => (K::Ge, 2),
        (b'+', _) => (K::Plus, 1),
        (b'-', _) => (K::Minus, 1),
        (b'*', _) => (K::Star, 1),
        (b'/', _) => (K::Slash, 1),
        (b'%', _) => (K::Percent, 1),
        (b'!', _) => (K::Bang, 1),
        (b'<', _) => (K::Lt, 1),
        (b'>', _) => (K::Gt, 1),
        (b'?', _) => (K::Quest, 1),
        (b':', _) => (K::Colon, 1),
        (b'(', _) => (K::LParen, 1),
        (b')', _) => (K::RParen, 1),
        (b'[', _) => (K::LBracket, 1),
        (b']', _) => (K::RBracket, 1),
        (b',', _) => (K::Comma, 1),
        (b'.', _) => (K::Dot, 1),
        (b';', _) => (K::Semi, 1),
        _ => return None,
    })
}

/// Never fails: bad characters become diagnostics and are skipped, so the parser still sees the rest.
fn lex(src: &str) -> (Vec<Token>, Vec<Diag>) {
    let b = src.as_bytes();
    let (mut toks, mut diags, mut i) = (vec![], vec![], 0);
    while i < b.len() {
        let start = i;
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let span = Span { start, end: i };
            match src[start..i].parse::<i64>() {
                Ok(n) => toks.push(Token {
                    kind: K::Num(n),
                    span,
                }),
                Err(_) => diags.push(Diag {
                    span,
                    message: "integer literal too large for i64".into(),
                    expected: vec![],
                }),
            }
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            toks.push(Token {
                kind: K::Id(src[start..i].to_string()),
                span: Span { start, end: i },
            });
        } else if let Some((kind, n)) = punct(b, i) {
            i += n;
            toks.push(Token {
                kind,
                span: Span { start, end: i },
            });
        } else {
            // `i` is always on a char boundary here, so decode the whole (possibly multi-byte) char.
            let ch = src[i..].chars().next().unwrap_or('?');
            i += ch.len_utf8();
            let hint = match ch {
                '&' | '|' => format!(" (did you mean `{ch}{ch}`?)"),
                '=' => " (did you mean `==`?)".to_string(),
                _ => String::new(),
            };
            diags.push(Diag {
                span: Span { start, end: i },
                message: format!("unexpected character `{ch}`{hint}"),
                expected: vec![],
            });
        }
    }
    toks.push(Token {
        kind: K::Eof,
        span: Span {
            start: b.len(),
            end: b.len(),
        },
    });
    (toks, diags)
}

// ---------- AST, pretty printers, tree-to-source oracle ----------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnOp {
    Neg,
    Not,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}
const BIN_OPS: [BinOp; 14] = [
    BinOp::Or,
    BinOp::And,
    BinOp::Eq,
    BinOp::Ne,
    BinOp::Lt,
    BinOp::Le,
    BinOp::Gt,
    BinOp::Ge,
    BinOp::Add,
    BinOp::Sub,
    BinOp::Mul,
    BinOp::Div,
    BinOp::Rem,
    BinOp::Pow,
];

impl UnOp {
    fn sym(self) -> &'static str {
        if self == UnOp::Neg { "-" } else { "!" }
    }
}
impl BinOp {
    fn sym(self) -> &'static str {
        match self {
            BinOp::Or => "||",
            BinOp::And => "&&",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Pow => "**",
        }
    }
    /// Precedence levels: 1 ternary, 2 `||`, 3 `&&`, 4 comparison, 5 additive, 6 multiplicative, 7 unary, 8 `**`, 9 postfix/atoms.
    fn prec(self) -> u8 {
        match self {
            BinOp::Or => 2,
            BinOp::And => 3,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div | BinOp::Rem => 6,
            BinOp::Pow => 8,
        }
    }
    fn is_cmp(self) -> bool {
        self.prec() == 4
    }
}
fn bin_of(k: &K) -> Option<BinOp> {
    Some(match k {
        K::OrOr => BinOp::Or,
        K::AndAnd => BinOp::And,
        K::EqEq => BinOp::Eq,
        K::Ne => BinOp::Ne,
        K::Lt => BinOp::Lt,
        K::Le => BinOp::Le,
        K::Gt => BinOp::Gt,
        K::Ge => BinOp::Ge,
        K::Plus => BinOp::Add,
        K::Minus => BinOp::Sub,
        K::Star => BinOp::Mul,
        K::Slash => BinOp::Div,
        K::Percent => BinOp::Rem,
        K::StarStar => BinOp::Pow,
        _ => return None,
    })
}
fn un_of(k: &K) -> Option<UnOp> {
    match k {
        K::Minus => Some(UnOp::Neg),
        K::Bang => Some(UnOp::Not),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Num(i64),
    Var(String),
    Un(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Tern(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Member(Box<Expr>, String),
}

fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    Expr::Bin(op, Box::new(l), Box::new(r))
}
fn un(op: UnOp, x: Expr) -> Expr {
    Expr::Un(op, Box::new(x))
}

/// Fully parenthesised form: every operator node gets its own parens, so two ASTs are equal iff these strings are.
fn full(e: &Expr) -> String {
    match e {
        Expr::Num(n) => n.to_string(),
        Expr::Var(v) => v.clone(),
        Expr::Un(op, x) => format!("({}{})", op.sym(), full(x)),
        Expr::Bin(op, l, r) => format!("({} {} {})", full(l), op.sym(), full(r)),
        Expr::Tern(c, t, f) => format!("({} ? {} : {})", full(c), full(t), full(f)),
        Expr::Call(c, a) => format!(
            "{}({})",
            full(c),
            a.iter().map(full).collect::<Vec<_>>().join(", ")
        ),
        Expr::Index(b, i) => format!("{}[{}]", full(b), full(i)),
        Expr::Member(b, n) => format!("{}.{n}", full(b)),
    }
}

/// Minimal-parenthesis source for a tree: an INDEPENDENT oracle of the precedence rules (it only adds parens when needed).
/// `min` is the lowest precedence level the context accepts without parentheses.
fn source_of(e: &Expr, min: u8) -> String {
    let (s, level) = match e {
        Expr::Num(n) => (n.to_string(), 9),
        Expr::Var(v) => (v.clone(), 9),
        Expr::Un(op, x) => (format!("{}{}", op.sym(), source_of(x, 7)), 7),
        Expr::Bin(op, l, r) => {
            let p = op.prec();
            // `**`: left operand must be postfix-level (so `(-a) ** b` needs parens), right is `unary`.
            let (lm, rm) = match op {
                BinOp::Pow => (9, 7),
                o if o.is_cmp() => (p + 1, p + 1), // non-associative: a comparison operand needs parens
                _ => (p, p + 1),                   // left associative
            };
            (
                format!("{} {} {}", source_of(l, lm), op.sym(), source_of(r, rm)),
                p,
            )
        }
        Expr::Tern(c, t, f) => (
            format!(
                "{} ? {} : {}",
                source_of(c, 2),
                source_of(t, 1),
                source_of(f, 1)
            ),
            1,
        ),
        Expr::Call(c, a) => {
            let args: Vec<String> = a.iter().map(|x| source_of(x, 1)).collect();
            (format!("{}({})", source_of(c, 9), args.join(", ")), 9)
        }
        Expr::Index(b, i) => (format!("{}[{}]", source_of(b, 9), source_of(i, 1)), 9),
        Expr::Member(b, n) => (format!("{}.{n}", source_of(b, 9)), 9),
    };
    if level < min { format!("({s})") } else { s }
}

/// Drop a deep tree without recursion (the default recursive Drop would overflow the stack on a 50 000-deep chain).
fn dismantle(e: Expr) -> usize {
    let (mut work, mut nodes) = (vec![e], 0);
    while let Some(x) = work.pop() {
        nodes += 1;
        match x {
            Expr::Un(_, a) | Expr::Member(a, _) => work.push(*a),
            Expr::Bin(_, a, b) | Expr::Index(a, b) => work.extend([*a, *b]),
            Expr::Tern(a, b, c) => work.extend([*a, *b, *c]),
            Expr::Call(c, args) => {
                work.push(*c);
                work.extend(args);
            }
            Expr::Num(_) | Expr::Var(_) => {}
        }
    }
    nodes
}

// ---------- shared parser plumbing ----------
type R = Result<Expr, Diag>;
type Method = fn(&[Token], usize) -> Result<(Expr, usize), Diag>;

struct P<'a> {
    t: &'a [Token],
    pos: usize,
    depth: usize,
}

impl<'a> P<'a> {
    fn new(t: &'a [Token], pos: usize) -> Self {
        P { t, pos, depth: 0 }
    }
    // These return `'a` references (not tied to &self) so a peeked token can be held while we keep mutating the parser.
    fn peek(&self) -> &'a Token {
        &self.t[self.pos.min(self.t.len() - 1)]
    }
    fn kind(&self) -> &'a K {
        &self.peek().kind
    }
    fn eat(&mut self, k: &K) -> bool {
        let hit = self.kind() == k;
        if hit {
            self.pos += 1;
        }
        hit
    }
    fn expect(&mut self, k: K, expected: &[&'static str]) -> Result<(), Diag> {
        if self.eat(&k) {
            Ok(())
        } else {
            Err(unexpected(self.peek(), expected))
        }
    }
    /// Depth guard: every recursive entry goes through here, so hostile nesting is an error, not a stack overflow.
    fn guarded(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(too_deep(self.peek()));
        }
        let r = f(self);
        self.depth -= 1;
        r
    }
    fn ident_after_dot(&mut self) -> Result<String, Diag> {
        match self.kind() {
            K::Id(s) => {
                self.pos += 1;
                Ok(s.clone())
            }
            _ => Err(unexpected(self.peek(), &["identifier"])),
        }
    }
    fn args(&mut self, sub: fn(&mut Self) -> R) -> Result<Vec<Expr>, Diag> {
        let mut v = vec![];
        if self.eat(&K::RParen) {
            return Ok(v);
        }
        loop {
            v.push(sub(self)?);
            if !self.eat(&K::Comma) {
                self.expect(K::RParen, &["`,`", "`)`"])?;
                return Ok(v);
            }
        }
    }
    /// Postfix loop shared by recursive descent and precedence climbing; `sub` parses a full sub-expression with THAT method.
    fn suffixes(&mut self, mut base: Expr, sub: fn(&mut Self) -> R) -> R {
        loop {
            match self.kind() {
                K::LParen => {
                    self.pos += 1;
                    let args = self.args(sub)?;
                    base = Expr::Call(Box::new(base), args);
                }
                K::LBracket => {
                    self.pos += 1;
                    let i = sub(self)?;
                    self.expect(K::RBracket, &["`]`"])?;
                    base = Expr::Index(Box::new(base), Box::new(i));
                }
                K::Dot => {
                    self.pos += 1;
                    base = Expr::Member(Box::new(base), self.ident_after_dot()?);
                }
                _ => return Ok(base),
            }
        }
    }
    fn leaf(&mut self) -> Option<Expr> {
        let e = match self.kind() {
            K::Num(n) => Expr::Num(*n),
            K::Id(s) => Expr::Var(s.clone()),
            _ => return None,
        };
        self.pos += 1;
        Some(e)
    }
}

// SECTION-BEGIN recursive descent
impl<'a> P<'a> {
    fn rd_expr(&mut self) -> R {
        self.guarded(|p| p.rd_ternary())
    }
    fn rd_ternary(&mut self) -> R {
        let c = self.rd_or()?;
        if !self.eat(&K::Quest) {
            return Ok(c);
        }
        let t = self.rd_expr()?;
        self.expect(K::Colon, &["`:`"])?;
        let f = self.rd_expr()?; // right associativity = recursion on the right
        Ok(Expr::Tern(Box::new(c), Box::new(t), Box::new(f)))
    }
    fn rd_or(&mut self) -> R {
        let mut l = self.rd_and()?;
        while self.eat(&K::OrOr) {
            l = bin(BinOp::Or, l, self.rd_and()?); // left associativity = loop + build inside the loop
        }
        Ok(l)
    }
    fn rd_and(&mut self) -> R {
        let mut l = self.rd_cmp()?;
        while self.eat(&K::AndAnd) {
            l = bin(BinOp::And, l, self.rd_cmp()?);
        }
        Ok(l)
    }
    fn rd_cmp(&mut self) -> R {
        let l = self.rd_add()?;
        let Some(op) = bin_of(self.kind()).filter(|o| o.is_cmp()) else {
            return Ok(l);
        };
        self.pos += 1;
        let r = self.rd_add()?;
        if bin_of(self.kind()).is_some_and(|o| o.is_cmp()) {
            return Err(chain_err(self.peek())); // `?` not `*` in the grammar: a second comparison is an error
        }
        Ok(bin(op, l, r))
    }
    fn rd_add(&mut self) -> R {
        let mut l = self.rd_mul()?;
        while let Some(op) = bin_of(self.kind()).filter(|o| matches!(o, BinOp::Add | BinOp::Sub)) {
            self.pos += 1;
            l = bin(op, l, self.rd_mul()?);
        }
        Ok(l)
    }
    fn rd_mul(&mut self) -> R {
        let mut l = self.rd_unary()?;
        while let Some(op) =
            bin_of(self.kind()).filter(|o| matches!(o, BinOp::Mul | BinOp::Div | BinOp::Rem))
        {
            self.pos += 1;
            l = bin(op, l, self.rd_unary()?);
        }
        Ok(l)
    }
    fn rd_unary(&mut self) -> R {
        match un_of(self.kind()) {
            Some(op) => {
                self.pos += 1;
                Ok(un(op, self.guarded(|p| p.rd_unary())?))
            }
            None => self.rd_power(),
        }
    }
    fn rd_power(&mut self) -> R {
        let base = self.rd_postfix()?;
        if !self.eat(&K::StarStar) {
            return Ok(base);
        }
        Ok(bin(BinOp::Pow, base, self.guarded(|p| p.rd_unary())?))
    }
    fn rd_postfix(&mut self) -> R {
        let base = self.rd_primary()?;
        self.suffixes(base, P::rd_expr)
    }
    fn rd_primary(&mut self) -> R {
        if let Some(e) = self.leaf() {
            return Ok(e);
        }
        if self.eat(&K::LParen) {
            let e = self.rd_expr()?;
            self.expect(K::RParen, &["`)`"])?;
            return Ok(e);
        }
        Err(unexpected(self.peek(), ATOM_EXPECTED))
    }
}
fn m_rd(t: &[Token], pos: usize) -> Result<(Expr, usize), Diag> {
    let mut p = P::new(t, pos);
    let e = p.rd_expr()?;
    Ok((e, p.pos))
}
// SECTION-END recursive descent

// SECTION-BEGIN precedence climbing
impl<'a> P<'a> {
    fn pc_full(&mut self) -> R {
        self.pc_expr(0)
    }
    /// Parse operators with precedence >= `min`. Left assoc: recurse with prec+1; right assoc: with prec; non-assoc: remember it.
    fn pc_expr(&mut self, min: u8) -> R {
        self.guarded(|p| p.pc_loop(min))
    }
    fn pc_loop(&mut self, min: u8) -> R {
        let mut lhs = self.pc_atom()?;
        let mut none_prec = 0u8; // precedence of the non-associative operator we just applied (0 = none)
        loop {
            let (prec, op) = match self.kind() {
                K::Quest => (1, None), // the ternary is just another (right associative) operator in the table
                k => match bin_of(k) {
                    Some(op) => (op.prec(), Some(op)),
                    None => break,
                },
            };
            if prec < min {
                break;
            }
            if prec == none_prec {
                return Err(chain_err(self.peek()));
            }
            self.pos += 1;
            match op {
                None => {
                    let t = self.pc_expr(0)?;
                    self.expect(K::Colon, &["`:`"])?;
                    lhs = Expr::Tern(Box::new(lhs), Box::new(t), Box::new(self.pc_expr(1)?));
                    none_prec = 0;
                }
                Some(op) => {
                    let next = if op == BinOp::Pow { prec } else { prec + 1 };
                    lhs = bin(op, lhs, self.pc_expr(next)?);
                    none_prec = if op.is_cmp() { prec } else { 0 };
                }
            }
        }
        Ok(lhs)
    }
    fn pc_atom(&mut self) -> R {
        if let Some(op) = un_of(self.kind()) {
            self.pos += 1;
            // Operand binds at level 7, which still admits `**` (8): that is exactly why `-2 ** 2` == -(2 ** 2).
            return Ok(un(op, self.pc_expr(7)?));
        }
        let prim = if let Some(e) = self.leaf() {
            e
        } else if self.eat(&K::LParen) {
            let e = self.pc_expr(0)?;
            self.expect(K::RParen, &["`)`"])?;
            e
        } else {
            return Err(unexpected(self.peek(), ATOM_EXPECTED));
        };
        self.suffixes(prim, P::pc_full)
    }
}
fn m_pc(t: &[Token], pos: usize) -> Result<(Expr, usize), Diag> {
    let mut p = P::new(t, pos);
    let e = p.pc_expr(0)?;
    Ok((e, p.pos))
}
// SECTION-END precedence climbing

// SECTION-BEGIN pratt
const TERNARY_BP: (u8, u8) = (10, 9); // right < left  => right associative
const PREFIX_BP: u8 = 70; // binds looser than `**` (80) so `-a ** b` = -(a ** b), tighter than `*` (60)
const POSTFIX_BP: u8 = 90;
fn infix_bp(op: BinOp) -> (u8, u8) {
    match op {
        BinOp::Or => (20, 21),
        BinOp::And => (30, 31),
        o if o.is_cmp() => (40, 41), // non-associative handled by the `last_cmp` flag
        BinOp::Add | BinOp::Sub => (50, 51),
        BinOp::Mul | BinOp::Div | BinOp::Rem => (60, 61),
        _ => (80, 79),
    }
}
impl<'a> P<'a> {
    fn pratt_full(&mut self) -> R {
        self.pratt_expr(0)
    }
    fn pratt_expr(&mut self, min_bp: u8) -> R {
        self.guarded(|p| p.pratt_loop(min_bp))
    }
    fn pratt_loop(&mut self, min_bp: u8) -> R {
        // 1. the "nud": whatever can START an expression (literals, groups, prefix operators)
        let mut lhs = if let Some(e) = self.leaf() {
            e
        } else if let Some(op) = un_of(self.kind()) {
            self.pos += 1;
            un(op, self.pratt_expr(PREFIX_BP)?)
        } else if self.eat(&K::LParen) {
            let e = self.pratt_expr(0)?;
            self.expect(K::RParen, &["`)`"])?;
            e
        } else {
            return Err(unexpected(self.peek(), ATOM_EXPECTED));
        };
        // 2. the "led" loop: postfix and infix operators that bind at least as tightly as the caller allows
        let mut last_cmp = false;
        loop {
            let k = self.kind();
            if matches!(k, K::LParen | K::LBracket | K::Dot) {
                if POSTFIX_BP < min_bp {
                    break;
                }
                self.pos += 1;
                lhs = match k {
                    K::LParen => Expr::Call(Box::new(lhs), self.args(P::pratt_full)?),
                    K::LBracket => {
                        let i = self.pratt_expr(0)?;
                        self.expect(K::RBracket, &["`]`"])?;
                        Expr::Index(Box::new(lhs), Box::new(i))
                    }
                    _ => Expr::Member(Box::new(lhs), self.ident_after_dot()?),
                };
                last_cmp = false;
                continue;
            }
            let op = bin_of(k);
            let (lbp, rbp) = match (k, op) {
                (K::Quest, _) => TERNARY_BP,
                (_, Some(op)) => infix_bp(op),
                _ => break,
            };
            if lbp < min_bp {
                break;
            }
            let is_cmp = op.is_some_and(|o| o.is_cmp());
            if is_cmp && last_cmp {
                return Err(chain_err(self.peek()));
            }
            self.pos += 1;
            lhs = match op {
                None => {
                    let t = self.pratt_expr(0)?; // between `?` and `:` anything goes (like inside parentheses)
                    self.expect(K::Colon, &["`:`"])?;
                    Expr::Tern(Box::new(lhs), Box::new(t), Box::new(self.pratt_expr(rbp)?))
                }
                Some(op) => bin(op, lhs, self.pratt_expr(rbp)?),
            };
            last_cmp = is_cmp;
        }
        Ok(lhs)
    }
}
fn m_pratt(t: &[Token], pos: usize) -> Result<(Expr, usize), Diag> {
    let mut p = P::new(t, pos);
    let e = p.pratt_expr(0)?;
    Ok((e, p.pos))
}
// SECTION-END pratt

// SECTION-BEGIN shunting yard
#[derive(Clone, Debug, PartialEq)]
enum Rpn {
    Num(i64),
    Var(String),
    Un(UnOp),
    Bin(BinOp),
    Tern,
    Call(usize),
    Index,
    Member(String),
}
#[derive(Clone, Copy)]
enum Item {
    Un(UnOp),
    Bin(BinOp),
}
impl Item {
    fn prec(self) -> u8 {
        match self {
            Item::Un(_) => 7,
            Item::Bin(o) => o.prec(),
        }
    }
    fn rpn(self) -> Rpn {
        match self {
            Item::Un(o) => Rpn::Un(o),
            Item::Bin(o) => Rpn::Bin(o),
        }
    }
}
/// Operator-stack entries. `Quest`/`Colon` are the two halves of a ternary; the bracket kinds remember what to emit on close.
enum Fr {
    Op(Item),
    Quest,
    Colon,
    Paren,
    Call { commas: usize, open: usize },
    Bracket,
}
fn closer_expected(f: &Fr) -> &'static [&'static str] {
    match f {
        Fr::Bracket => &["`]`"],
        Fr::Call { .. } => &["`,`", "`)`"],
        _ => &["`)`"],
    }
}
/// Pop operators (and finished ternaries) to the output until a bracket frame or the bottom; an open `?` is an error.
fn unwind(st: &mut Vec<Fr>, out: &mut Vec<Rpn>, t: &Token) -> Result<Option<Fr>, Diag> {
    while let Some(f) = st.pop() {
        match f {
            Fr::Op(i) => out.push(i.rpn()),
            Fr::Colon => out.push(Rpn::Tern),
            Fr::Quest => return Err(unexpected(t, &["`:`"])),
            bracket => return Ok(Some(bracket)),
        }
    }
    Ok(None)
}
/// Pure iteration, no recursion: nesting lives in the explicit `st` Vec, so depth is bounded only by memory.
fn shunt(toks: &[Token], start: usize) -> Result<(Vec<Rpn>, usize), Diag> {
    let (mut out, mut st, mut i, mut want_operand) = (Vec::new(), Vec::<Fr>::new(), start, true);
    loop {
        let t = &toks[i.min(toks.len() - 1)];
        if want_operand {
            match &t.kind {
                K::Num(n) => {
                    out.push(Rpn::Num(*n));
                    want_operand = false;
                }
                K::Id(s) => {
                    out.push(Rpn::Var(s.clone()));
                    want_operand = false;
                }
                // `f()`: a `)` right after a call's `(` closes an empty argument list
                K::RParen if matches!(st.last(), Some(Fr::Call { open, .. }) if *open + 1 == i) => {
                    st.pop();
                    out.push(Rpn::Call(0));
                    want_operand = false;
                }
                K::LParen => st.push(Fr::Paren),
                k if un_of(k).is_some() => st.push(Fr::Op(Item::Un(un_of(k).unwrap_or(UnOp::Neg)))),
                _ => return Err(unexpected(t, ATOM_EXPECTED)),
            }
            i += 1;
            continue;
        }
        match &t.kind {
            k if bin_of(k).is_some() => {
                let op = bin_of(k).unwrap_or(BinOp::Add);
                while let Some(Fr::Op(top)) = st.last() {
                    let (tp, cp) = (top.prec(), op.prec());
                    if tp == cp && op.is_cmp() {
                        return Err(chain_err(t));
                    }
                    // pop stronger operators; equal ones too when left associative (`**` is the only right-assoc one)
                    if tp > cp || (tp == cp && op != BinOp::Pow) {
                        out.push(top.rpn());
                        st.pop();
                    } else {
                        break;
                    }
                }
                st.push(Fr::Op(Item::Bin(op)));
                want_operand = true;
            }
            K::Quest => {
                while let Some(Fr::Op(top)) = st.last() {
                    out.push(top.rpn());
                    st.pop();
                }
                st.push(Fr::Quest);
                want_operand = true;
            }
            K::Colon => {
                let mut found = false;
                while let Some(f) = st.pop() {
                    match f {
                        Fr::Op(it) => out.push(it.rpn()),
                        Fr::Colon => out.push(Rpn::Tern), // inner else-branch ended
                        Fr::Quest => {
                            st.push(Fr::Colon);
                            found = true;
                            break;
                        }
                        b => return Err(unexpected(t, closer_expected(&b))),
                    }
                }
                if !found {
                    return Ok((out, i)); // a `:` that belongs to nobody ends the expression; the caller reports it
                }
                want_operand = true;
            }
            K::LParen => {
                st.push(Fr::Call { commas: 0, open: i });
                want_operand = true;
            }
            K::LBracket => {
                st.push(Fr::Bracket);
                want_operand = true;
            }
            K::Dot => match toks.get(i + 1).map(|n| (&n.kind, n)) {
                Some((K::Id(s), _)) => {
                    out.push(Rpn::Member(s.clone()));
                    i += 1;
                }
                Some((_, n)) => return Err(unexpected(n, &["identifier"])),
                None => return Err(unexpected(t, &["identifier"])),
            },
            K::RParen => match unwind(&mut st, &mut out, t)? {
                None => return Ok((out, i)),
                Some(Fr::Paren) => {}
                Some(Fr::Call { commas, open }) => {
                    out.push(Rpn::Call(if i == open + 1 { 0 } else { commas + 1 }))
                }
                Some(b) => return Err(unexpected(t, closer_expected(&b))),
            },
            K::RBracket => match unwind(&mut st, &mut out, t)? {
                None => return Ok((out, i)),
                Some(Fr::Bracket) => out.push(Rpn::Index),
                Some(b) => return Err(unexpected(t, closer_expected(&b))),
            },
            K::Comma => match unwind(&mut st, &mut out, t)? {
                None => return Ok((out, i)),
                Some(Fr::Call { commas, open }) => {
                    st.push(Fr::Call {
                        commas: commas + 1,
                        open,
                    });
                    want_operand = true;
                }
                Some(b) => return Err(unexpected(t, closer_expected(&b))),
            },
            _ => {
                // Anything else ends the expression; it is only legal if no bracket is still open.
                return match unwind(&mut st, &mut out, t)? {
                    None => Ok((out, i)),
                    Some(b) => Err(unexpected(t, closer_expected(&b))),
                };
            }
        }
        i += 1;
    }
}
/// RPN -> tree with an explicit stack (so the result of the iterative parse is comparable with the other methods).
fn rpn_to_expr(rpn: &[Rpn]) -> Option<Expr> {
    let mut st: Vec<Expr> = vec![];
    for item in rpn {
        let e = match item {
            Rpn::Num(n) => Expr::Num(*n),
            Rpn::Var(v) => Expr::Var(v.clone()),
            Rpn::Un(o) => un(*o, st.pop()?),
            Rpn::Bin(o) => {
                let r = st.pop()?;
                bin(*o, st.pop()?, r)
            }
            Rpn::Tern => {
                let (f, t) = (st.pop()?, st.pop()?);
                Expr::Tern(Box::new(st.pop()?), Box::new(t), Box::new(f))
            }
            Rpn::Call(n) => {
                let args = st.split_off(st.len().checked_sub(*n)?);
                Expr::Call(Box::new(st.pop()?), args)
            }
            Rpn::Index => {
                let i = st.pop()?;
                Expr::Index(Box::new(st.pop()?), Box::new(i))
            }
            Rpn::Member(n) => Expr::Member(Box::new(st.pop()?), n.clone()),
        };
        st.push(e);
    }
    let e = st.pop()?;
    st.is_empty().then_some(e)
}
fn m_shunt(t: &[Token], pos: usize) -> Result<(Expr, usize), Diag> {
    let (rpn, end) = shunt(t, pos)?;
    let e = rpn_to_expr(&rpn).ok_or_else(|| Diag {
        span: t[pos].span,
        message: "internal: malformed RPN".into(),
        expected: vec![],
    })?;
    Ok((e, end))
}
fn rpn_string(rpn: &[Rpn]) -> String {
    let words: Vec<String> = rpn
        .iter()
        .map(|r| match r {
            Rpn::Num(n) => n.to_string(),
            Rpn::Var(v) => v.clone(),
            Rpn::Un(o) => {
                if *o == UnOp::Neg {
                    "neg".into()
                } else {
                    "not".into()
                }
            }
            Rpn::Bin(o) => o.sym().into(),
            Rpn::Tern => "?:".into(),
            Rpn::Call(n) => format!("call/{n}"),
            Rpn::Index => "[]".into(),
            Rpn::Member(m) => format!(".{m}"),
        })
        .collect();
    words.join(" ")
}
// SECTION-END shunting yard

const METHODS: [(&str, Method); 4] = [
    ("recursive descent", m_rd),
    ("precedence climbing", m_pc),
    ("pratt", m_pratt),
    ("shunting-yard", m_shunt),
];

// ---------- drivers: single expression, and a `;`-separated program with error recovery ----------
fn parse_tokens(toks: &[Token], m: Method) -> R {
    let (e, p) = m(toks, 0)?;
    match toks[p].kind {
        K::Eof => Ok(e),
        _ => Err(unexpected(&toks[p], &["operator", "end of input"])),
    }
}
fn parse_one(src: &str, m: Method) -> R {
    let (toks, lex_diags) = lex(src);
    match lex_diags.into_iter().next() {
        Some(d) => Err(d),
        None => parse_tokens(&toks, m),
    }
}

/// Panic-mode recovery: after a bad statement skip to the next `;` and resume, so we can report many errors at once.
fn sync(toks: &[Token], mut pos: usize) -> usize {
    while !matches!(toks[pos].kind, K::Semi | K::Eof) {
        pos += 1; // stray `)` / `]` are skipped here too
    }
    if toks[pos].kind == K::Semi {
        pos + 1
    } else {
        pos
    }
}
fn parse_program(src: &str, m: Method) -> (Vec<Expr>, Vec<Diag>) {
    let (toks, lex_diags) = lex(src);
    let mut diags = lex_diags.clone();
    let (mut pos, mut out) = (0, vec![]);
    while toks[pos].kind != K::Eof {
        let result = m(&toks, pos).and_then(|(e, p)| match toks[p].kind {
            K::Semi | K::Eof => Ok((e, p)),
            _ => Err(unexpected(&toks[p], TRAILING_EXPECTED)),
        });
        let stmt_end = match result {
            Ok((e, p)) => {
                out.push(e);
                p
            }
            Err(d) => {
                let end = sync(&toks, pos);
                // A lexer error inside this statement already explained the damage: drop the cascading parse error.
                let lo = if pos == 0 { 0 } else { toks[pos - 1].span.end };
                let hi = toks[end.min(toks.len() - 1)].span.start;
                if !lex_diags
                    .iter()
                    .any(|l| l.span.start >= lo && l.span.start < hi.max(lo + 1))
                {
                    diags.push(d);
                }
                end
            }
        };
        pos = if toks[stmt_end].kind == K::Semi {
            stmt_end + 1
        } else {
            stmt_end
        };
    }
    diags.sort_by_key(|d| d.span.start);
    (out, diags)
}

// ---------- evaluation: tree walk (reference) and RPN stack machine ----------
#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i64),
    Func(&'static str),
}
const FUNCS: [&str; 4] = ["min", "max", "abs", "sum"];
const MEMBERS: [&str; 4] = ["sq", "neg", "abs", "half"];
const VARS: [&str; 5] = ["a", "b", "c", "d", "e"];

type Res = Result<Val, String>;
fn lookup(name: &str) -> Res {
    if let Some(f) = FUNCS.iter().find(|f| **f == name) {
        return Ok(Val::Func(f));
    }
    match name {
        "a" => Ok(Val::Int(3)),
        "b" => Ok(Val::Int(-5)),
        "c" => Ok(Val::Int(7)),
        "d" => Ok(Val::Int(2)),
        "e" => Ok(Val::Int(12)),
        _ => Err(format!("undefined variable `{name}`")),
    }
}
fn int(v: Val) -> Result<i64, String> {
    match v {
        Val::Int(n) => Ok(n),
        Val::Func(f) => Err(format!("function `{f}` used as a number")),
    }
}
fn do_bin(op: BinOp, l: Val, r: Val) -> Res {
    let (l, r) = (int(l)?, int(r)?);
    Ok(Val::Int(match op {
        BinOp::Or => (l != 0 || r != 0) as i64,
        BinOp::And => (l != 0 && r != 0) as i64,
        BinOp::Eq => (l == r) as i64,
        BinOp::Ne => (l != r) as i64,
        BinOp::Lt => (l < r) as i64,
        BinOp::Le => (l <= r) as i64,
        BinOp::Gt => (l > r) as i64,
        BinOp::Ge => (l >= r) as i64,
        BinOp::Add => l.wrapping_add(r),
        BinOp::Sub => l.wrapping_sub(r),
        BinOp::Mul => l.wrapping_mul(r),
        BinOp::Div | BinOp::Rem if r == 0 => return Err("division by zero".into()),
        BinOp::Div => l.wrapping_div(r),
        BinOp::Rem => l.wrapping_rem(r),
        BinOp::Pow if r < 0 => return Err("negative exponent".into()),
        BinOp::Pow => l.wrapping_pow(r.min(u32::MAX as i64) as u32),
    }))
}
fn do_un(op: UnOp, v: Val) -> Res {
    let n = int(v)?;
    Ok(Val::Int(if op == UnOp::Neg {
        n.wrapping_neg()
    } else {
        (n == 0) as i64
    }))
}
fn do_member(v: Val, name: &str) -> Res {
    let x = int(v)?;
    match name {
        "sq" => Ok(Val::Int(x.wrapping_mul(x))),
        "neg" => Ok(Val::Int(x.wrapping_neg())),
        "abs" => Ok(Val::Int(x.wrapping_abs())),
        "half" => Ok(Val::Int(x / 2)),
        _ => Err(format!("no member `{name}`")),
    }
}
fn do_index(v: Val, i: Val) -> Res {
    let (x, i) = (int(v)?, int(i)?);
    Ok(Val::Int((x >> i.rem_euclid(63)) & 1)) // bit `i` of x (indices wrap mod 63 so it is total)
}
fn do_call(callee: Val, args: Vec<Val>) -> Res {
    let Val::Func(name) = callee else {
        return Err("value is not callable".into());
    };
    let ns = args.into_iter().map(int).collect::<Result<Vec<_>, _>>()?;
    let r = match (name, ns.as_slice()) {
        ("abs", [x]) => x.wrapping_abs(),
        ("min", [first, rest @ ..]) => rest.iter().fold(*first, |m, x| m.min(*x)),
        ("max", [first, rest @ ..]) => rest.iter().fold(*first, |m, x| m.max(*x)),
        ("sum", xs) => xs.iter().fold(0i64, |s, x| s.wrapping_add(*x)),
        _ => {
            return Err(format!(
                "wrong number of arguments ({}) for `{name}`",
                ns.len()
            ));
        }
    };
    Ok(Val::Int(r))
}
fn do_tern(c: Val, t: Val, f: Val) -> Res {
    Ok(if int(c)? != 0 { t } else { f })
}

fn eval(e: &Expr) -> Res {
    match e {
        Expr::Num(n) => Ok(Val::Int(*n)),
        Expr::Var(v) => lookup(v),
        Expr::Un(op, x) => do_un(*op, eval(x)?),
        Expr::Bin(op, l, r) => {
            let l = eval(l)?;
            do_bin(*op, l, eval(r)?)
        }
        Expr::Tern(c, t, f) => {
            let (c, t) = (eval(c)?, eval(t)?);
            do_tern(c, t, eval(f)?)
        }
        Expr::Call(c, args) => {
            let c = eval(c)?;
            do_call(c, args.iter().map(eval).collect::<Result<Vec<_>, _>>()?)
        }
        Expr::Index(b, i) => {
            let b = eval(b)?;
            do_index(b, eval(i)?)
        }
        Expr::Member(b, n) => do_member(eval(b)?, n),
    }
}
fn eval_rpn(rpn: &[Rpn]) -> Res {
    let mut st: Vec<Val> = vec![];
    let under = || "stack underflow".to_string();
    for item in rpn {
        let v = match item {
            Rpn::Num(n) => Val::Int(*n),
            Rpn::Var(v) => lookup(v)?,
            Rpn::Un(o) => do_un(*o, st.pop().ok_or_else(under)?)?,
            Rpn::Bin(o) => {
                let r = st.pop().ok_or_else(under)?;
                do_bin(*o, st.pop().ok_or_else(under)?, r)?
            }
            Rpn::Tern => {
                let (f, t) = (st.pop().ok_or_else(under)?, st.pop().ok_or_else(under)?);
                do_tern(st.pop().ok_or_else(under)?, t, f)?
            }
            Rpn::Call(n) => {
                let args = st.split_off(st.len().checked_sub(*n).ok_or_else(under)?);
                do_call(st.pop().ok_or_else(under)?, args)?
            }
            Rpn::Index => {
                let i = st.pop().ok_or_else(under)?;
                do_index(st.pop().ok_or_else(under)?, i)?
            }
            Rpn::Member(n) => do_member(st.pop().ok_or_else(under)?, n)?,
        };
        st.push(v);
    }
    st.pop().ok_or_else(under)
}

// ---------- seeded random programs from the grammar ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

fn gen_leaf(r: &mut Rng) -> Expr {
    if r.below(2) == 0 {
        Expr::Num(r.below(10) as i64)
    } else {
        Expr::Var(VARS[r.below(VARS.len())].to_string())
    }
}
fn gen_box(r: &mut Rng, d: u32) -> Box<Expr> {
    Box::new(gen_expr(r, d))
}
fn gen_expr(r: &mut Rng, d: u32) -> Expr {
    if d == 0 || r.below(100) < 15 {
        return gen_leaf(r);
    }
    let d = d - 1;
    match r.below(100) {
        0..=49 => Expr::Bin(
            BIN_OPS[r.below(BIN_OPS.len())],
            gen_box(r, d),
            gen_box(r, d),
        ),
        50..=62 => Expr::Un(
            if r.below(2) == 0 {
                UnOp::Neg
            } else {
                UnOp::Not
            },
            gen_box(r, d),
        ),
        63..=72 => Expr::Tern(gen_box(r, d), gen_box(r, d), gen_box(r, d)),
        73..=80 => {
            let f = FUNCS[r.below(FUNCS.len())];
            let n = match f {
                "abs" => 1,
                "sum" => r.below(4),
                _ => 1 + r.below(3),
            };
            Expr::Call(
                Box::new(Expr::Var(f.into())),
                (0..n).map(|_| gen_expr(r, d)).collect(),
            )
        }
        81..=88 => Expr::Index(gen_box(r, d), gen_box(r, d)),
        _ => Expr::Member(gen_box(r, d), MEMBERS[r.below(MEMBERS.len())].into()),
    }
}

/// Mutate a valid source into (usually) invalid input: delete, insert, replace, swap or duplicate a slice.
fn mutate(r: &mut Rng, src: &str) -> String {
    const ALPHABET: &[&str] = &[
        "(",
        ")",
        "[",
        "]",
        "+",
        "-",
        "*",
        "**",
        "/",
        "%",
        "&",
        "|",
        "!",
        "=",
        "<",
        ">",
        "?",
        ":",
        ",",
        ".",
        ";",
        "a",
        "1",
        " ",
        "$",
        "\u{e9}",
        "\u{20ac}",
        "99999999999999999999",
    ];
    let mut cs: Vec<char> = src.chars().collect();
    for _ in 0..1 + r.below(3) {
        if cs.is_empty() {
            cs.push('1');
        }
        let i = r.below(cs.len());
        match r.below(5) {
            0 => {
                cs.remove(i);
            }
            1 | 2 => {
                let ins: Vec<char> = ALPHABET[r.below(ALPHABET.len())].chars().collect();
                cs.splice(i..i, ins);
            }
            3 => {
                let j = r.below(cs.len());
                cs.swap(i, j);
            }
            _ => {
                let end = (i + 1 + r.below(4)).min(cs.len());
                let dup: Vec<char> = cs[i..end].to_vec();
                cs.splice(i..i, dup);
            }
        }
    }
    cs.into_iter().collect()
}

// ---------- tests ----------
fn ok_str(src: &str, m: Method) -> String {
    match parse_one(src, m) {
        Ok(e) => full(&e),
        Err(d) => format!("ERR@{}..{}", d.span.start, d.span.end),
    }
}

fn check_tables() {
    let table: &[(&str, &str)] = &[
        ("a - b - c", "((a - b) - c)"),
        ("a / b * c % d", "(((a / b) * c) % d)"),
        ("a ** b ** c", "(a ** (b ** c))"),
        ("a ? b : c ? d : e", "(a ? b : (c ? d : e))"),
        ("a ? b ? c : d : e", "(a ? (b ? c : d) : e)"),
        ("-2 ** 2", "(-(2 ** 2))"),
        ("(-2) ** 2", "((-2) ** 2)"),
        ("2 ** -2", "(2 ** (-2))"),
        ("2 ** -a ** 3", "(2 ** (-(a ** 3)))"),
        ("a + b * c - d", "((a + (b * c)) - d)"),
        ("a || b && c || d", "((a || (b && c)) || d)"),
        ("!a && b == c", "((!a) && (b == c))"),
        ("a || b ? c : d", "((a || b) ? c : d)"),
        ("a ? b : c || d", "(a ? b : (c || d))"),
        ("a < b == (c < d)", "ERR"),
        ("-a.b[1](2)", "(-a.b[1](2))"),
        ("f(a, b + 1)(c)[d].e", "f(a, (b + 1))(c)[d].e"),
        ("f()", "f()"),
        ("- - a", "(-(-a))"),
        ("a ? b + 1 : c * 2", "(a ? (b + 1) : (c * 2))"),
        ("x.y ** 2", "(x.y ** 2)"),
    ];
    for (src, want) in table {
        for (name, m) in METHODS {
            let got = ok_str(src, m);
            if *want == "ERR" {
                assert!(
                    got.starts_with("ERR"),
                    "{name}: `{src}` should be rejected, got {got}"
                );
            } else {
                assert_eq!(&got, want, "{name}: `{src}`");
            }
        }
    }
    // evaluation oracle values (a=3 b=-5 c=7 d=2 e=12)
    let evals: &[(&str, i64)] = &[
        ("-2 ** 2", -4),
        ("2 ** 3 ** 2", 512),
        ("10 - 4 - 3", 3),
        ("a ? b : c", -5),
        ("0 ? 1 : 0 ? 2 : 3", 3),
        ("max(a, b, c) + min(d, e)", 9),
        ("e.half ** 2 % 5", 1),
        ("5[0] + 5[1] + 5[2]", 2),
        ("2 ** -1 + 0", -1), // negative exponent -> error, see below; placeholder replaced by the explicit check
    ];
    for (src, want) in &evals[..evals.len() - 1] {
        let e = parse_one(src, m_pratt).expect("parses");
        assert_eq!(eval(&e), Ok(Val::Int(*want)), "{src}");
    }
    let e = parse_one("2 ** -1", m_pratt).expect("parses");
    assert_eq!(eval(&e), Err("negative exponent".into()));
    let e = parse_one("1 / (a - 3)", m_pratt).expect("parses");
    assert_eq!(eval(&e), Err("division by zero".into()));
}

fn check_error_spans() {
    // (source, expected error span as (start, end), message fragment)
    let cases: &[(&str, (usize, usize), &str)] = &[
        ("1 + * 2", (4, 5), "unexpected `*`"),
        ("a < b < c", (6, 7), "non-associative"),
        ("a == b != c", (7, 9), "non-associative"),
        ("(1 + 2", (6, 6), "end of input"),
        ("1 + ", (4, 4), "end of input"),
        ("a ? b", (5, 5), "end of input"),
        ("a ? b ) c", (6, 7), "unexpected `)`"),
        ("f(a,)", (4, 5), "unexpected `)`"),
        ("f(a b)", (4, 5), "identifier `b`"),
        ("a[1", (3, 3), "end of input"),
        ("a.", (2, 2), "end of input"),
        ("a.5", (2, 3), "number `5`"),
        ("1 2", (2, 3), "number `2`"),
        ("a : b", (2, 3), "unexpected `:`"),
        ("1 $ 2", (2, 3), "unexpected character `$`"),
        ("a = b", (2, 3), "did you mean `==`"),
        ("99999999999999999999", (0, 20), "too large"),
        ("1 + \u{e9}", (4, 6), "unexpected character"),
    ];
    for (src, (s, e), frag) in cases {
        for (name, m) in METHODS {
            let d = parse_one(src, m).expect_err(&format!("{name}: `{src}` must fail"));
            assert_eq!(
                (d.span.start, d.span.end),
                (*s, *e),
                "{name}: span for `{src}`"
            );
            assert!(
                d.message.contains(frag),
                "{name}: `{src}` message `{}` lacks `{frag}`",
                d.message
            );
        }
    }
    let d = parse_one("1 + * 2", m_pratt).unwrap_err();
    let shown = render("1 + * 2", &d);
    assert_eq!(
        shown,
        "error: unexpected `*`\n --> 1:5\n  |\n1 | 1 + * 2\n  |     ^ expected number, identifier, `(`, `-`, `!`"
    );
    println!("{shown}");
}

fn check_recovery() {
    let src = "1 + 2;\n(3 * ;\n4 $ 5;\nf(1, 2;\na < b < c;\n6 ** 7";
    println!("-- error recovery: one run, many diagnostics (recursive descent) --");
    let (good, diags) = parse_program(src, m_rd);
    for d in &diags {
        println!("{}", render(src, d));
    }
    let shown: Vec<String> = good.iter().map(full).collect();
    println!("recovered statements: {shown:?}");
    assert_eq!(shown, ["(1 + 2)", "(6 ** 7)"]);
    assert_eq!(
        diags.len(),
        4,
        "one per bad statement: (3 * ; / lexer `$` / f(1, 2; / chained <"
    );
    assert!(diags[1].message.contains("unexpected character `$`"));
    for (name, m) in METHODS {
        let (g2, d2) = parse_program(src, m);
        assert_eq!(g2, good, "{name}: recovered statements");
        let spans = |ds: &[Diag]| ds.iter().map(|d| d.span).collect::<Vec<_>>();
        assert_eq!(spans(&d2), spans(&diags), "{name}: diagnostic spans");
    }
}

fn check_random(n: usize) -> (usize, usize) {
    let mut rng = Rng(0x5EED_CAFE_F00D_1234);
    let (mut ok_evals, mut err_evals) = (0, 0);
    for k in 0..n {
        let tree = gen_expr(&mut rng, 2 + (k % 5) as u32);
        let text = source_of(&tree, 0);
        // 1. every method reproduces the generated tree from minimal-paren source
        for (name, m) in METHODS {
            let got = parse_one(&text, m)
                .unwrap_or_else(|d| panic!("{name} failed on `{text}`: {}", d.message));
            assert_eq!(got, tree, "{name}: AST differs for `{text}`");
        }
        // 2. pretty-print (fully parenthesised) -> parse round trip, even though `full` is a different surface syntax
        let fp = full(&tree);
        for (name, m) in METHODS {
            let again = parse_one(&fp, m)
                .unwrap_or_else(|d| panic!("{name} failed on full `{fp}`: {}", d.message));
            assert_eq!(full(&again), fp, "{name}: round trip");
        }
        // 3. evaluation: tree walk == RPN machine (and both see the same first error)
        let (rpn, _) = shunt(&lex(&text).0, 0).expect("shunt");
        let (a, b) = (eval(&tree), eval_rpn(&rpn));
        assert_eq!(a, b, "eval mismatch for `{text}` rpn={}", rpn_string(&rpn));
        if a.is_ok() {
            ok_evals += 1
        } else {
            err_evals += 1
        }
    }
    (ok_evals, err_evals)
}

fn check_fuzz(n_src: usize) -> (usize, usize, usize) {
    let mut rng = Rng(0xFEED_BEEF_0BAD_5EED);
    let (mut total, mut accepted, mut rejected) = (0, 0, 0);
    for k in 0..n_src {
        let base = source_of(&gen_expr(&mut rng, 1 + (k % 4) as u32), 0);
        for _ in 0..4 {
            let m = mutate(&mut rng, &base);
            total += 1;
            let (toks, lex_diags) = lex(&m);
            let mut results = vec![];
            for (_, f) in METHODS {
                results.push(parse_tokens(&toks, f));
                // Program mode must also never panic and must report in-bounds, char-aligned spans.
                let (_, diags) = parse_program(&m, f);
                for d in &diags {
                    assert!(
                        d.span.start <= d.span.end && d.span.end <= m.len(),
                        "span out of range for `{m}`"
                    );
                    assert!(
                        m.is_char_boundary(d.span.start) && m.is_char_boundary(d.span.end),
                        "span splits a char in `{m}`"
                    );
                    let _ = render(&m, d);
                }
            }
            if lex_diags.is_empty() {
                // all four methods must agree: same AST, or same error span
                match &results[0] {
                    Ok(e) => {
                        accepted += 1;
                        for (r, (name, _)) in results.iter().zip(METHODS) {
                            assert_eq!(r.as_ref().ok(), Some(e), "{name} disagrees on `{m}`");
                        }
                    }
                    Err(d0) => {
                        rejected += 1;
                        for (r, (name, _)) in results.iter().zip(METHODS) {
                            let d = r
                                .as_ref()
                                .err()
                                .unwrap_or_else(|| panic!("{name} accepted `{m}`"));
                            assert_eq!(
                                d.span, d0.span,
                                "{name} error span differs on `{m}`: {} vs {}",
                                d.message, d0.message
                            );
                        }
                    }
                }
            } else {
                rejected += 1;
            }
        }
    }
    (total, accepted, rejected)
}

fn check_depth_and_size() {
    // (1) hostile nesting: the three recursive parsers return an error; shunting-yard just works.
    for depth in [100_000usize, 300] {
        let src = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
        for (name, m) in METHODS {
            let r = parse_one(&src, m);
            match (name, &r) {
                ("shunting-yard", Ok(e)) => assert_eq!(*e, Expr::Num(1)),
                ("shunting-yard", Err(d)) => {
                    panic!("shunting-yard should have no depth limit: {}", d.message)
                }
                (_, Err(d)) => assert!(
                    d.message.contains("nested more than"),
                    "{name}: {}",
                    d.message
                ),
                (_, Ok(_)) => panic!("{name} accepted {depth} nested parens"),
            }
        }
    }
    let ok_src = format!("{}1{}", "(".repeat(100), ")".repeat(100));
    for (name, m) in METHODS {
        assert_eq!(
            parse_one(&ok_src, m),
            Ok(Expr::Num(1)),
            "{name}: 100 nested parens is fine"
        );
    }
    let unary = format!("{}1", "-".repeat(100_000));
    for (name, m) in [METHODS[0], METHODS[1], METHODS[2]] {
        assert!(
            parse_one(&unary, m)
                .unwrap_err()
                .message
                .contains("nested more than"),
            "{name}"
        );
    }
    // (2) 100k-token flat expressions on a deliberately tiny stack: proves left-assoc chains are loops, not recursion.
    let n = 50_000u64;
    let flat: String = (1..=n)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(" + "); // 99 999 tokens
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || {
            let (toks, diags) = lex(&flat);
            assert!(diags.is_empty());
            let want = (n * (n + 1) / 2) as i64;
            let mut nodes = vec![];
            for (name, m) in METHODS {
                let e = parse_tokens(&toks, m).unwrap_or_else(|d| panic!("{name}: {}", d.message));
                nodes.push((name, dismantle(e)));
            }
            let (rpn, _) = shunt(&toks, 0).expect("shunt");
            assert_eq!(eval_rpn(&rpn), Ok(Val::Int(want)));
            (toks.len(), nodes)
        })
        .expect("spawn");
    let (ntoks, nodes) = handle.join().expect("no stack overflow");
    assert!(ntoks > 99_000 && nodes.iter().all(|(_, c)| *c == 2 * n as usize - 1));
    println!(
        "flat 1+2+...+{n}: {ntoks} tokens, {} AST nodes in each method, parsed on a 512 KiB stack, sum = {}",
        nodes[0].1,
        n * (n + 1) / 2
    );
    // (3) a right-assoc `**` chain 100k long: recursive parsers refuse politely, shunting-yard builds it.
    let pows = vec!["1"; 100_000].join(" ** ");
    assert!(
        parse_one(&pows, m_pratt)
            .unwrap_err()
            .message
            .contains("nested more than")
    );
    let e = parse_one(&pows, m_shunt).expect("shunt handles any depth");
    assert_eq!(dismantle(e), 199_999);
}

fn loc(tag: &str) -> usize {
    let me = include_str!("expression_parsing_three_ways.rs");
    let (begin, end) = (
        format!("// SECTION-BEGIN {tag}"),
        format!("// SECTION-END {tag}"),
    );
    let mut on = false;
    let mut n = 0;
    for l in me.lines() {
        if l.trim_start() == end {
            on = false;
        }
        if on && !l.trim().is_empty() && !l.trim_start().starts_with("//") {
            n += 1;
        }
        if l.trim_start() == begin {
            on = true;
        }
    }
    n
}

fn bench() {
    let mut rng = Rng(42);
    let sources: Vec<String> = (0..2000)
        .map(|k| source_of(&gen_expr(&mut rng, 2 + (k % 5) as u32), 0))
        .collect();
    let toks: Vec<Vec<Token>> = sources.iter().map(|s| lex(s).0).collect();
    let total_tokens: usize = toks.iter().map(Vec::len).sum();
    println!(
        "\n-- comparison ({} expressions, {total_tokens} tokens, 20 rounds each) --",
        sources.len()
    );
    println!(
        "{:<20} {:>5} {:>10} {:>13}  {:<34} {}",
        "method", "LOC", "ns/token", "recursion", "extensibility", "error-message quality"
    );
    let notes = [
        (
            "recursive descent",
            "recursive descent",
            "new operator = new function",
            "best: each rule knows what it expects",
        ),
        (
            "precedence climbing",
            "precedence climbing",
            "new operator = new table row",
            "good; expected sets are generic",
        ),
        (
            "pratt",
            "pratt",
            "rows + prefix/postfix/mixfix",
            "good; same hooks as recursive descent",
        ),
        (
            "shunting-yard",
            "shunting yard",
            "ops need stack-rule thought",
            "weakest: errors found via stack state",
        ),
    ];
    for ((name, m), (_, tag, ext, err)) in METHODS.iter().zip(notes) {
        let t0 = Instant::now();
        let mut sink = 0usize;
        for _ in 0..20 {
            for t in &toks {
                sink += m(t, 0).map(|(_, p)| p).unwrap_or(0);
            }
        }
        let ns = t0.elapsed().as_nanos() as f64 / (20 * total_tokens) as f64;
        let rec = if *name == "shunting-yard" {
            "none"
        } else {
            "depth-guarded"
        };
        println!(
            "{name:<20} {:>5} {ns:>10.1} {rec:>13}  {ext:<34} {err}",
            loc(tag)
        );
        assert!(sink > 0);
    }
    println!(
        "(LOC counts code lines between SECTION markers in this file; timings are indicative, not a benchmark)"
    );
}

fn demo() {
    println!("-- the same input through all four parsers --");
    for src in [
        "-2 ** 2 + f(a, b)[1].sq",
        "a ? b : c ? d : e",
        "a - b - c",
        "a ** b ** c",
    ] {
        let e = parse_one(src, m_pratt).expect("demo parses");
        let (rpn, _) = shunt(&lex(src).0, 0).expect("demo shunts");
        println!("{src:<26} => {:<40} RPN: {}", full(&e), rpn_string(&rpn));
        for (name, m) in METHODS {
            assert_eq!(parse_one(src, m).as_ref(), Ok(&e), "{name}");
        }
    }
}

fn main() {
    // Big stack: the reference evaluator, `full` and Drop are recursive on purpose (they are not what we are testing).
    let t0 = Instant::now();
    let h = std::thread::Builder::new().stack_size(256 * 1024 * 1024).spawn(|| {
        demo();
        check_tables();
        println!("\n-- precedence/associativity tables: 21 examples x 4 methods agree");
        check_error_spans();
        println!();
        check_recovery();
        let (ok, err) = check_random(5000);
        println!("\nrandom: 5000 grammar-generated expressions parse identically in 4 methods, round-trip, eval == RPN ({ok} evaluated, {err} same-error)");
        assert!(ok > 1500, "too few successful evaluations to be meaningful");
        let (total, acc, rej) = check_fuzz(5000);
        println!("fuzz: {total} mutated strings, no panics, 4 methods agree ({acc} accepted, {rej} rejected, spans in range)");
        assert_eq!(total, 20_000);
        check_depth_and_size();
        bench();
    });
    h.expect("spawn").join().expect("all checks pass");
    println!("\nall assertions passed in {:.2?}", t0.elapsed());
}
