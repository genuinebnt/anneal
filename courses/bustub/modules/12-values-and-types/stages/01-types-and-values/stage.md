Everything above the storage layer talks in **values**: a number, a boolean, a string, a timestamp, or **NULL**, which still has a type. This module builds the `Value` type and its operations, the foundation of expressions, tuples and every executor in the next projects. The data model is given, BusTub's own: a `TypeId` enum (`Boolean`, `TinyInt`, ..., `Varchar`, `Timestamp`) and a `Value` enum with one variant per type plus `Null(TypeId)`. You write what is true of each type and how values are made.

> [!CHECK] BusTub stores a 32-bit integer NULL as the number `-2 147 483 648` (`i32::MIN`) and never lets that number be a real value. Why does it do that instead of keeping a separate "is null" flag? What does it cost, and where does the choice show up when you construct and when you compare values?
> ||A flag per value would need an extra byte (or bit) next to every integer in every page; the reserved number needs nothing and the stored form is just the integer. The price: one number of each type cannot be used as data (the smallest usable `INTEGER` is `i32::MIN + 1`), and a constructor must turn the reserved number into a NULL. It shows up in the constructors (`Value::integer(i32::MIN)` is a NULL), in `min_value` (one above the reserved number) and in every range check later.||
>
> - What would `Value::integer(i32::MIN)` return?
> - Which numbers can a `TINYINT` really hold?
> - What type does a NULL have?

## The task

In `type_id.rs` and `value.rs`:

- `TypeId::type_size()` (bytes of the type in a tuple's fixed part; `Varchar` is 0, `Invalid` is an `UnknownType` error), `type_id_to_string()` (`"INTEGER"`, ...), `is_coercable_from(other)` (BusTub's table: `Invalid` accepts nothing, `Boolean` accepts anything, the numeric types accept numerics and `Varchar`, `Timestamp` accepts `Varchar` and itself, `Varchar` accepts every type but `Invalid`).
- `min_value()` and `max_value()` per type (the smallest usable value; `Varchar`'s min is the empty string and its max is NULL; `Invalid` is a `MismatchType` error).
- The constructors `Value::null(t)`, `boolean`, `tinyint`, `smallint`, `integer`, `bigint`, `decimal`, `timestamp`, `varchar`: for each numeric type the **reserved number** (`i8::MIN`, ..., `f64::MIN` for decimals, `u64::MAX` for timestamps) gives a NULL of that type.
- The accessors `type_id`, `is_null`, `as_i64` (widens any integer; `None` for NULL and non-integers), `as_f64`, and `check_comparable(other)` (which types may be compared), and `Display` (BusTub's text: `true`, integers in decimal, decimals with six digits, a NULL as `integer_null`, `varlen_null`, ...).

The tests: the sizes and names, **the whole coercion table** written out, properties for the reserved number over random numbers of each width (a value holds its number unless it is the reserved one, in which case it is a NULL of that type; a NULL has a type and no number), min and max are values in order, display strings, the comparability table.

## Your freedom

Nothing about the data model (it is BusTub's); how you write the matches and in what order is yours. The point of this stage is the discipline of an exhaustive `match`.

## The Rust toolbox

**An enum with data.** `enum Value { Null(TypeId), Boolean(bool), Integer(i32), Varchar(String), .. }`: each variant carries its own payload and the compiler checks that every `match` covers all of them. Adding a variant later makes every non-exhaustive match a compile error, which is the point.

**`match` with guards and or-patterns.** `TinyInt | SmallInt | Integer | BigInt => ..` groups arms; `x if x == RESERVED => Value::Null(..)` adds a condition.

**Constants named after what they mean.** `BUSTUB_INT32_NULL` in `limits.rs` is `i32::MIN`: use it rather than the literal, and the code says why.

**Widening with `as`.** `v as i64` of an `i8`/`i16`/`i32` is lossless and sign-extending; it is the only `as` in this stage that never loses anything.

**`Result` with a custom error.** `type_size` returns `Result<u64>` where the error is an `Exception { kind, message }`: `Err(Exception::new(ExceptionType::UnknownType, "Unknown type."))`.

## If this is new

- L7 Enums & pattern matching: enums with data, exhaustive `match`.
- [S1 Option & Result](/t/s1-option-result): `Option` for "no number", `Result` for errors.
- The optional concepts *enums with data and match*, *SQL types and three-valued logic* and *integers and casts* are the three ideas of the module.
- [S2 Strings & text](/t/s2-strings-text): Understand: UTF-8, case mapping, comparing strings.

## Tests

- Sizes and names of every type; `Invalid` is an error.
- The coercion table, exactly; every type is coercable from itself except `Invalid`.
- The reserved number makes a NULL of that type for every width; every other number survives; a NULL has a type and no number.
- Minimum and maximum values are real values, in order, with the special cases.
- Values print like BusTub; the comparability table.

## Hints

### Start from the table

The coercion table has 81 entries; write the two or three rules that generate it instead of 81 arms, and check your rules against the doc comment.

### The reserved number

Do the integer constructors first and the NULL idea will follow: one `if v == RESERVED { Value::Null(t) } else { Value::T(v) }` per type.

### Decimal NULL

A decimal has no "smallest integer", so BusTub reserves `f64::MIN`; comparing floats with `==` is exact here because it is a constant.

## Performance

Nothing here is slow. The interesting cost is the size of `Value`: an enum is as large as its largest variant plus a tag (`String` makes it 32 bytes). A tuple of ten columns materialised as `Vec<Value>` costs 320 bytes plus the strings; executors that pass rows around feel it.

**Measure it.** Print `std::mem::size_of::<Value>()`, and `size_of::<Option<Value>>()`. Predict both before you run them (niche optimisation applies to `Option`).

## Experiment

Optional. Predict first, then run.

1. **A flag instead of a reserved number.** Add `Integer(i32, bool)`. What does it cost in `size_of::<Value>()` and which functions in this stage get simpler?
2. **A new type.** Add a `Date` variant and see which `match`es the compiler complains about. Which of them would you have forgotten?

## Other designs

- **Reserved numbers (BusTub's, ours).** No extra space; one number per type is lost.
- **A separate validity bitmap per column** (Arrow, Parquet). Columns store plain numbers and a bit per value says "null": the usual design for columnar engines.
- **`Option<T>` per field** in a typed row struct: type-safe, no dynamic tag, but the schema is then fixed at compile time.
- **Dynamic dispatch** (`Box<dyn Type>` per value): BusTub's own C++ shape; slower and bigger.

## In BusTub

BusTub has one `Type` class per SQL type (`IntegerType`, `VarlenType`, ...) with virtual `Add`, `CompareEquals`, `CastAs` and friends, and a `Value` that holds a tagged union. This course keeps the data model (the `TypeId` enum and the `Value` enum are given) and puts every operation in `Value`'s methods, matching on the variants.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class Value { union Val {..} value_; TypeId type_id_; }` | `enum Value { Boolean(bool), Integer(i32), .. }` |
| `INT32_NULL = INT32_MIN` | `BUSTUB_INT32_NULL`, handled in the constructor |
| `virtual` methods per `Type` subclass | one `match` per method |
| `throw Exception(ExceptionType::UNKNOWN_TYPE, ..)` | `Err(Exception::new(..))` |

**Port rule:** a tagged union plus a class hierarchy becomes one enum, and each virtual call becomes a `match`.

## Learn more

- [The Rust Book: enums and `match`](https://doc.rust-lang.org/book/ch06-00-enums.html) · [Enum layout (niches)](https://doc.rust-lang.org/reference/type-layout.html)
- [SQL NULL on Wikipedia](https://en.wikipedia.org/wiki/Null_(SQL))
