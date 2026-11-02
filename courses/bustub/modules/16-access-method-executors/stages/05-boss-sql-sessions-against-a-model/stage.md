**Where this fits.** Scans, inserts, deletes, updates and index scans work one by one. This stage runs them **together**, as a user would: a random session of SQL statements against a table with an index created at a random moment, checked after every statement against plain Rust vectors, plus BusTub's own SQL tests for the same executors.

> [!CHECK] After any sequence of inserts, deletes and updates, name the invariants that must hold between the table, its index and a plain vector of rows. Which of them can a *single-statement* test never catch, and which of your executors would break first if you forgot one?
> ||(1) The table's live rows equal the vector, as a multiset. (2) The index holds exactly one entry per live row, with the right key, and no entry for a dead row. (3) A full index scan lists the rows in key order. The index/table agreement is the one a single statement cannot catch: an update that forgets to drop the old entry is invisible until a later scan or lookup, and an index created *after* some statements must start with the rows already there (module 3c's `create_index`).||
>
> - Which statement did the test run last when it failed?
> - Does the failure disappear if you create the index earlier or later?
> - What is the smallest session that shows it?

## The task

Nothing new to write. Make both pass:

- **`stages_3e::s3e_05`**: a random session of up to two dozen statements (inserts of rows with NULLs, deletes, updates of `b`, `c` or the key `a` by random predicates, selects with random predicates), with `create index ia on t(a)` at a random point. After every statement: the statement answers with the right count; `select * from t` equals the vector; once the index exists, it agrees with the table (every live row once, no dead entries) and `select * from t order by a` comes back in key order (the optimizer, given, rewrites it to an index scan).
- **`sql_access_methods_test`** (`cargo test --test sql_access_methods_test`): BusTub's `p3.00-primer`, `p3.01-seqscan`, `p3.02-insert`, `p3.03-update` and `p3.04-delete` files. (`p3.05` and `p3.06` need the optimizer rules of module 3h, so they come later.)

## Your freedom

None new: a failure belongs to one of your executors.

## The Rust toolbox

**Reading a proptest counterexample.** It prints the session that failed (shrunk to the fewest steps) and the statement text. Paste the statements into the shell (`cargo run --bin bustub_shell`) in order and look at the table after each; or write them as a `#[test]` with `sql(&db, "...")` lines.

**`explain`.** `explain (o) select * from t where a = 3` shows whether a scan became an index scan.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Build it: a model of SQL in plain vectors; random sessions of statements.

## Tests

- The random session property (table, index and order agree with the model after every statement).
- BusTub's five `.slt` files.

## Hints

### The first statement that breaks

If a session fails after an `update` of the key, look at the index entry of the *old* key; after a `delete`, at the entry of the dead row; after an `insert` before the index exists, at `create_index` (module 3c).

### A selective filter and the batch

If `select ... where` loses rows only when many match, the scan returns after the first batch (it must return `true` while the batch is non-empty and `false` only when it is empty).

## Performance

A session of two dozen statements on tables of tens of rows runs in milliseconds; the BusTub files run a few thousand rows each (a few seconds in debug mode).

## Experiment

Optional. Predict first, then run.

1. **Break one thing.** Make `delete` skip the index. Which statement in the session shrinks to the smallest failing one?
2. **Index before or after.** Always create the index first. Does any bug in `create_index` hide?

## Other designs

None for this stage. The *Other designs* sections of 3e-01 to 3e-04 list the alternatives to compare with yours.

## In BusTub

The executors of Project 3, task 1 (`seq_scan_executor.cpp`, `insert_executor.cpp`, `update_executor.cpp`, `delete_executor.cpp`, `index_scan_executor.cpp`) are stubs (`UNIMPLEMENTED("TODO(P3): Add implementation.")`) with the contract in the header comments. The 2025 BusTub executors are **batched**: `Next(std::vector<Tuple> *tuple_batch, std::vector<RID> *rid_batch, size_t batch_size)` fills the vectors with up to `batch_size` rows and returns whether it produced any.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `./bin/bustub-sqllogictest ../test/sql/p3.01-seqscan.slt --verbose` | `cargo test --test sql_access_methods_test p3_01_seqscan` |

**Port rule:** the same `.slt` files, run by a Rust port of the runner.

## Learn more

- BusTub's [Project 3 page](https://15445.courses.cs.cmu.edu/fall2025/project3/) · [sqllogictest](https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki)
