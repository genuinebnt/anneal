# The SQL performance course (owner asked 2026-11-01)

**Goal (the owner's words):** "as high quality or better, with more content, than Use The Index, Luke ... a one-for-all reference for
query performance optimisation and everything about SQL performance. If multiple databases are needed, use them; if an analytical load
needs an analytical engine, use it." The content must be **researched** (official docs, source, papers), with the best breadth and depth.

**Status:** pilot started, **9 of about 47 modules written**: all of part I (1 `sql-anatomy`, 2 `sql-equality`, 3 `sql-multicolumn`,
4 `sql-functions`, 5 `sql-ranges`, 6 `sql-partial-null`, 7 `sql-obfuscated`) and part II's first two, 8 `sql-explain` and 9 `sql-access-filter`. The pipeline (plan capture, loader, reader,
plan view) works end to end for them. Everything below the "Built" section is still to do. Mockup (approved: "ok"):
https://claude.ai/artifact/2m75DYbPU1u4v5j2GjQTx2 (private to the owner's account; the course map, a reference page, a lab, the plan explorer).

Use Use The Index, Luke (use-the-index-luke.com, Markus Winand, all rights reserved) and the book *SQL Performance Explained* for
**topics only**: never copy its text or figures. Link to it under "Sources and further reading". Everything here is written for anneal
and measured on a real database.

## What is built

| Piece | Where | Notes |
|---|---|---|
| Example schema | `tools/sqlperf/schema.sql` | customers 50k, products 10k, orders 2M, order_items ~6M, deterministic (`setseed`); `psql -v scale=N` multiplies orders. Throwaway databases only. |
| Setups | `content/sql/setups.toml` | each plan's indexes are applied before and undone after (`sql`, `undo`), so the schema stays the same for every plan; a setup with `session` instead (planner settings such as `SET enable_seqscan = off`, to show the plan the planner rejected) runs in the session that takes the plan |
| Plan capture | `tools/sqlperf/capture.py` | finds ```` ```plan id=… setup=… ```` blocks in pages, runs them on a real Postgres (warm, one session), writes `content/sql/plans/<id>.json` (query, setups, JSON plan, text plan, server version). `--check` re-runs and fails if a plan's **shape** changed or a plan is unused. |
| Pages | `content/sql/pages/sql-*.toml` | the same format as System design pages (sections, checks, terms, sources, summary, figures); file names start with `sql-`; `group` is one of `SQL_GROUPS` in `crates/content/src/sd.rs` |
| Loader | `crates/content/src/sd.rs` | SQL pages join the System design catalog (so checks, review cards, ⌘K and search work unchanged); plans load into `catalog.plans`; a ```` ```plan ```` block without a captured plan is a content error |
| API | `crates/api/src/sd.rs` | `GET /api/sd/concepts/{id}` returns `plans` (the ones the page shows) |
| Reader | `web/src/pages/sd/SdRead.tsx`, `web/src/components/SdMd.tsx` | the reading view; `PlansContext` hands the plans to `PlanBlock` |
| Plan view | `web/src/plan.ts`, `web/src/components/PlanView.tsx`, `web/src/styles/plan.css` | parsers for Postgres JSON and text, MySQL `EXPLAIN ANALYZE` / `FORMAT=TREE`, SQLite `EXPLAIN QUERY PLAN`; per-step own time and pages; flags (misestimates, rows discarded by a filter, disk sorts, hash batches, heap fetches, missing workers); an operation knowledge base (`KB` in plan.ts); bars grow in run order; hottest step marked. Unit tests in `plan.test.ts` run against the captured plans. |
| Module 1 | `content/sql/pages/sql-anatomy.toml` | anatomy of an index, measured with `pageinspect`: 3 plans, 4 checks, 10 terms, 9 sources. **The quality bar for every other module.** |
| Module 2 | `content/sql/pages/sql-equality.toml` | equality, primary and unique keys (with bigint vs UUIDv4 vs UUIDv7 measured), selectivity and why a 5.5% value reads every page, IN/ANY/OR and `Index Searches`, checking index use (a forced plan, `pg_stat_user_indexes`), MySQL/SQLite/DuckDB/ClickHouse measured: 8 plans (Postgres 18.6), 7 checks, 12 terms, 17 sources |
| Module 3 | `content/sql/pages/sql-multicolumn.toml` | multi-column indexes: the leftmost prefix, the column-order centrepiece (a customer's latest 20 orders: 3,116 pages with `(created_at, customer_id)`, 23 with `(customer_id, created_at)`, 4 covering; the 3,116 explained to the page), most-selective-first tested, Postgres 18 skip scan (and why text needs about two searches per value), redundant prefix indexes, `CREATE INDEX CONCURRENTLY` in an sqlx migration, MySQL/SQLite measured, ClickHouse key order: 10 plans (Postgres 18.6), 7 checks, 8 terms, 13 sources |
| Module 4 | `content/sql/pages/sql-functions.toml` | functions and casts on a column (the forced plan that still seq-scans), expression indexes and their statistics, immutability (`created_at::date` refused, `AT TIME ZONE 'UTC'` accepted), case-insensitive search four ways (`lower()`, `citext`, nondeterministic ICU collation, generated column; `casefold()`), the B-tree integer operator family behind which casts keep an index, bind parameters (protocol, custom vs generic plans, parameter types; an sqlx `f64` bind measured to turn a lookup into a Seq Scan), MySQL/SQLite/DuckDB measured: 8 plans (Postgres 18.6), 7 checks, 10 terms, 16 sources |
| Module 5 | `content/sql/pages/sql-ranges.toml` | ranges as leaf walks (a one-day range reads 2,993 pages for 3,307 rows, a month reads more pages than the table has), half-open vs `BETWEEN`, equality before range, `LIKE` as a range (`C.UTF-8` is not `C`: needs `COLLATE "C"` or `text_pattern_ops`; `^@`; generic plans lose it), bitmap AND/OR: 10 plans (Postgres 18.6), 5 checks, 7 terms, 8 sources |
| Module 6 | `content/sql/pages/sql-partial-null.toml` | partial indexes (308 pages vs 7,950 for the same top-20), implication and the generic-plan trap, NULL in B-trees, `= NULL`, unique and `NULLS NOT DISTINCT`, the `tickets` table (setup `tickets`: 500k rows, 2% NULL) with a 17-page partial index: 10 plans, 5 checks, 6 terms, 8 sources |
| Module 7 | `content/sql/pages/sql-obfuscated.toml` | conditions that hide a column: dates (`date_trunc`, intervals), arithmetic, `coalesce`, OR across two indexes, the optional-filter `(col = $1 OR $1 IS NULL)` that reads 14,960 pages as a generic plan, `QueryBuilder`: 10 plans, 4 checks, 4 terms, 7 sources |
| Module 8 | `content/sql/pages/sql-explain.toml` | reading EXPLAIN: a cost rebuilt from the cost settings (1,140 = 515 + 500 + 125), the tree read inside out, per-loop rows and times, work_mem spills, ANALYZE of a write inside a rolled-back transaction with its WAL, the measured cost of ANALYZE's timing (about 2.5x, `TIMING OFF` about 1.2x), the options table (`GENERIC_PLAN`, `MEMORY`), an sqlx plan helper: 4 plans (Postgres 18.6), 4 checks, 6 terms, 9 sources. Opens group `sql-plans` (added to `SD_GROUPS`) |
| Module 9 | `content/sql/pages/sql-access-filter.toml` | access vs filter conditions: the same lookup at 46 pages (status as a Filter) and 4 (status in the index), `Rows Removed by Filter` as the waste meter (2 million removed to return 1), `Index Searches` (3 for scattered IN values, 1 for adjacent), lossy bitmaps (`Rows Removed by Index Recheck: 297,696` at `work_mem = 64kB`, none at the default), a Rust helper that flags wasteful nodes: 5 plans (Postgres 18.6), 4 checks, 6 terms, 7 sources |

The only Postgres in the loop is a throwaway container: `docker run -d --name anneal-sqlperf -e POSTGRES_USER=anneal -e POSTGRES_HOST_AUTH_METHOD=trust -e POSTGRES_DB=sqlperf -p 5418:5432 postgres:18-alpine -c shared_buffers=256MB -c max_parallel_workers_per_gather=2`, then
`psql -h 127.0.0.1 -p 5418 -U anneal sqlperf -f tools/sqlperf/schema.sql` (about 75 s) and
`python3 tools/sqlperf/capture.py --db postgres://anneal@127.0.0.1:5418/sqlperf`. **Never run any of this against the dev or production
database.** Production runs Postgres 18, so capture on 18 (the pilot's plans were taken on 18.3).

## The quality bar (read module 1 first)

1. **Measured, never invented.** Every plan, size, page count and timing on a page comes from `capture.py` or from a query you ran and
   quote with its output (like the `pageinspect` numbers in module 1). Never type a plan by hand. Numbers that are version-specific
   say so ("Postgres 18 added skip scan").
2. **Researched.** Read the official docs for the feature (the Postgres docs for the version, the relevant source file or README when the
   docs are thin, the paper when there is one). Check each source URL opens (`curl -sL -o /dev/null -w '%{http_code}'`); some sites
   (O'Reilly, Medium, DuckDB's docs) block or render client-side: link them only if you can state the fact from another source.
   Facts you could not confirm are left out, not softened.
3. **Same sections, same pieces:** `summary` (5 lines), long-form `[[sections]]` with a section on **other engines** where it differs
   (MySQL InnoDB, SQLite, ClickHouse, DuckDB), a section **In real code** (Rust with sqlx, and SQL), `[[checks]]` that are open questions
   (predict the plan, write the index, explain why the planner skipped it: **never multiple choice**), `[[terms]]` tied to sections,
   `[[sources]]` with `kind`, figures where a picture beats prose.
4. **Markdown pieces** (all checked by `check_markdown` in `crates/content/src/sd.rs`): ```` ```sql ````/```` ```rust ```` fences with
   `title=`, `group=` (tabs) and `hl=`; ```` ```plan id=… setup=… ````; `:::steps` step-throughs; `:::deeper` folds; `^[margin note]`;
   `$$…$$` and `\( … \)` math; callouts `> [!NOTE|TIP|WARNING|INTERVIEW|ASIDE]`.
5. **ANSI SQL notes** where Postgres differs from the standard (`FETCH FIRST` vs `LIMIT`, `INCLUDE`, `IS DISTINCT FROM`, `FILTER`).
6. **Code you show must run.** Run every SQL and Rust snippet you print against the schema (or `cargo check` it) before you commit it,
   as was done for the consistent-hashing page.
7. **Credit and licences.** Text adapted from an open-licence source gets `credit` (and an entry in `content/sd/NOTICE.md`). Nothing from
   all-rights-reserved or no-derivatives sources (Use The Index, Luke; ByteByteGo; DDIA; Hello Interview).
8. **Owner's UI rules:** no scrollbars, underlines or separator lines (pills and spacing), 4-space indent, do not run prettier, mockup and
   approval before new screens or big UI, long jobs in the background, never touch the owner's data (see CLAUDE.md).

## The course map (47 modules, 10 parts)

File name = `content/sql/pages/<id>.toml`; `group` in brackets. The order is the reading order; numbers are the mockup's.

**I · Indexes** `[sql-indexes]`: 1 `sql-anatomy` **done** · 2 `sql-equality` **done** (equality, primary keys, verifying index use) · 3 `sql-multicolumn` **done** (concatenated keys, column order, Postgres 18 **skip scan**; the mockup's centrepiece, measured on the course schema: `(created_at, customer_id)` 3,116 pages vs `(customer_id, created_at)` 23, and 4 with a covering index; the mockup's 1,483 vs 4 were not from this data) · 4 `sql-functions` **done** (functions, case-insensitive search, expression indexes, bind parameters) · 5 `sql-ranges` **done** (greater, less, BETWEEN, LIKE and `text_pattern_ops`, collations, index combine/bitmap) · 6 `sql-partial-null` **done** (partial indexes, NULL in indexes, NOT NULL) · 7 `sql-obfuscated` **done** (conditions that hide a column: dates, numeric strings, redundant clauses, smart logic, maths).

**II · Reading plans** `[sql-plans]`: 8 `sql-explain` **done** (EXPLAIN, ANALYZE, BUFFERS, SETTINGS, FORMAT JSON; reading the tree) · 9 `sql-access-filter` **done** (access vs filter conditions, `Index Searches`, `Rows Removed`) · 10 `sql-statistics` (pg_stats, MCVs, histograms, `CREATE STATISTICS`, n_distinct) · 11 `sql-planner-wrong` (misestimates, correlated columns, parameterised plans, generic vs custom, `plan_cache_mode`, hints via settings) · 12 `sql-parallel` (parallel query, `JIT`, `work_mem`).

**III · Query shapes** `[sql-shapes]`: 13 `sql-joins` (nested loop, hash, merge; which index each wants) · 14 `sql-n-plus-one` (the ORM problem, `= ANY($1)`, joins) · 15 `sql-sort-group` (ORDER BY and GROUP BY from an index, ASC/DESC, NULLS FIRST/LAST, incremental sort) · 16 `sql-pagination` (top-N, OFFSET vs keyset, row-value comparison) · 17 `sql-subqueries` (EXISTS vs IN, CTEs and inlining, LATERAL) · 18 `sql-windows` (window functions, frames, pagination with them) · 19 `sql-aggregates` (count(*), count(DISTINCT), approximate counts, FILTER, ROLLUP).

**IV · Covering and clustering** `[sql-covering]`: 20 `sql-covering-indexes` (index-only scans, INCLUDE) · 21 `sql-visibility-map` (why heap fetches happen, VACUUM) · 22 `sql-clustering` (physical order, CLUSTER, BRIN, correlation).

**V · Postgres internals** `[sql-internals]`: 23 `sql-mvcc` (tuple versions, xmin/xmax, snapshots) · 24 `sql-vacuum` (VACUUM, bloat, HOT updates, fillfactor, autovacuum tuning) · 25 `sql-toast` (wide rows, TOAST, jsonb) · 26 `sql-wal` (WAL, checkpoints, `synchronous_commit`, what a write costs).

**VI · Writes and concurrency** `[sql-writes]`: 27 `sql-write-cost` (what each index costs INSERT/UPDATE/DELETE, measured) · 28 `sql-batching` (multi-row INSERT, `UNNEST`, `COPY`, pipeline mode) · 29 `sql-upsert` (`ON CONFLICT`, `MERGE`) · 30 `sql-locks` (row locks, `SKIP LOCKED` job queues, deadlocks, advisory locks) · 31 `sql-long-transactions` (idle in transaction, what they block, timeouts).

**VII · Special indexes** `[sql-special]`: 32 `sql-gin` (arrays and jsonb) · 33 `sql-fulltext` (tsvector, ranking, GIN vs GiST) · 34 `sql-trigrams` (`pg_trgm`, `LIKE '%x%'`) · 35 `sql-gist` (ranges, exclusion constraints, PostGIS) · 36 `sql-vectors` (pgvector: HNSW and IVFFlat, recall vs speed).

**VIII · At scale** `[sql-scale]`: 37 `sql-partitioning` (declarative partitioning, pruning, partition-wise joins) · 38 `sql-matviews` (materialized views, refresh) · 39 `sql-pooling` (PgBouncer, pool sizing, prepared statements through a pooler) · 40 `sql-replicas` (read replicas, lag, caches; link the System design pages).

**IX · From Rust, and in production** `[sql-rust]`: 41 `sql-sqlx` (binds, prepared statements and the statement cache, pools, transactions, `query_as`, batching, **plan assertions in tests**: fail when a plan gains a Sort node or a Seq Scan) · 42 `sql-monitoring` (`pg_stat_statements`, `auto_explain`, `pg_stat_activity` waits, `pg_locks`, slow query log, `pg_stat_user_indexes`).

**X · Other engines and analytics** `[sql-engines]` (added after the owner's note: "if analytical load needed use it"): 43 `sql-mysql` (InnoDB clustered indexes, the optimizer, `EXPLAIN ANALYZE`, differences from Postgres) · 44 `sql-sqlite` (rowid tables, covering indexes, `EXPLAIN QUERY PLAN`) · 45 `sql-columnar` (column stores: ClickHouse sparse indexes and granules, DuckDB zone maps, vectorised execution, compression) · 46 `sql-olap-vs-oltp` (when to leave Postgres: the same aggregate on Postgres vs DuckDB/ClickHouse over a big dataset, measured; materialised rollups; `pg_duckdb`/foreign data only if verified) · 47 `sql-engine-compare` (one query, four engines, four plans).

**Back matter:** `sql-myths` ("most selective column first", "indexes degenerate", "SELECT * is bad": each checked against real plans) · glossary · the example schema page. Part X and the big-data module need **large datasets**: use generated data at a higher `scale`, or public datasets (NYC taxi trips, GitHub Archive, IMDb dumps; check each licence and note it on the page); keep downloads out of git (a script fetches them).

## Still to build around the content

1. **Frame and route.** Today SQL pages appear as a group inside System design Learn (`SD_GROUPS` in `web/src/pages/sd/SdHome.tsx` has `sql-indexes`; add the other nine as their pages arrive) and open at `/sd/learn/sql-…`. The mockup's course map (parts, modules, progress) is a **new page** (`/sql`, mockup A): build it from the mockup, with its own header entry only after the owner approves where it lives. Keep ids stable: review cards are keyed by page id and check index.
2. **The plan explorer** (mockup D): a page where you paste any plan (`parsePlan` already guesses the format) and get `PlanView`; plus "open in explorer" from any plan on a page. Needs: the operation knowledge base grown to every common node in Postgres, MySQL and SQLite; hints that link to the module that explains a problem; a compare view for two plans (before/after an index); tests with captured plans from each engine (capture MySQL and SQLite plans with a script like `capture.py`; both run in containers or locally).
3. **Labs** (mockup C): stages the learner runs on their laptop, like BusTub: the CLI starts a throwaway Postgres in a container, loads the schema, and runs tests that assert **plans** (buffers read, no Sort node, an index-only scan) and results (same rows as the slow version, with a property test). A lab must never run on the server. Design the CLI part (`anneal sql ...`) with the owner before building; follow `docs/COURSE_STANDARDS.md` for stage rules (open checks, performance section, hints at BusTub's level, tests that explain themselves).
4. **CI.** Add `python3 tools/sqlperf/capture.py --check` against a Postgres 18 service container once there are enough plans (it needs the schema loaded first, about 75 s); `anneal validate` already fails a page whose plan is not captured. Re-capture plans when production's Postgres major version changes.
5. **Search and discoverability.** ⌘K already finds SQL pages as System design pages (they are in the same catalog). Add a "SQL performance" lens when `/sql` exists.

## Gotchas

- The shell's default Node (fnm) may fail with "Bad CPU type in executable"; use `export PATH=/opt/homebrew/bin:$PATH` before `pnpm` or `node`.
- `capture.py` needs `psql` on the path and the schema loaded; plans differ between runs only in timings and buffer counts, which is why `--check` compares **shapes**.
- If a plan you wanted does not appear (the planner chose a different one), do not edit the JSON: change the setup or the query until Postgres chooses it, or say on the page what it chose and why.
- Plans that sit near the planner's parallel-scan threshold flip between `Seq Scan` and `Gather` between runs and fail `--check`; give them a `session` setup (`no-parallel`). Recapturing a page's plans with `--only <id>` keeps the other plan files untouched (a full run rewrites every plan's timings).
- A Postgres 18 server without Docker: build `REL_18_STABLE` from the GitHub source (`./configure --prefix=... --without-icu --without-readline --without-zlib`, needs `flex` and `bison`), `initdb` as a non-root user. Plans captured on 18.6 this way matched the Docker-captured ones for modules 1 to 4 (29 of 29 shapes).
- Module ids are `sql-<topic>`; the loader rejects other prefixes in `content/sql/pages`.
- `psql` here is 14; the container is 18. Use the container's server for plans (any client works).
- Setups run through `psql`'s standard input, one statement at a time, so a setup may `VACUUM` (with `psql -c`, as before
  2026-11-01, several statements ran as one transaction and `VACUUM` failed).
- If Docker Hub answers 429 (rate limit), pull `mirror.gcr.io/library/postgres:18-alpine` (the same official image) and tag it
  `postgres:18-alpine`.
- When a sandbox blocks postgresql.org, the documentation's source is in the Postgres repository (`doc/src/sgml` on the
  `REL_18_STABLE` branch, readable through raw.githubusercontent.com): the same text, to research from.
