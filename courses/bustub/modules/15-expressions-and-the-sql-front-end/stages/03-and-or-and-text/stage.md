`WHERE price > 10 AND stock > 0`: `AND` and `OR` combine two answers that can each be true, false or **NULL** (unknown). Some combinations are known anyway: `FALSE AND anything` is FALSE, because no value of "anything" can make it true; `TRUE OR anything` is TRUE. Getting the NULL rows right is what separates a database from a boolean calculator. The second node of this stage is the first *function*: `lower(name)` and `upper(name)`. Case mapping looks trivial and is not: `ß` becomes `SS`, `É` becomes `é`, and the string has a different length afterwards.

> [!CHECK] What do `NULL AND FALSE`, `NULL OR TRUE` and `NULL AND TRUE` evaluate to, and why can the first two be answered without knowing the NULL? Separately: `upper('straße')` has how many characters, and what does that say about doing case mapping byte by byte?
> ||FALSE, TRUE and NULL. `AND` is false when either side is false and `OR` is true when either side is true, whatever the unknown turns out to be; `NULL AND TRUE` depends on the unknown, so it stays NULL. `upper('straße')` is `STRASSE`: seven characters become seven here but `ß` (one char, two bytes) became `SS` (two chars, two bytes): case mapping works on characters, can change their count and cannot be done byte by byte.||
>
> - Replace NULL by true, then by false: does the answer change?
> - Does your `evaluate` have to evaluate the right side when the left side decides the answer?
> - Which standard-library methods do the case mapping?

## The task

In `src/execution/expressions/logic_expression.rs` (`LogicType`, the constructor with its BOOLEAN check, `to_string` and the conversion to a BOOLEAN `Value` are given):

- `perform_computation(lhs, rhs) -> CmpBool`: the truth tables below (`CmpBool::Null` for NULL; `as_cmp_bool`, given, converts a `Value`).
- `evaluate` and `evaluate_join`: evaluate both children, compute, convert with `cmp_to_value`.

| AND | TRUE | FALSE | NULL | | OR | TRUE | FALSE | NULL |
|---|---|---|---|---|---|---|---|---|
| **TRUE** | TRUE | FALSE | NULL | | **TRUE** | TRUE | TRUE | TRUE |
| **FALSE** | FALSE | FALSE | FALSE | | **FALSE** | TRUE | FALSE | NULL |
| **NULL** | NULL | FALSE | NULL | | **NULL** | TRUE | NULL | NULL |

In `src/execution/expressions/string_expression.rs` (the constructor with its VARCHAR check, `to_string` and the type are given):

- `compute(val: &str) -> String`: the lower- or upper-case form, by `expr_type`.
- `result(val: &Value) -> Value`: the transformed string as a VARCHAR; the VARCHAR NULL stays NULL.

The tests: all nine combinations of AND and of OR, both sides BOOLEAN, logic over comparisons in a row and a join; lower and upper on ASCII, `école`, `straße` and an emoji, NULL in and out, a non-VARCHAR argument refused, nested calls; and two properties: AND and OR follow Kleene's logic and are commutative for every pair of true/false/NULL, and `lower`/`upper` equal the standard library's `to_lowercase`/`to_uppercase` for any text (and upper twice is upper once).

## Your freedom

The truth tables as a `match` on the pair, on one side then the other, or as arithmetic on a three-valued number; whether `compute` is a free function or a method.

## The Rust toolbox

**`match` on a pair.** `match (lhs, rhs) { (CmpBool::False, _) | (_, CmpBool::False) => CmpBool::False, (CmpBool::True, CmpBool::True) => CmpBool::True, _ => CmpBool::Null }` is the whole AND table: the arms are tried in order, so the specific ones go first and `_` catches the rest.

**Or-patterns.** `(False, _) | (_, False)` is one arm for two shapes.

**`str::to_lowercase` / `to_uppercase`** allocate a new `String` and handle Unicode (including the final-sigma rule); `to_ascii_lowercase` is the byte-wise one and would get `É` wrong.

**`Option::map`.** `val.as_str().map(|s| Value::varchar(&self.compute(s)))` handles "a string or a NULL" without a `match`.

**A property that is a table.** The AND/OR test enumerates `{true, false, NULL}²` by generating `Option<bool>` and compares with a four-line oracle; an exhaustive truth table is a property over a finite set.

## If this is new

- L7 Enums & pattern matching: `match` on tuples, or-patterns.
- [S2 Strings & text](/t/s2-strings-text): characters versus bytes, case mapping.
- [S1 Option & Result](/t/s1-option-result): `map`, `as_str`.
- The optional *unicode and case mapping* concept.
- Y5 Testing & verification: Build it: a model for three-valued logic; print-then-parse round trips; fuzzing a lexer and a parser.

## Tests

- All nine combinations of AND and of OR; both sides must be BOOLEAN when the node is built; logic of comparisons in a row and a join; logic describes itself.
- `lower` and `upper` on ASCII, accented letters, `straße`, an emoji, an empty string; NULL in, NULL out; a non-VARCHAR argument is refused; nested calls and columns; `compute` is a plain string function.
- Properties: Kleene's logic for every pair; the standard library for any text.

## Hints

### Check the table before you code

Replace NULL by true and then by false in each cell: if both replacements agree, the cell is that answer; if they differ, it is NULL. That rule produces the whole table.

### Do not short-circuit

Evaluate both children: an error in the right child must still be an error (the expression tree has no side effects, but it can fail).

## Performance

Both nodes are branches and, for text, one allocation. `to_lowercase` is `O(n)` with a heavier constant than ASCII-only code because it must decode characters; an engine that knows a column is ASCII can use `make_ascii_lowercase` in place.

**Measure it.** Lower a million 20-character strings with `to_lowercase` and with `to_ascii_lowercase`; predict the ratio.

## Experiment

Optional. Predict first, then run.

1. **Short-circuit.** Make `AND` return FALSE when the left side is FALSE without evaluating the right. Which tests notice, and when would it matter in a real query?
2. **ASCII only.** Switch to `to_ascii_lowercase`. Which tests fail, and which languages would your database be wrong for?

## Other designs

- **Short-circuit evaluation** (most programming languages): faster, different with errors and side effects.
- **A single n-ary AND/OR node** (`a AND b AND c`): fewer nodes, one loop.
- **Locale-aware collation** (PostgreSQL's `lower` depends on the locale): Turkish dotless i.
- **Case folding for comparisons** (`casefold`, not `lower`).

## In BusTub

`logic_expression.h` (`PerformComputation` returns `CmpBool`), and `string_expression.h`, which does `std::transform(val.begin(), val.end(), val.begin(), ::tolower)` (ASCII-only: the port is deliberately better, and the tests include `école`).

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::transform(..., ::tolower)` (bytes, locale-dependent) | `str::to_lowercase` (characters, Unicode) |
| `CmpBool::CmpNull` | `CmpBool::Null` |
| `if (lhs == CmpFalse \|\| rhs == CmpFalse) return CmpFalse;` | `(False, _) \| (_, False) => False` |

**Port rule:** a byte-wise string loop becomes a `str` method; a chain of `if`s over two enum values becomes a `match` on the pair.

## Learn more

- [`str::to_lowercase`](https://doc.rust-lang.org/std/primitive.str.html#method.to_lowercase) · [Unicode FAQ: case mappings](https://www.unicode.org/faq/casemap_charprop.html) · PostgreSQL's [logical operators](https://www.postgresql.org/docs/current/functions-logical.html)
