Until now the SQL text has been parsed by code you were given. From here the front of the pipeline is yours, starting with its first step. A **lexer** (or tokenizer) reads characters and groups them into **tokens**: `select a1, 42 from t` is `Word("select")`, `Word("a1")`, `Symbol(",")`, `Number("42")`, `Word("from")`, `Word("t")`. It throws away what the parser should not have to see (white space, comments), folds the case of unquoted words (PostgreSQL treats `SELECT` and `select` alike), and refuses text that is not SQL at all (`a @ b`). Nothing in it knows what a query is.

> [!CHECK] In `a<=b`, why must the lexer try two-character symbols before one-character ones? In `1-2`, is the `-` a minus sign or a subtraction, and who decides? In `x /* c */ y`, how many tokens, and what makes `a - -b` different from `a --b`?
> ||If `<` were tried first the result would be `<` then `=`: a different token stream, and `a < = b` is a syntax error. The lexer does not decide about `-`: it produces the symbol `-`, and the parser decides from position whether it is binary or unary. `x /* c */ y` is two tokens (the comment is dropped). `a - -b` is `a`, `-`, `-`, `b`, while `a --b` is `a` then a *comment* running to the end of the line.||
>
> - What is the longest match rule, and where does your code apply it?
> - What does the lexer do at the end of the input inside a string?
> - Where does a number end in `12e` and in `1.2.3`?

## The task

In `src/sql/lexer.rs` (the `Token` enum and `parse_error` are given), implement `tokenize(sql) -> Result<Vec<Token>>`:

- **White space** separates tokens and produces none. `-- ...` runs to the end of the line and `/* ... */` to the closing `*/`; both produce nothing, and a comment separates tokens (`8/*c*/2` is two numbers).
- **Words** start with a letter or `_` and continue with letters, digits, `_` or `$`; the token is the word **lower-cased** (letters beyond ASCII are letters).
- **`"Quoted names"`** are `Quoted` exactly as written; an unterminated one is a parse error.
- **Numbers** are digits with an optional fraction and an optional exponent, **kept as written** (`1.5`, `.5`, `7.`, `1e3`, `2.5E-4`); a leading `.5` is a number; an `e` that is not followed by digits starts the next word (`12e` is `12`, `e`).
- **`'Strings'`** may contain `''` for a quote; an unterminated one is a parse error.
- **Symbols**: `<= >= <> != || :: ==` (two characters, tried first), then `+ - * / % = < > , ( ) ; . [ ]`.
- Anything else is `parse_error("syntax error at or near \"c\"")`.

The tests: exact scenarios (words, numbers, symbols, case folding, number shapes, string quoting, two-character symbols, comments, bad input is an error and never a panic) and three properties: printing any list of tokens with spaces, comments or newlines between them and lexing gives the list back; and the lexer answers (tokens or an error) for any text without panicking.

## Your freedom

Everything inside `tokenize`: a `Vec<char>` and an index, a `Peekable<Chars>`, `char_indices` and slicing, helper functions for each kind of token, or a table of symbols. The `Token` type is the contract.

## The Rust toolbox

**`Vec<char>` and an index.** `let chars: Vec<char> = sql.chars().collect();` makes lookahead trivial (`chars.get(i + 1)`) and avoids slicing a `&str` in the middle of a multi-byte character. Slice with `chars[start..i].iter().collect::<String>()`.

**`Peekable`.** `let mut it = sql.chars().peekable(); while let Some(&c) = it.peek() { ... it.next(); }` is the iterator version; `it.next_if(|c| c.is_ascii_digit())` consumes a character only if it matches.

**`char` classification.** `c.is_alphabetic()`, `is_alphanumeric()`, `is_ascii_digit()`, `is_whitespace()` are Unicode-aware (the first three) and ASCII-only (`is_ascii_*`).

**`&'static str` for symbols.** `Token::Symbol(&'static str)` means the token holds a pointer to a literal and allocates nothing; return the literal from a `match`.

**An error with a message.** `return Err(parse_error("unterminated quoted string"))`; the error carries the kind `Invalid` and the text "Query failed to parse: ...".

**Never index blindly.** `chars[i + 1]` panics at the end; `chars.get(i + 1)` returns an `Option`. The "any text" property will find every unchecked index.

## If this is new

- [S2 Strings & text](/t/s2-strings-text): `char` versus byte, `chars()`, why slicing a `str` can panic.
- S6 Iterators: `Peekable`, `while let Some(..) = it.peek()`.
- L8 Error design: returning `Err` early with `?` and `return Err(..)`.
- The optional *lexers and precedence climbing* concept has a complete tiny lexer to read first.
- Y5 Testing & verification: Build it: a model for three-valued logic; print-then-parse round trips; fuzzing a lexer and a parser.

## Tests

- Words, numbers and symbols; white space alone is nothing; case folding; non-ASCII letters; number shapes (`12e`, `1.2.3`); string quoting; two-character symbols win; comments are skipped and separate tokens; bad input is an error.
- Properties: lexing printed tokens gives them back (with spaces, newlines and comments as separators); any text is answered without a panic.

## Hints

### The shape of the loop

"Look at the current character; decide which kind of token starts; consume exactly its characters; push it; continue." The decision is an `if` ladder: white space, `--`, `/*`, letter, digit (or `.` followed by a digit), `'`, `"`, then symbols.

### Longest match

For symbols look at two characters first (`<=`, `<>`, ...), then one. For numbers, scan digits, then an optional `.` and digits, then an optional exponent **only if digits follow it** (look ahead before you consume the `e`).

### Strings

Inside a string, a quote followed by a quote is one quote character in the token; a quote followed by anything else ends the string.

### The "any text" test found a panic

Look for `chars[i]` or `chars[i + 1]` without a bounds check, a `/*` that is never closed, and a slice that starts after the end.

## Performance

A lexer is a single pass: `O(n)` with a small constant, a few hundred megabytes per second for simple code. Collecting `Vec<char>` costs four bytes per character; a byte-based lexer over `&[u8]` avoids that and treats non-ASCII bytes as part of words.

**Measure it.** Tokenize a megabyte of generated `insert` statements and compute bytes per second; then switch to `Peekable<Chars>` and compare.

## Experiment

Optional. Predict first, then run.

1. **Keep positions.** Add the byte offset to each token and report it in errors. What changes in the property that prints and re-lexes?
2. **Unterminated comment.** What does your lexer do with `/* never closed`? What would PostgreSQL do?

## Other designs

- **A hand-written loop (ours).**
- **Regex-driven lexers** (`logos`, `lex`): a table of patterns compiled to a state machine.
- **Lexerless parsing** (parser combinators over characters, e.g. `nom`): no token stream.
- **Keep the case** and let the parser fold: simpler lexer, more work later.

## In BusTub

BusTub does not have a lexer of its own: it embeds PostgreSQL's parser (`libpg_query`), whose scanner (`scan.l`) is generated by flex. The port gives you a small parser instead so the course needs no C dependency; this stage is the part PostgreSQL's `scan.l` plays.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `while (i < n && isalnum(s[i])) ++i;` | `while i < chars.len() && chars[i].is_alphanumeric() { i += 1; }` |
| `enum TokenType { WORD, NUMBER, ... }` + a union | `enum Token { Word(String), Number(String), Symbol(&'static str), .. }` |
| flex rules in a `.l` file | a hand-written loop (or `logos`) |
| `strncmp(s + i, "<=", 2) == 0` | `chars[i..].starts_with(&['<', '='])` or a two-character `match` |

**Port rule:** a lexer's state machine becomes a loop with `if`/`match`; a tagged union becomes an enum with data.

## Learn more

- [`Peekable`](https://doc.rust-lang.org/std/iter/struct.Peekable.html) · PostgreSQL's [lexical structure of SQL](https://www.postgresql.org/docs/current/sql-syntax-lexical.html) · [Crafting Interpreters: scanning](https://craftinginterpreters.com/scanning.html)
