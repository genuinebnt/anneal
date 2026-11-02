A table has a **schema**: an ordered list of columns, each with a name, a type and, for text, a maximum length. The schema is also the **layout of a row**: where in a tuple's bytes each column starts. A row (module 3b's tuple) stores its columns one after another in a *fixed part*, and a text column's slot holds not the text but the offset where the text is, in a *variable part* after the fixed one. This stage builds `Column` and `Schema`; the next builds tuples on top.

> [!CHECK] A schema has columns `a INTEGER, b VARCHAR(20), c BIGINT`. Why does `b` take 4 bytes in the fixed part rather than 20? Where will its text go, and what does the 4-byte slot hold? What does that buy when tuples are read?
> ||The fixed part must have the same size for every row so that a column's offset is a constant of the schema; a 20-byte reservation would waste space for short strings and break for long ones. The slot holds the **offset** (within the tuple) where the text is stored in the variable part after the fixed part. Reading column `c` is then `data[offset_c..]` with no need to look at the text of `b` first, and a tuple takes exactly the space its text needs.||
>
> - What is the offset of `c`?
> - What is the length of the fixed part?
> - Which columns are not inlined?

## The task

`Column` (fields given: name, type, length, offset): `Column::new(name, type)` for fixed-size types (panics for `Varchar` and `Invalid`: wrong constructor), `Column::new_varchar(name, length)`, `with_column_name`, `is_inlined()` (everything but `Varchar`), `to_string(simplified)` in BusTub's two formats. `storage_size()` is the type's size, or the declared length for text; `offset()` is where the column starts, set by the schema.

`Schema::new(columns)` lays the columns out left to right: each gets its **offset**; a fixed column advances the offset by its size, a text column by **4** (the slot). The fixed length is the final offset; the schema remembers which columns are not inlined. `copy_schema(from, attrs)` is a new schema of the chosen columns **in that order, with new offsets**; `try_col_idx(name)` is the index of the first column with that name; `col_idx` panics for a missing one; the counts and `to_string` in both formats.

The tests: example checks for the constructors and formats; properties over random column lists: every offset is the sum of the sizes before it, the length is the total, the non-inlined list and counts are right, `copy_schema` equals building a schema from the cloned columns, names are found by their first occurrence.

## Your freedom

How the schema computes its layout (a loop, a `fold`, `scan`) and what extra it caches. The struct's fields are given; the layout rule is BusTub's.

## The Rust toolbox

**Private fields and accessors.** The struct's fields are private; `pub fn name(&self) -> &str` returns a borrow and `pub fn offset(&self) -> u32` copies a small value. Only the schema may set an offset (`pub(crate) fn set_offset`), so a column's offset cannot be changed from outside the crate.

**A running offset with a `for` loop.** `let mut offset = 0; for column in columns { column.set_offset(offset); offset += size; }`, collecting the placed columns into a new `Vec`. `into_iter().enumerate()` gives the index for the non-inlined list.

**`Vec<Column>` into `Schema`: ownership.** `Schema::new(columns: Vec<Column>)` takes ownership, so the schema can change the offsets in place; `copy_schema` clones from a borrowed schema (`.clone()` on each column).

**`position` for "first match".** `columns.iter().position(|c| c.name() == name)` returns `Option<usize>`.

**`assert!` vs `Option`.** A column constructor with the wrong type is a bug in the caller (panic with a message); a name that may not exist is a normal question (`Option`).

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves) and [L2 Borrowing](/t/l2-borrowing): `Vec<Column>` by value, `&Column` out.
- [S3 Vec & slices](/t/s3-vec-slices): `iter`, `position`, `into_iter().enumerate()`.
- [S1 Option & Result](/t/s1-option-result).
- L7 Enums & pattern matching: Patterns in depth: matching on column types.

## Tests

- Fixed columns have their type's size; a varchar its declared length and is not inlined; renaming keeps everything else.
- Wrong constructors and missing names panic.
- Columns and schemas print in BusTub's two formats; the empty schema is fine.
- Offsets are consecutive with 4-byte slots for text; the schema knows its non-inlined columns.
- `copy_schema` and name lookup.

## Hints

### The offset of the next column

Write a three-column example on paper: `a INT, b VARCHAR(20), c BIGINT`. Offsets 0, 4, 8 and a length of 16.

### The empty schema

No columns means length 0 and `is_inlined()` true. Does your loop handle it without a special case?

## Performance

A schema is built once per table and read for every tuple: accessors should be inlined and offsets precomputed (they are). Cloning a schema per row would be a bug; modules pass `Arc<Schema>` (`SchemaRef`) around.

**Measure it.** Look up a column's offset a hundred million times through `schema.column(i).offset()`: it is a bounds check and a load.

## Experiment

Optional. Predict first, then run.

1. **Alignment.** Change the layout so every column starts at a multiple of its size (insert padding). What does a row of `BOOLEAN, BIGINT` cost now and what do you gain on x86?
2. **Reorder.** Sort columns by decreasing size inside the schema (a hidden layout) while keeping the user's order for output. What would the extra indirection cost?

## Other designs

- **Offsets in the schema (ours, BusTub's).** Computed once.
- **Compute offsets on demand** from the type sizes: no stored state, more work per access.
- **Aligned layouts** (PostgreSQL's): faster loads, wasted padding.
- **Columnar storage**: no row layout at all; one array per column.

## In BusTub

`Column`, `Schema` and `Tuple` are classes in `src/catalog` and `src/storage/table`; `TablePage` is the slotted page in `src/storage/page/table_page.{h,cpp}`. BusTub's `TableHeap` (module 3c) links `TablePage`s into a table.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Column(std::string name, TypeId type)` and `Column(name, VARCHAR, length)` constructors | `Column::new` and `Column::new_varchar` |
| `std::shared_ptr<const Schema>` | `Arc<Schema>` (`SchemaRef`) |
| `const std::vector<Column> &GetColumns() const` | `fn columns(&self) -> &[Column]` |
| `BUSTUB_ASSERT(...)` | `assert!` with a message |

**Port rule:** a getter returning a const reference becomes a method returning a borrow; a shared pointer to a const object becomes an `Arc`.

## Learn more

- [`Iterator::position`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.position) · [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html)
