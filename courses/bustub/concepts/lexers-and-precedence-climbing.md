---
title: Lexers and precedence climbing
summary: Turning text into tokens with a peekable iterator, and tokens into a tree with one function per precedence level or a binding-power loop; how associativity falls out of a single comparison.
minutes: 9
---
`1 + 2 * 3` is nine characters. The engine needs a tree: `+` at the top, `1` on its left, `2 * 3` on its right. Two small programs do it: a **lexer** that groups characters into **tokens** (`1`, `+`, `2`, `*`, `3`) and a **parser** that groups tokens into a tree. Neither needs a library, and both are worth writing once, because every configuration language, query language and calculator you will meet is built the same way.

## The lexer: characters to tokens

A lexer is a loop over the characters with one character of lookahead. In Rust the tool is a `Peekable<Chars>`: `peek()` looks without consuming, `next()` consumes.

```rust
let mut it = "12+(3)".chars().peekable();
while let Some(&c) = it.peek() {
    // decide from c what kind of token starts here, then consume exactly its characters
}
```

Each kind of token has a rule for where it ends: digits end at the first non-digit, a word at the first character that is not a letter, digit or `_`, a string at the closing quote. The two things that go wrong are **longest match** (`<=` is one token, not `<` then `=`: try the two-character symbols first) and **what is not a token at all** (an `@` is an error, not a skipped character). White space and comments produce no token.

## The parser: tokens to a tree

The grammar says which operator binds tighter: `*` before `+`, `and` before `or`. There are two classic ways to write that down.

**One function per level.** `or_expr` calls `and_expr`, which calls `not_expr`, down to `atom`. Each level loops: parse one operand of the level below, then while the next token is an operator of this level, parse another operand and build a node.

**Binding powers (precedence climbing, or Pratt parsing).** One function takes a number, the loosest operator it may still consume:

```text
expr(min):
    left = atom()
    while next token is an operator with power >= min:
        consume it
        right = expr(power + 1)        // left associative: only tighter operators go on the right
        left  = node(op, left, right)
    return left
```

The `+ 1` is the whole story of **associativity**: with it, `10 - 3 - 2` takes `10 - 3` first (the second `-` is not tighter than the first, so it is not swallowed by the right operand) and the tree leans left. Without it (`expr(power)`) the tree leans right, which is right for `^` and `=` and wrong for `-`.

## Unary operators and parentheses

A prefix operator is part of `atom`: see `-`, parse an atom (or a tighter expression), wrap it. A parenthesis restarts the climb: see `(`, call `expr(1)` for the loosest level again, expect `)`. That is how `(1 + 2) * 3` puts a `+` below a `*`.

## Printing is the best test

For a tree `t`, print it with the **fewest parentheses its grammar allows** (parenthesise a child whose operator binds looser than its parent, and a right child whose operator binds equally), then parse the text: you must get `t` back. A generator of random trees plus that one property checks every precedence and every associativity at once, which no list of hand-picked examples does.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `while (i < n && isdigit(s[i])) ++i;` | `while it.peek().is_some_and(char::is_ascii_digit) { it.next(); }` |
| a `Token` struct with a `type` field and a union | an `enum Token { Num(i64), Op(char), ... }` |
| `std::unique_ptr<Expr>` children | `Box<Expr>` children |
| bison / yacc grammar files | a hand-written recursive descent; or a crate such as `nom`, `chumsky`, `lalrpop` |

## In real code

### Using it: a calculator in forty lines

```rust test
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(i64),
    Op(char),
    LParen,
    RParen,
}

fn lex(s: &str) -> Vec<Tok> {
    let mut out = vec![];
    let mut it = s.chars().peekable();
    while let Some(&c) = it.peek() {
        if c.is_whitespace() {
            it.next();
        } else if c.is_ascii_digit() {
            let mut n = 0i64;
            while let Some(d) = it.peek().and_then(|c| c.to_digit(10)) {
                n = n * 10 + d as i64;
                it.next();
            }
            out.push(Tok::Num(n));
        } else {
            it.next();
            out.push(match c {
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                _ => Tok::Op(c),
            });
        }
    }
    out
}

#[derive(Debug, PartialEq)]
enum E {
    Num(i64),
    Neg(Box<E>),
    Bin(char, Box<E>, Box<E>),
}

fn power(op: char) -> Option<u8> {
    match op {
        '+' | '-' => Some(1),
        '*' | '/' => Some(2),
        _ => None,
    }
}

struct P {
    toks: Vec<Tok>,
    pos: usize,
}

impl P {
    fn expr(&mut self, min: u8) -> E {
        let mut left = self.atom();
        while let Some(Tok::Op(op)) = self.toks.get(self.pos).cloned() {
            let Some(p) = power(op) else { break };
            if p < min {
                break;
            }
            self.pos += 1;
            let right = self.expr(p + 1); // left associative
            left = E::Bin(op, Box::new(left), Box::new(right));
        }
        left
    }

    fn atom(&mut self) -> E {
        let t = self.toks[self.pos].clone();
        self.pos += 1;
        match t {
            Tok::Num(n) => E::Num(n),
            Tok::Op('-') => E::Neg(Box::new(self.atom())),
            Tok::LParen => {
                let e = self.expr(1);
                assert_eq!(self.toks.get(self.pos), Some(&Tok::RParen), "expected )");
                self.pos += 1;
                e
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

fn parse(s: &str) -> E {
    P { toks: lex(s), pos: 0 }.expr(1)
}

fn show(e: &E) -> String {
    match e {
        E::Num(n) => n.to_string(),
        E::Neg(x) => format!("-{}", show(x)),
        E::Bin(op, l, r) => format!("({} {op} {})", show(l), show(r)),
    }
}

#[test]
fn the_lexer_groups_characters() {
    assert_eq!(lex("12+(3)"), vec![Tok::Num(12), Tok::Op('+'), Tok::LParen, Tok::Num(3), Tok::RParen]);
    assert_eq!(lex("  7 "), vec![Tok::Num(7)], "white space is no token");
}

#[test]
fn precedence_and_associativity() {
    assert_eq!(show(&parse("1 + 2 * 3")), "(1 + (2 * 3))");
    assert_eq!(show(&parse("1 * 2 + 3")), "((1 * 2) + 3)");
    assert_eq!(show(&parse("10 - 3 - 2")), "((10 - 3) - 2)", "the +1 makes it lean left");
    assert_eq!(show(&parse("(1 + 2) * 3")), "((1 + 2) * 3)");
    assert_eq!(show(&parse("-2 * 3")), "(-2 * 3)", "a prefix minus binds tighter than any operator");
}
```

### In the exercises

- **3d-03:** `tokenize` is the lexer loop above with SQL's token kinds: words folded to lower case, `"quoted names"`, numbers with fractions and exponents, `'strings'` with `''`, two-character symbols, comments.
- **3d-04:** `expr` is the parser, with SQL's levels: `or`, `and`, `not`, comparisons and `is [not] null`, `+ - ||`, `* / %`, unary minus. `primary` (given) parses the atoms and calls back into `expr` for parentheses and call arguments.

### Where it is used

- **`sqlparser-rs`**, the parser behind DataFusion and many Rust databases, is a hand-written precedence-climbing parser with a numeric precedence per operator.
- **rust-analyzer** parses Rust expressions with a Pratt parser; matklad's article "Simple but powerful Pratt parsing" is the best short explanation.
- **PostgreSQL** generates its parser from a grammar (`gram.y`, bison) with a precedence table, which is the same idea moved into a tool.
