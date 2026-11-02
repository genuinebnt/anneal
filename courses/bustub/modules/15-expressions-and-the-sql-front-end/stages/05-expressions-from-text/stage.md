The lexer gave you tokens. The parser turns them into the **syntax tree** (`Expr`, in `src/sql/ast.rs`): `a + 1 >= b - 2 and not c` is an `and` whose left side is a `>=` between two arithmetic nodes and whose right side is a `not`. The statement-level parser (`select`, `insert`, `create table`, ...) is given; it calls `expr()` whenever a full expression is expected (the select list, `where`, call arguments, parentheses). **That one function is yours.**

The grammar is a question of who binds tighter. From loosest to tightest:

| level | operators | associativity |
|---|---|---|
| 1 | `or` | left |
| 2 | `and` | left |
| 3 | `not` (prefix) | right |
| 4 | `= == < > <= >= <> !=` and postfix `is [not] null` | left |
| 5 | `+ - \|\|` | left |
| 6 | `* / %` | left |
| 7 | prefix `-` and `+` | right |
| atoms | numbers, strings, `true`/`false`/`null`, names, `f(args)`, `( expr )` | |

> [!CHECK] Parse `10 - 3 - 2` and `not a = b and c` by hand, as trees. Which one makes you decide an associativity, and which makes you decide a precedence between a prefix and an infix operator? Then: if you printed the tree back with the fewest parentheses that keep its meaning, which child of which node gets parentheses?
> ||`10 - 3 - 2` is `(10 - 3) - 2`: left associative, so the right operand of a `-` may contain only operators that bind *tighter*. `not a = b and c` is `(not (a = b)) and c`: `not` binds looser than `=` (it covers the whole comparison) and tighter than `and`. Printing: parenthesise a child whose operator binds *looser* than its parent's, and, because the operators are left associative, a *right* child whose operator binds equally: `a - (b - c)`.||
>
> - What does the loop in a level do after it has consumed an operator?
> - Where does `is null` fit: before or after the comparison operators?
> - What happens at the end of the input in the middle of an expression?

## The task

In `src/sql/parser.rs`, implement `Parser::expr` (and any helper methods you want); the helpers `peek`, `next`, `at_word`, `eat_word`, `at_symbol`, `eat_symbol`, `expect_*`, `unexpected()` and `primary()` are given, and a public `parse_expr(sql)` parses one whole expression and rejects anything left over.

- The levels and associativities of the table above; `primary()` parses the atoms (and calls `expr()` itself for parentheses and arguments).
- `a is null`, `a is not null` become `Expr::IsNull { negated }`; `not` is `Expr::Unary { op: "not" }`; infix operators are `Expr::Binary { op }` with the operator as written (`and`, `or`, `<>`, `!=`, `||` ...); a prefix `-` in front of a number is part of the number (`-5` is `Integer(-5)`), in front of anything else `Expr::Unary { op: "-" }`; a prefix `+` does nothing.
- A missing operand, an unclosed parenthesis or text left over is an error built with `self.unexpected()`.

The tests: exact trees (atoms, `*` before `+`, left associativity, `and` before `or`, comparison before `and`, `not`, `is null`, unary minus, errors) and three properties: **print any random tree with the fewest parentheses its precedences allow, parse it, and get the same tree back**; wrapping in parentheses and spacing the tokens out with comments changes nothing; and any run of tokens is parsed or refused without a panic.

## Your freedom

Recursive descent with one method per level (what the given statement parser does), a loop with **binding powers** (one function and a table), or a Pratt parser with prefix and infix handlers. Any of them passes if the trees are the same.

## The Rust toolbox

**One method per level.** `fn or_expr(&mut self) -> P<Expr> { let mut left = self.and_expr()?; while self.eat_word("or") { let right = self.and_expr()?; left = Expr::Binary { op: "or".into(), left: Box::new(left), right: Box::new(right) }; } Ok(left) }` is the whole shape; each level differs in the operator test and the callee.

**Binding powers.** `fn expr_bp(&mut self, min: u8) -> P<Expr>`: take an atom, then while the next token is an infix operator whose power is at least `min`, consume it and recurse with `power + 1` for the right side. The `+ 1` is what makes it left associative.

**`Box` for recursive types.** `Expr::Binary { left: Box<Expr>, .. }`: a tree needs an indirection because a node contains nodes. `Box::new(left)` moves the left tree to the heap, and `left = Expr::Binary { left: Box::new(left), .. }` reuses the variable.

**`?` through the whole stack.** Every parse method returns `P<Expr>` (a `Result`); `?` hands errors up to the caller untouched.

**Peeking a token without consuming.** `match self.peek() { Some(Token::Symbol(s)) if ["+", "-"].contains(s) => *s, _ => break }` picks the operator, and `self.pos += 1` consumes it.

**A printer in the test.** The tests print trees with a precedence-aware function; read it (it is short) when a counterexample puzzles you: the property's failure message shows the SQL text it printed.

## If this is new

- L7 Enums & pattern matching: an AST as enums, `let else`, patterns with guards.
- S7 Smart pointers & interior mutability: `Box<Expr>` for a recursive type.
- L8 Error design: errors with positions and messages.
- Y5 Testing & verification: print-then-parse round trips; a generator of random trees.
- The optional *lexers and precedence climbing* concept has a complete calculator parser to read first.
- [F3 Memory & allocation](/t/f3-memory-allocation): Arenas and pools: an arena AST, a string interner (optional).

## Tests

- Atoms (numbers, strings, booleans, NULL, dotted names, calls, parentheses leave no trace); `*` binds tighter than `+`; operators of a level associate left; `and` before `or`, comparison before `and`; `not` covers a comparison; `is null`, unary minus and its fold into numbers; text that is not one expression is an error.
- Properties: print-then-parse gives the tree back; parentheses and spacing change nothing; any run of tokens is answered.

## Hints

### Write the table as functions first

Even if you want binding powers in the end, start with the six functions of the table: it is the easiest way to see the shape. Then ask which parts repeat.

### Where do `is null` and `not` go?

`is null` is a postfix operator at the comparison level: after you have parsed the left operand at the additive level, a loop that accepts `is [not] null` and comparison operators (so `a = b is null` is `(a = b) is null`, as the printer expects). `not` is a prefix operator one level above comparison: `not` followed by another `not_expr`.

### The minus sign

A minus in front of a number is part of the number: when the operand you parsed is `Integer(v)` return `Integer(-v)`, when it is `Float(s)` flip the sign of the text, otherwise wrap in `Unary`. That is what keeps `1 - -5` and `-5 * 2` right.

### A failing property

The message prints the SQL text of the counterexample (shrunk to a small one). Parse it by hand with the table above and find the level that behaves differently.

## Performance

Recursive descent is `O(n)` in the tokens; the depth of recursion is the nesting depth plus the number of levels (seven calls per atom in the one-method-per-level form). A deeply nested input (`((((((...))))))` a hundred thousand deep) overflows the stack in any recursive parser; PostgreSQL limits expression depth for this reason (`max_stack_depth`).

**Measure it.** Parse `1 + 1 + ... + 1` with 100 000 terms: a left-leaning loop does it with no recursion in the operators; try right-nesting with parentheses and see where it stops.

## Experiment

Optional. Predict first, then run.

1. **Make `-` right associative.** Change the `+ 1`. Which tests fail, and which exact trees differ?
2. **Chained comparisons.** In many databases `1 < 2 < 3` is an error or a type error, not `(1 < 2) < 3`. Make the parser reject it. Which property needs to change?
3. **A new operator.** Add `^` (power, right associative, tighter than `*`). What is the one-line change in a binding-power parser, and the three-line change in the per-level one?

## Other designs

- **Recursive descent, one function per level** (the given statement parser, PostgreSQL's hand-written parts).
- **Precedence climbing / Pratt parsing:** a table of powers, one loop (`sqlparser-rs`, rust-analyzer).
- **Shunting-yard:** an explicit operator stack instead of recursion.
- **Generated parsers:** a grammar file with precedence declarations (PostgreSQL's `gram.y` via bison, `lalrpop`).
- **Parser combinators** (`nom`, `chumsky`).

## In BusTub

BusTub hands the text to `libpg_query`, which builds PostgreSQL's parse tree (`PGAExpr` nodes with an operator name and two children), and its binder (`BindExpression`) walks that tree. The port's `Expr` is the same shape: an operator name with a left and a right child.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unique_ptr<Expr> left` | `Box<Expr>` |
| a parse function that throws on error | a function that returns `Result<Expr>` and uses `?` |
| `bison` precedence declarations (`%left '+' '-'`) | a function per level, or a table of binding powers |
| `switch (tok.type) { case T_NUMBER: ... }` | `match self.next() { Some(Token::Number(n)) => ... }` |

**Port rule:** a grammar with precedence declarations becomes one function per level (or one function and a power table); a thrown parse error becomes an `Err` that `?` carries up.

## Learn more

- [Crafting Interpreters: parsing expressions](https://craftinginterpreters.com/parsing-expressions.html) · PostgreSQL's [operator precedence](https://www.postgresql.org/docs/current/sql-syntax-lexical.html#SQL-PRECEDENCE)
