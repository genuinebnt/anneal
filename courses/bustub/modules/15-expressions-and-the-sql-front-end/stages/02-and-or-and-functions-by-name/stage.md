`WHERE price > 10 AND stock > 0`: `AND` and `OR` combine two answers that can each be true, false or **NULL** (unknown). Some combinations are known anyway: `FALSE AND anything` is FALSE, because no value of "anything" can make it true; `TRUE OR anything` is TRUE. Getting the NULL rows right is what separates a database from a boolean calculator.

The second node of this stage is the first *function*: `lower(name)` and `upper(name)`. Case mapping looks trivial and is not: `ß` becomes `SS`, `É` becomes `é`, and the string can change length. And a function is called by name, so the planner needs a way to turn the name `lower` and a list of arguments into a node: a **function factory**. The binder (given) turns `lower(name)` into "a call of a function named `lower` with one argument"; the planner hands that to your factory. After this stage the planner can build every node of the module; the next two stages write the text front end that feeds it.

> [!CHECK] What do `NULL AND FALSE`, `NULL OR TRUE` and `NULL AND TRUE` evaluate to, and why can the first two be answered without knowing the NULL? Separately: `upper('straße')` has how many characters, and what does that say about doing case mapping byte by byte?
> ||FALSE, TRUE and NULL. `AND` is false when either side is false and `OR` is true when either side is true, whatever the unknown turns out to be; `NULL AND TRUE` depends on the unknown, so it stays NULL. `upper('straße')` is `STRASSE`: six characters become seven, because `ß` (one character, two bytes in UTF-8) becomes `SS` (two characters, two bytes). Case mapping works on characters, can change their count and cannot be done byte by byte.||
>
> - Replace NULL by true, then by false: does the answer change?
> - Does your `evaluate` have to evaluate the right side when the left side decides the answer?
> - Which standard-library methods do the case mapping?

## The task

In `logic_expression.rs`, `string_expression.rs` (both in `src/execution/expressions/`) and `src/planner/planner.rs`. The constructors with their type checks, `to_string`, `as_cmp_bool`, `cmp_to_value` and `StringExpression::evaluate` are given. After this stage:

- **`AND` and `OR` follow Kleene's three-valued logic.** `perform_computation(lhs, rhs) -> CmpBool` is the table below (`CmpBool::Null` for NULL); `evaluate` and `evaluate_join` evaluate both sides, compute, and answer a BOOLEAN. A failing child is an error even when the other side would decide the answer.
- **`lower` and `upper` map characters, not bytes.** `compute(&str)` is the Unicode lower- or upper-case form, by `expr_type`; `result(&Value)` is that string as a VARCHAR, and the VARCHAR NULL stays NULL.
- **Functions are found by name.** `Planner::get_func_call_from_factory(func_name, args) -> Result<ExprRef>` builds a `StringExpression` for `lower` and `upper` called with exactly one argument. Any other name is an error (`func call {name} not supported in planner yet`), so is any other number of arguments, and a non-VARCHAR argument is the error the `StringExpression` constructor already reports. The binder lower-cases names, so `UPPER(x)` arrives as `upper`.

| AND | TRUE | FALSE | NULL | | OR | TRUE | FALSE | NULL |
|---|---|---|---|---|---|---|---|---|
| **TRUE** | TRUE | FALSE | NULL | | **TRUE** | TRUE | TRUE | TRUE |
| **FALSE** | FALSE | FALSE | FALSE | | **FALSE** | TRUE | FALSE | NULL |
| **NULL** | NULL | FALSE | NULL | | **NULL** | TRUE | NULL | NULL |

The NULLs in these tables come from columns and comparisons. A bare `NULL` literal is typed INTEGER, as in BusTub, so in the shell `SELECT false AND null` and `SELECT lower(null)` are type errors, where PostgreSQL answers `false` and NULL.

> [!ASIDE] The steps, if you would rather not work them out
> 1. `perform_computation`: a `match` on `(lhs, rhs)` for AND (any FALSE gives FALSE, two TRUEs give TRUE, the rest NULL) and the mirror for OR.
> 2. Logic `evaluate`: evaluate both children with `?`, convert each with `as_cmp_bool`, compute, convert back with `cmp_to_value`; `evaluate_join` the same with `evaluate_join`.
> 3. `compute`: `to_lowercase` or `to_uppercase` by `expr_type`. `result`: `as_str` on the value, `Some` becomes a VARCHAR of the computed string, `None` the VARCHAR NULL.
> 4. The factory: map the name to a `StringExpressionType` (an unknown name is an error), check that there is exactly one argument, and return `Arc::new(StringExpression::new(arg, kind)?)`.

## Your freedom

The truth tables as a `match` on the pair, on one side then the other, or as arithmetic on a three-valued number; whether `compute` is a free function or a method. And how functions are found, which is the design decision of the stage: a `match` on the name, or a **registry**, a table of entries (name, number of arguments, constructor) that one lookup serves and one line extends. A `match` is shorter for two functions; a registry is the shape a database with user-defined functions needs, and it puts the arity next to the name instead of in a separate check. Choose, and be ready to say what adding `length(x)` would cost in yours (the third experiment).

## The Rust toolbox

**`match` on a pair, with or-patterns.** `match (lhs, rhs) { (CmpBool::False, _) | (_, CmpBool::False) => CmpBool::False, (CmpBool::True, CmpBool::True) => CmpBool::True, _ => CmpBool::Null }` is the whole AND table: arms are tried in order, so the specific ones go first and `_` catches the rest.

**`str::to_lowercase` / `to_uppercase`** allocate a new `String` and handle Unicode (including the final-sigma rule); `to_ascii_lowercase` is the byte-wise one and would get `É` wrong.

**`Option::map`.** `val.as_str().map(|s| Value::varchar(&self.compute(s)))` handles "a string or a NULL" without a `match`.

**`match` on a string slice.** `match func_name { "lower" => .., "upper" => .., _ => return Err(..) }`: string literals are patterns.

**A table of constructors.** `const FUNCTIONS: &[(&str, usize, fn(Vec<ExprRef>) -> Result<ExprRef>)] = &[("lower", 1, |mut a| Ok(Arc::new(StringExpression::new(a.remove(0), StringExpressionType::Lower)?))), ...];` A closure that captures nothing coerces to a plain `fn` pointer, so it can live in a `const`; `FUNCTIONS.iter().find(|(name, ..)| *name == func_name)` is the lookup.

**`Vec::remove` and `Arc::new` into a trait object.** `args.remove(0)` takes the single argument out by value, with no clone. `Ok(Arc::new(StringExpression::new(arg, kind)?))`: the coercion from `Arc<StringExpression>` to `Arc<dyn Expression>` happens at the `Ok`.

**A property that is a table.** The AND/OR test enumerates `{true, false, NULL}²` by generating `Option<bool>` and compares with a four-line oracle: an exhaustive truth table is a property over a finite set.

## If this is new

- L7 Enums & pattern matching: `match` on tuples, or-patterns.
- [S2 Strings & text](/t/s2-strings-text): characters versus bytes, case mapping.
- [S1 Option & Result](/t/s1-option-result): `map`, `as_str`, returning `Err` early, passing on a constructor's error with `?`.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): coercing `Arc<T>` to `Arc<dyn Trait>`.
- The optional *unicode and case mapping* concept.
- Y5 Testing & verification: Build it: a model for three-valued logic; print-then-parse round trips; fuzzing a lexer and a parser.

## Tests

- All nine combinations of AND and of OR, and Kleene's logic, commutative, for every pair; both sides must be BOOLEAN when the node is built; logic over comparisons in a row and a join, and its text.
- `lower` and `upper` on ASCII, accented letters, `straße`, an emoji and the empty string, equal to the standard library for any text; NULL in, NULL out; a non-VARCHAR argument is refused; nested calls and columns.
- The factory builds working `lower` and `upper`, and refuses other names, other argument counts and arguments that are not strings.

## Hints

### Check the table before you code

Replace NULL by true and then by false in each cell: if both replacements agree, the cell is that answer; if they differ, it is NULL. That rule produces the whole table.

### Do not short-circuit

Evaluate both children: an error in the right child must still be an error. The expression tree has no side effects, but it can fail, and a short-circuit makes the same query fail or not depending on the data.

### Where each error belongs

`shout('a')` is a name error and `lower('a', 'b')` an arity error: only the factory knows names and how many arguments each takes, so it checks both before building anything. `upper(1)` is a type error, and the `StringExpression` constructor already reports it: pass it on with `?` rather than writing a second check that could disagree with the first. All three are reported at planning time, before any row is read.

## Performance

Logic is a branch per row; text adds one allocation. `to_lowercase` is `O(n)` with a heavier constant than ASCII-only code because it must decode characters; an engine that knows a column is ASCII can use `make_ascii_lowercase` in place. The factory runs once per call site per query, at planning time, never per row: resolve once, evaluate many times.

**Measure it.** Lower a million 20-character strings with `to_lowercase` and with `to_ascii_lowercase`, after predicting the ratio. Then build a hundred `lower` calls through your factory and compare that with evaluating one of them over a thousand rows.

## Experiment

Optional. Predict first, then run.

1. **Short-circuit.** Make `AND` return FALSE when the left side is FALSE without evaluating the right. Which tests notice, and when would it matter in a real query?
2. **ASCII only.** Switch to `to_ascii_lowercase`. Which tests fail, and which languages would your database be wrong for?
3. **A third function.** Add `length(x)` returning an INTEGER. Which places change in your design (a new expression type, the factory, the `explain` text), and how many lines does the factory part take?

## Other designs

- **Short-circuit evaluation** (most programming languages): faster, different with errors and side effects. **A single n-ary AND/OR node** (`a AND b AND c`): fewer nodes, one loop.
- **Locale-aware case mapping** (PostgreSQL's `lower` depends on the locale: Turkish dotless i), and **case folding** for comparisons (`casefold`, not `lower`).
- **A match in the planner (BusTub's)** or **a function registry** filled at start-up, so extensions can add functions.
- **Overload resolution by argument types** (PostgreSQL): the same name picks different implementations. **Binding functions in the binder**, so the planner receives resolved functions.

## In BusTub

`logic_expression.h` (`PerformComputation` returns `CmpBool`); `string_expression.h`, which does `std::transform(val.begin(), val.end(), val.begin(), ::tolower)` (ASCII-only: the port is deliberately better, and the tests include `école`); and `Planner::GetFuncCallFromFactory` in `src/planner/plan_func_call.cpp`:

```cpp
if (func_name == "lower" || func_name == "upper") { ... return std::make_shared<StringExpression>(args[0], ...); }
throw Exception(fmt::format("func call {} not supported in planner yet", func_name));
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `if (lhs == CmpFalse \|\| rhs == CmpFalse) return CmpFalse;` | `(False, _) \| (_, False) => False` |
| `std::transform(..., ::tolower)` (bytes, locale-dependent) | `str::to_lowercase` (characters, Unicode) |
| `if (func_name == "lower" \|\| func_name == "upper")` | `match func_name { "lower" => .., "upper" => .., _ => .. }` |
| `std::make_shared<StringExpression>(args[0], kind)` | `Arc::new(StringExpression::new(arg, kind)?)` |

**Port rule:** a chain of `if`s over two enum values becomes a `match` on the pair; a byte-wise string loop becomes a `str` method; a chain of string comparisons becomes a `match` on the slice (or a table); `make_shared` becomes `Arc::new`.

## Learn more

- [`str::to_lowercase`](https://doc.rust-lang.org/std/primitive.str.html#method.to_lowercase) · [Unicode FAQ: case mappings](https://www.unicode.org/faq/casemap_charprop.html) · PostgreSQL's [logical operators](https://www.postgresql.org/docs/current/functions-logical.html)
- BusTub's [expression factory](https://github.com/cmu-db/bustub/blob/master/src/planner/plan_func_call.cpp)
