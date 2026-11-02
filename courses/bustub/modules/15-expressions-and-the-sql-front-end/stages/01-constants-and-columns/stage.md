From here on the course runs SQL. You type `select lower(name) from people where id = 3;` and a pipeline turns the text into a **plan**, a tree of operators (scan, filter, projection, ...). The part of the plan that computes with values (`lower(name)`, `id = 3`, `a + b`) is an **expression tree**, and the nodes of that tree are this module's job.

```text
  SQL text  →  parser  →  binder  →  planner  →  optimizer  →  executors  →  rows
              (syntax)   (names →   (a tree of    (rewrites    (pull tuples,
                          objects)   operators)    the plan)    evaluate expressions)
```

This stage is the leaves of the tree: a **constant** (`5`, `'hello'`) and a **column value** (`the third column of the row`). Every other node of the module evaluates its children and combines the answers, so once leaves work the rest is small.

> [!CHECK] An expression `#0.2 = 5` is built once and evaluated for a million rows. What does it need to be told on each evaluation, and what does it already know? Why does a join make the expression's input two tuples instead of one?
> ||The tree is built once from names (`salary = 5`); by the time it exists the names are gone and `#0.2` means "column 2 of input 0". Each evaluation is given the *row* (bytes) and the *schema* (what the bytes mean), because a tuple alone does not say where column 2 starts. A join has two inputs, a left and a right row, each with its own schema, so a column must say which of the two it reads: `#1.0` is column 0 of the right row.||
>
> - What does a constant ignore?
> - Which two things does `ColumnValueExpression` need to produce a `Value`?
> - What would break if the left and right schemas were the same object?

## The task

In `src/execution/expressions/` (the `Expression` trait in `abstract_expression.rs` is given):

- `ConstantValueExpression::evaluate` and `evaluate_join`: return the literal, whatever the tuples are.
- `ColumnValueExpression::evaluate`: the value of column `col_idx` of the tuple, read with the tuple's schema (module 3b's `Tuple::get_value`).
- `ColumnValueExpression::evaluate_join`: `tuple_idx` says which input the column belongs to (0 = left, anything else = right), and each is read with **its own** schema.

The tests: exact scenarios (a constant is itself for any tuple, a column reads a NULL or a VARCHAR, a join column picks its side, expressions describe themselves as `#0.3` and `5`), and a property: for random rows and join rows, every column reads back what the row holds, from the side it names.

## Your freedom

Almost none here: these are the two simplest nodes. The design question is the shape of the `Expression` trait, and that one is given.

## The Rust toolbox

**A trait object behind `Arc`.** `ExprRef = Arc<dyn Expression>`: operators hold expressions as shared trait objects, as BusTub holds `shared_ptr<AbstractExpression>`. Sharing matters: the optimizer builds new plans out of parts of old ones without copying whole trees. `Arc::clone(&e)` copies a pointer and bumps a counter.

**`Value` is `Clone`.** A constant hands out a copy: `self.val.clone()`.

**Pick a side.** `if self.tuple_idx == 0 { left.get_value(left_schema, i) } else { right.get_value(right_schema, i) }` is the whole join logic.

**Downcasting.** Later modules ask "is this expression a column?": `expr.as_any().downcast_ref::<ColumnValueExpression>()` returns `Some(&ColumnValueExpression)` or `None`. (`as_any` is on the trait for exactly that.)

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Trait`, `Arc<dyn Trait>`, why a trait needs `Send + Sync` to be shared across threads.
- S7 Smart pointers & interior mutability: `Arc`, `Box`.
- The optional *expression trees* and *the SQL pipeline* concepts.

## Tests

- A constant (an integer, a string, a NULL) is itself for any tuple, in `evaluate` and in `evaluate_join`.
- A column reads the right column of a row, including a NULL and a VARCHAR.
- In a join, `tuple_idx` picks the left or right tuple and each is read with its own schema.
- Expressions describe themselves (`#0.3`, `5`) and know their return type.
- Property: random rows and join rows read back exactly.

## Hints

### Tuple index versus column index

`tuple_idx` chooses *which tuple*, `col_idx` chooses *which column of that tuple's schema*. A bug that reads `col_idx` of the left tuple for a right-side column returns plausible numbers; the join test uses two different schemas to catch it.

### `evaluate` takes one tuple, `evaluate_join` two

For a column, `evaluate` is column `col_idx` of the one tuple; nothing in it looks at `tuple_idx`.

## Performance

Evaluating `#0.2` costs a schema lookup and a decode: an offset read for a fixed-size column, a follow-the-offset and a `String` allocation for a VARCHAR. A scan that evaluates an expression per row pays it for every row; this is why real engines evaluate expressions on whole columns at once (vectorised execution).

**Measure it.** Build a tuple of ten integer columns and evaluate `#0.9` a million times; repeat with a VARCHAR column. The difference is the allocation.

## Experiment

Optional. Predict first, then run.

1. **Resolve once.** Evaluate `#0.9` by name each time (look the column up in the schema) instead of by position. How much slower is it?
2. **Share or copy.** Replace `Arc<dyn Expression>` by `Box` in a tree you build by hand. What can you no longer do?

## Other designs

- **A tree of trait objects (ours, BusTub's).**
- **One `enum Expr`** with a big `match` in a single `eval` function: no vtable, easier to optimise, closed to extension.
- **Compile to a closure or bytecode** and run that per row.
- **Vectorised evaluation:** each node produces a whole column per call.

## In BusTub

`constant_value_expression.h` (`return val_;`) and `column_value_expression.h` (`return tuple_idx_ == 0 ? left_tuple->GetValue(&left_schema, col_idx_) : right_tuple->GetValue(&right_schema, col_idx_);`). BusTub's `abstract_expression.h` says expressions are "modeled as trees, i.e. every expression may have a variable number of children".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class AbstractExpression { virtual Value Evaluate(...) const = 0; }` | `trait Expression { fn evaluate(&self, ...) -> Result<Value>; }` |
| `std::shared_ptr<AbstractExpression>` | `Arc<dyn Expression>` (`ExprRef`) |
| `dynamic_cast<const ColumnValueExpression *>(expr.get())` | `expr.as_any().downcast_ref::<ColumnValueExpression>()` |
| `const Tuple *tuple` (may be `nullptr`) | `&Tuple`; `Tuple::empty()` stands for "no row" |

**Port rule:** a class hierarchy with virtual methods becomes a trait; `shared_ptr` of the base becomes `Arc<dyn Trait>`; `dynamic_cast` becomes a downcast through `Any`.

## Learn more

- [Trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [`Any`](https://doc.rust-lang.org/std/any/trait.Any.html) · [Crafting Interpreters: evaluating expressions](https://craftinginterpreters.com/evaluating-expressions.html)
