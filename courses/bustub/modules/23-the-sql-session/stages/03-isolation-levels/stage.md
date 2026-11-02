Your transaction manager can begin a transaction at one of three levels, and so far nothing in SQL can ask for one: the session always uses snapshot isolation. The level is the contract a client buys: which of the anomalies from the lab of module 4d it may see. This stage lets a client say what it wants, `begin isolation level serializable`, or set a default for its session, and then **shows the difference** in the one place it matters: the same interleaving of two transactions commits at snapshot isolation and is refused at serializable.

> [!CHECK] Transaction A reads the whole table. Transaction B updates one row of it and commits. Transaction A then inserts a different row and commits. At snapshot isolation A commits; at serializable it must not. What exactly did A do that makes the second case wrong, and why is it fine at snapshot isolation?
> ||A made a **decision on data that is no longer true**: it read the table, B changed a row A had read, and A's later write may rest on the old value. At snapshot isolation that is allowed: A reads a consistent snapshot, and writes conflict only if they touch the *same row* (first committer wins). Serializable promises a result equivalent to *some* serial order, and no order is consistent with A having read before B's change and written after it, so the commit-time validation sees that A's read set was invalidated by a later commit and refuses.||
>
> - Which levels does the engine have? Which common ones does it not?
> - Does a level named in `begin` apply only to that transaction?
> - What does autocommit use?

## The task

- **Parsing** (in `src/sql/parser.rs`, the `begin` branch of `statement`; the AST variant `Statement::Begin { isolation: Option<String> }` is given): `begin [transaction] isolation level <words>`, the words lower-case and joined by single spaces (`serializable`, `repeatable read`, `read uncommitted`, `snapshot`). A missing name is a syntax error.
- **The session** (`src/common/session.rs`): the level names `read uncommitted`, `repeatable read` and `snapshot` (the last two are the same thing: snapshot isolation) and `serializable`. Anything else, `read committed` included, is a `NotImplemented` error and opens no transaction.
- **A session default:** `set default_transaction_isolation = 'serializable'` sets the level `begin` and autocommit use when none is named; it belongs to the session (another session is not affected).
- `Session::isolation_level()` is the level of the open transaction (`None` without one).
- **The difference:** nothing else to write: the transaction manager already validates serializable transactions at commit (4b). The tests check that your session passes the level through.

## Your freedom

Where the level name is turned into `IsolationLevel` (the parser could do it; this reference keeps the parser a syntax-only layer and maps names in the session), and whether names are case-insensitive (they are in SQL).

## The Rust toolbox

**Words until something else.** `while let Some(Token::Word(w)) = self.peek().cloned() { ... }` collects `read uncommitted` without knowing its length.

**A `match` on a `&str`.** `match name.trim().to_lowercase().as_str() { "serializable" => ..., "repeatable read" | "snapshot" => ..., other => Err(...) }`: aliases are or-patterns.

**A guard in a pattern.** `Statement::Set { name, value } if name == "default_transaction_isolation"` picks one variable out of all `SET`s.

## If this is new

- L7 Enums and patterns: guards and or-patterns.
- [S2 Strings and text](/t/s2-strings-text): case-insensitive comparison without allocation surprises.

## Tests

- `begin` takes each level name, with extra spaces and capitals.
- The default is per session and is overridden by a level in `begin`.
- Unsupported and unknown levels are refused and open nothing; a missing name is a syntax error.
- Snapshot reads are repeatable and see no phantoms.
- The same interleaving commits at snapshot isolation and is refused at serializable.

## Hints

### Parse words, map them in the session

The parser's job is to read `isolation level serializable` into a string; deciding that `repeatable read` means snapshot isolation is a decision about this *engine*, which belongs one layer up.

### Refuse before you begin

Convert the level *before* asking the transaction manager for a transaction. A refused level must not leave a registered transaction behind.

### The commit error is yours to produce

When validation fails the transaction manager has already aborted the transaction and returned `false`. The session must report an error **and** consider the transaction over.

## Performance

Serializable costs more than snapshot: the transaction records what it read (scan predicates, here) and validates at commit by looking at everything that committed meanwhile. The cost grows with the read set and the number of concurrent commits; a snapshot transaction does neither. That is why snapshot isolation is the default in most systems and serializable is opt-in.

**Measure it.** A transaction that scans 100 000 rows and updates one, at snapshot and at serializable, with and without concurrent writers: where does the extra time go?

## Experiment

Optional. Predict first, then run.

1. **Always begin at serializable.** Which tests fail (the write skew one) and which pass?
2. **Map `read committed` to snapshot** instead of refusing it. Is that honest? What anomaly would a client that asked for read committed be protected from that it did not ask for, and what does it lose?

## Other designs

- **Parse the level into an enum** in the parser and carry it in the AST: stricter and more code; the string is enough while the session is the only consumer.
- **Per-statement levels** (`select ... for update`, `set transaction`): finer control; most systems offer a transaction-wide level only.
- **Read committed** as a real level (each statement sees the commits up to its own start): easy to add on top of this engine's versions, and what PostgreSQL uses by default.

## In BusTub

BusTub's `IsolationLevel` has `READ_UNCOMMITTED`, `SNAPSHOT_ISOLATION` and `SERIALIZABLE`, and its shell's `begin` takes no level; the transaction manager (project 4) is where the levels act. The SQL syntax for choosing one is this module's.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `enum class IsolationLevel` and a `std::map<std::string, IsolationLevel>` | an `enum` and a `match` on the name |
| `strcasecmp` | `to_lowercase()` once, then a `match` |
| a `SET` handled in one `if` chain | a `match` arm with a guard |

**Port rule:** unsupported input is an error that names it, not a silent substitute.

## Learn more

- [PostgreSQL: transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html) · [A Critique of ANSI SQL Isolation Levels (Berenson et al.)](https://arxiv.org/abs/cs/0701157)
