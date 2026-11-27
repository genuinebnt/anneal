Your transaction manager can begin a transaction at one of three levels, and so far nothing in SQL can ask for one: the session always uses snapshot isolation. The level is the contract a client buys: which of the anomalies from the lab of module 4d it may see. This stage lets a client say what it wants, `begin isolation level serializable`, or set a default for its session, and then **shows the difference** in the one place it matters: the same interleaving of two transactions commits at snapshot isolation and is refused at serializable.

> [!CHECK] Transaction A reads the whole table. Transaction B updates one row of it and commits. Transaction A then inserts a different row and commits. At snapshot isolation A commits; at serializable this engine refuses it. Is the schedule really not serializable? Look for a serial order of A and B that gives the same reads and the same final table, and explain why the engine refuses either way.
> ||It **is** serializable: A, then B. A's read saw the table before B's update, and A's insert touches no row B read or wrote, so running all of A first and then all of B gives the same reads and the same table; PostgreSQL's serializable snapshot isolation commits this schedule. This engine's check is **conservative**: at commit it looks at every row written by a transaction that committed after A's read timestamp and refuses A if one of A's scan predicates matches that row, before or after the write. B's row matches A's full scan, so A is refused: a false positive, the price of a check that never asks whether some serial order exists. The anomaly serializable is there to stop is **write skew** (the two doctors of the boss stage): each transaction reads what the other writes, so neither order explains both, and snapshot isolation lets both commit.||
>
> - Run all of A before B: does A read the same sum? Does the table end the same?
> - What does the commit-time check compare, and does it ever look for a serial order?
> - In write skew, which transaction reads what the other one writes?

## The task

- **Parsing** (in `src/sql/parser.rs`, the `begin` branch of `statement`; the AST variant `Statement::Begin { isolation: Option<String> }` is given): `begin [transaction] isolation level <words>`, the words lower-case and joined by single spaces (`serializable`, `repeatable read`, `read uncommitted`, `snapshot`). A missing name is a syntax error.
- **The session** (`src/common/session.rs`): the level names `read uncommitted`, `repeatable read` and `snapshot` (the last two are the same thing: snapshot isolation) and `serializable`. Anything else, `read committed` included, is a `NotImplemented` error and opens no transaction.
- **A session default:** `set default_transaction_isolation = 'serializable'` sets the level `begin` and autocommit use when none is named; it belongs to the session (another session is not affected).
- `Session::isolation_level()` is the level of the open transaction (`None` without one).
- **The difference:** nothing else to write: the transaction manager already validates serializable transactions at commit (4b). The tests check that your session passes the level through.

## Your freedom

Where the level name is turned into `IsolationLevel` (the parser could do it; this reference keeps the parser a syntax-only layer and maps names in the session), and whether names are case-insensitive (they are in SQL).

## The Rust toolbox

**Items until something else.** `while let Some(Item::Name(n)) = items.peek().cloned() { .. }` consumes a run of one kind of item and stops at the first of another kind, so a name of one, two or three words needs no length.

**A `match` on a `&str`.** `match answer.trim().to_lowercase().as_str() { "yes" | "y" => .., "no" | "n" => .., other => Err(..) }`: normalise once, then aliases are or-patterns and `other` binds whatever is left for the error message.

**A guard in a pattern.** `Command::Set { key, value } if key == "colour" => ..` picks one case out of a variant; an arm without the guard further down takes the rest of that variant.

## If this is new

- [Patterns](https://doc.rust-lang.org/reference/patterns.html) (the Rust Reference): guards and or-patterns.
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
