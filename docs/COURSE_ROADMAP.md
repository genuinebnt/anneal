# BusTub course: what is built and what comes next

Written 2026-10-10 from the planning discussion. Status words: **built** (shipped and verified), **approved** (the owner said to do it), **proposed** (suggested, waiting for a yes), **advanced** (already in [ADVANCED_DB_COURSE.md](ADVANCED_DB_COURSE.md), do not build twice). Order inside each list is the recommended order of work. Nothing proposed is started until the owner says go.

## 1. Built

The optional Rust on-ramp (r, 5 stages), modules 1a to 4c, and the optional primers 0a to 0d. See [BUSTUB_RESTRUCTURE.md](BUSTUB_RESTRUCTURE.md) §8.

### Built since the first version of this list

- **3i, the SQL surface** (8 stages and 6 challenges; BEYOND BusTub): the operators BusTub's engine lacks (`*`, `/`, `%`, `||`, `NOT`, unary minus, `IS [NOT] NULL`, `+`/`-` on every numeric type), `BETWEEN` and `IN` (NULL traps), `CASE`, `COALESCE`, `NULLIF`, `LIKE` (no exponential backtracking), `OFFSET`, `DISTINCT` aggregates, `EXPLAIN ANALYZE` and prepared statements (`?`). Boss: a script plus random expressions and conditions against a three-valued-logic model. Mutation-checked (24 mutants, all caught). Concept articles: `case-expressions-and-lazy-evaluation`, `like-matching-without-backtracking`, `explain-analyze-and-operator-statistics`, `prepared-statements-and-parameter-binding`.
- **4e, SQL transactions** (5 stages and 6 challenges; BEYOND BusTub): a `Session` with autocommit, `BEGIN`/`COMMIT`/`ROLLBACK`, `BEGIN ISOLATION LEVEL`, per-session default level, failed transactions (PostgreSQL's rule), and a boss that runs sessions against each other (bank with retries, consistent readers, no lost update, write skew at snapshot but not serializable). Concept article: `sessions-and-the-transaction-state-machine`. This is the SQL half of the transaction-statements item below: it is a module of its own because it needs the transaction manager of 4a and 4b, which come after 3i.
- **The BEYOND tag:** modules that port none of BusTub's files, and every challenge, carry a BEYOND (BUSTUB) tag on the Courses page (`beyond = true` in a stage.toml flags a single stage).

- **Challenges** (`kind = "challenge"`): optional extra exercises after a module. Own tally (`challenges`, `challenges_done`), never `current`, never block unlocking, no solution uploaded, CHALLENGE tag. Pages say what and why only, plus the invariants, the relations between input and output, and worked examples (lint-enforced). **148 shipped, at least five per module** (R, 0a-0d, 1a, 1b, 1f, 1g, 2a-2d, 3a-3j, 4a-4e); each has at least five tests, and every one is checked in all three tiers: **Build** (write it from the contract), **Extend** (builds on your own Trie, SkipList, hash set, ...) and **Debug** (the stub lines *are* the bug). The replacer modules 1c, 1d (two each) and 1e (none) are deliberately not extended: a replacer's policy is the module, there is nothing more to practise. Tests live in the module's own test file inside `mod ch_<id>` with the prefix `s<code>_c<n>` / `sr_c<n>`. The pages and Rust are produced by `tools/course_challenges/gen.py` from the packs in `tools/course_challenges/packs/` (one `C(...)` per challenge); run it with `--root <author tree>`, it is idempotent.
- **Listening run strip:** the stage page shows "listening", "running tests…" (the CLI posts `runs/start`) and then the result; the button says "Copy test command".
- **Reset progress:** the whole course, a project or a module (Courses page and `anneal course reset --module M | --all`).

## 2. Approved

| # | Item | Size | Notes |
|---|---|---|---|
| 1 | **4d Lock manager** (standalone 2PL), **built** (7 stages; the hybrid store and the anomaly lab are the optional challenges 4d-c6 and 4d-c7) | 7 stages | Modes IS/IX/S/SIX/X and the compatibility matrix; one request queue; blocking and FIFO; upgrades; 2PL and isolation levels with intent locks; deadlock detection (waits-for graph, abort the youngest); boss: strict-2PL store equals a serial order, and the same workloads under MVCC are serializable. Plan in the planning thread; open decisions in §5. |
| 2 | "Errors you will meet" pages, one per module, **built** | 28 concept pages `errors-<code>` | 21 error families, each produced and checked with a real compiler (the bad program, the message, the fix), mapped to the modules that provoke them; linked as optional concepts from every stage. The generator is /tmp/c2/gen/errfam.py, errmods.py and errgen.py (copy them into tools/ before they are lost). |
| 3 | `anneal course adopt <stage>` | CLI only | Copy our implementation of a stage into the learner's repo. |
| 4 | Rewrite the older concept articles in the newer style | ongoing | Quality pass, no new content. |
| 5 | Cost-based optimizer and an LSM tree | see §4 | The owner approved "1-5" of the earlier roadmap, which included these two. They are also in the advanced course; see the decision in §5. |

## 3. Proposed, by area

### 3.1 Transactions and concurrency
- **Hybrid store** (**built** as the optional challenge `4d-c6`, Extend tier): snapshot reads that never wait, row locks from the learner's own lock manager for writes that wait instead of aborting, and `get_for_update`.
- **Isolation-anomaly lab** (**built** as the optional challenge `4d-c7`, Build tier): a given step-driven database at five isolation levels; the learner writes the five scenarios (dirty read, non-repeatable read, lost update, write skew, phantom) and predicts the table, which the tests compare with what the scenarios observe. The engine is `src/concurrency/lab_db.rs`, tied to 4d with `files = [...]`.
- **Key-range / next-key locking** with the lock manager, to stop phantoms under 2PL. (about 2 stages)
- **Optimistic concurrency control** (validation at commit). (about 3 stages)
- **Time-travel queries** (`AS OF`) from MVCC. (1 to 2 stages)
- Lock escalation; index concurrency beyond latch crabbing (OLC). Low value; leave.

### 3.2 SQL surface (new modules after 3h, not edits to 3d)
- **3i SQL surface** (**built**, see above) and **4e SQL transactions** (**built**). Still open from this item: the `FOR UPDATE` syntax (the hybrid store of challenge 4d-c6 is its engine side) and `HAVING` on window functions.
- **3j, outer joins and set operations** (**built**; 7 stages and 5 challenges; BEYOND BusTub): `RIGHT`/`FULL` joins (an executor that keeps the right side with a flag per row), filter pushdown that respects the join type, outer-join simplification from NULL-rejecting filters, and `UNION`/`INTERSECT`/`EXCEPT` with and without `ALL` (counts, NULLs equal, precedence, parentheses, `ORDER BY`/`LIMIT` on the whole). Boss: the join and set identities plus optimized-equals-plain over random queries. Mutation-checked (26 mutants, all caught after two tests were strengthened). Concept articles: `outer-joins-and-null-padding`, `filters-above-and-below-outer-joins`, `set-operations-and-bag-arithmetic`. Challenges: a hash outer join, the customer with one order (count(*) after a LEFT join), the union that kept a duplicate, set operations by merging, which filters reject NULLs.
- **3k Errors worth returning** (built): one `DbError` for the whole database, with causes, context, SQLSTATE-style codes and a client-safe message. Subqueries and CTEs (scalar, `IN` and `EXISTS` subqueries, `WITH`, correlated subqueries and decorrelation) are still to come under another code.
- DDL and constraints (`PRIMARY KEY`, `NOT NULL`, `UNIQUE`, `CREATE INDEX`, `DROP`, `ALTER TABLE ADD COLUMN`, views): fold into 3i or a small module, by size.
- Types (`DATE`, `TIMESTAMP`, `DECIMAL`): leave until last; it changes 3a and 3b.
- Depth items, lower value: `GROUPING SETS`/`ROLLUP`/`CUBE`, `DISTINCT` aggregates, window frames, recursive CTEs, `LATERAL`, JSON and array types, sequences, foreign keys, triggers, materialized views.

### 3.3 Storage: variable-length and large data
- **Large and variable-length data module (project 3, after 3c):** overflow pages (TOAST-style), `VARCHAR(n)` with limits and collation, tuple growth on update with stable row ids (forwarding), a free space map and page compaction (vacuum), a NULL bitmap and alignment, compression of large values. Recommended first pick of this group: overflow + tuple growth + vacuum.
- **Variable-length and composite B+ tree keys** (optional module after 2d; it changes the 2c layout), prefix compression and suffix truncation, unique/secondary/covering indexes.
- **Check first:** whether the catalog is persisted and a database reopens (BusTub's is in memory); then a persistent catalog and a file header with magic number and format version.

### 3.4 Execution
- **Disk-spilling hash aggregation and a Grace hash join** (high value; different skill from the radix join in the advanced course).
- **Bloom filter primer** (standalone structure plus a join filter); needed later by the LSM.
- Sort-merge join, semi and anti joins; B+ tree bulk loading; page compression; PAX layout bridge.

### 3.5 Recovery and durability
- **Point-in-time recovery and backup from the log** (small extension of 4c).
- Page checksums and torn-page detection; ARIES with page LSNs (optional extension); log shipping to a read replica; background flusher.

### 3.6 Testing and trust (cheap, multiplies everything built)
- **Differential testing against SQLite** (random SQL on both engines).
- **Deterministic simulation testing** (injected disk, clock and scheduler; replay from a seed; 4c's crash disk is the first piece).
- **Isolation-level checker** (record a history, check it against snapshot isolation and serializability); a natural boss for 4b and 4d.
- **Benchmark harness:** YCSB, TPC-C, TPC-H-lite. The capstone needs numbers.
- SQL fuzzer (no panics, good errors).

### 3.7 Distributed (the advanced course has Raft only)
- **Two-phase commit and a coordinator** (the missing piece between recovery and Raft).
- Sharding and partitioning, primary–backup replication and consistency models, distributed execution with exchange operators.

### 3.8 System layer
- Server with thread pool, connection handling, statement timeouts and cancellation; per-query memory budgets and admission control; roles and `GRANT`; slow-query log and metrics; async and direct I/O, readahead.
- Modern structures not in either course: adaptive radix tree, pointer-swizzling buffer manager; Bw-tree, cuckoo hashing and learned indexes (low).
- Specialised indexes not in the advanced course: inverted index with BM25 (medium); R-tree and bitmap (low).

## 4. Already in the advanced course (do not build in BusTub)

Columnar storage and encodings, vectorised execution, query compilation, morsel-driven parallelism, radix and multi-way hash joins, the wire protocol (Part A); the Cascades optimizer, join ordering, statistics and cost models (Part B); LSM trees, Raft, cloud table formats (Part C); vector databases and time-series (Part D).

## 5. Decisions the owner still has to make

1. **Lock manager (4d):** table and row hierarchy with intent locks (recommended) or flat locks; detection only (recommended) or also wait-die; include the hybrid stage 4d-08 (recommended, optional); place it after 4c as 4d (recommended).
2. **Cost-based optimizer and LSM:** build them in the advanced course (recommended, no duplicate work) or as simpler BusTub-side modules now.
3. **Errors pages:** one per module (recommended) or one per project.
4. **SQL surface:** which groups (recommended: the everyday gaps, outer joins and set operations, subqueries and CTEs, transaction statements; constraints if cheap); new modules after 3h (recommended); types left for last.
5. **Storage gaps:** which groups to take (recommended first: overflow + tuple growth + vacuum); whether variable-length B+ tree keys may change 2c or stay an optional module; check catalog persistence first.
6. **Extras:** which of the testing items, benchmark harness, key-range locking, 2PC and time-travel queries to keep (recommended: all five).

## 6. Recommended order if every recommendation is accepted

1. 4d lock manager (+ anomaly lab, key-range locking, hybrid stage)
2. "Errors you will meet" pages and `anneal course adopt`
3. SQL: 3i, 3j, 3k
4. Testing: SQLite differential test, simulation testing, isolation checker, benchmark harness
5. Large and variable-length data module; spilling aggregation and Grace hash join; Bloom primer
6. Point-in-time recovery; 2PC and coordinator; time-travel queries
7. Concept article rewrite (alongside the above)
8. Then the advanced course, Part B first ([ADVANCED_DB_COURSE.md](ADVANCED_DB_COURSE.md) §5)
