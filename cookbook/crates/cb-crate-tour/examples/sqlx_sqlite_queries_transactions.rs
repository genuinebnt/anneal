//! RECIPE 1104 (crates): sqlx with SQLite: pools, migrations, typed queries, transactions, savepoints, upserts, pagination, bulk insert
//! LEVEL: intermediate
//! Use in: async Rust services that want to write SQL themselves (no ORM) against Postgres, MySQL or SQLite.   Run: cargo run --release -p cb-crate-tour --example sqlx_sqlite_queries_transactions
//! COMMON USES: web service persistence, job queues, local-first apps, integration tests against a throwaway database.
//! METHODS: sqlx::sqlite::SqlitePoolOptions, sqlx::sqlite::SqliteConnectOptions, sqlx::query, sqlx::query_as, sqlx::query_scalar, sqlx::FromRow, sqlx::Row::try_get, Query::bind, Query::execute, Query::fetch_one, Query::fetch_optional, Query::fetch_all, Query::fetch, sqlx::Pool::begin, sqlx::Transaction::commit, sqlx::Transaction::rollback, sqlx::Acquire::begin, sqlx::QueryBuilder::push_values, sqlx::QueryBuilder::push_bind, sqlx::raw_sql, sqlx::error::DatabaseError::kind, sqlx::error::DatabaseError::code, sqlx::types::Json, QueryBuilder::{separated, push_tuples}, #[derive(sqlx::Type)] (enum, transparent), #[sqlx(default, try_from, flatten, skip)], Pool::begin_with, SqlitePoolOptions::after_connect
//! DOCS: https://docs.rs/sqlx/0.8/sqlx/
//! CRATES: sqlx = "0.8" (features runtime-tokio, sqlite, chrono, uuid, json, migrate, macros: used here only for the derives), tokio = "1", futures-util = "0.3", chrono = "0.4", uuid = "1"
//!
//! COMPILE-TIME CHECKED QUERIES (described, not used here). `sqlx::query!("SELECT id FROM users WHERE name = ?", name)` connects
//! to the database named by DATABASE_URL while COMPILING, checks the SQL and infers an anonymous record type. For builds
//! without a database run `cargo sqlx prepare` (sqlx-cli), commit the `.sqlx/` directory and set SQLX_OFFLINE=true. This file
//! uses the RUNTIME functions, so a typo in SQL is a test failure here, not a compile error. That is the trade-off.
//! MIGRATIONS. `sqlx::migrate!("./migrations")` embeds a directory into the binary. This file hand-writes a 20-line runner
//! instead so that the whole example is one file and you can see what a migration tool does: a version table, ordered scripts,
//! each applied once inside a transaction. (The real one also stores checksums to detect edited migrations.)
//! POSTGRES DIFFERENCES (comments only, nothing here runs against Postgres): placeholders are `$1, $2` instead of `?`; richer
//! types (arrays, enums via `#[derive(sqlx::Type)]`, NUMERIC through the bigdecimal/rust_decimal features, TIMESTAMPTZ);
//! `PgListener` for LISTEN/NOTIFY; `PgConnection::copy_in_raw` for COPY; `SELECT ... FOR UPDATE SKIP LOCKED` for job queues;
//! a failed statement poisons the whole transaction until ROLLBACK (SQLite lets it continue), so use savepoints there.
//!
//! CHEAT SHEET
//!   exactly one row -> fetch_one (RowNotFound if none) ; maybe one -> fetch_optional ; all -> fetch_all ; huge -> fetch (stream)
//!   one column      -> query_scalar ; a struct -> query_as + #[derive(FromRow)]
//!   writes          -> execute -> rows_affected / last_insert_rowid ; or RETURNING + fetch_one
//!   atomicity       -> let mut tx = pool.begin(); ... .execute(&mut *tx); tx.commit()   (drop without commit = rollback)
//!   partial undo    -> tx.begin() creates a SAVEPOINT
//!   bulk insert     -> QueryBuilder::push_values in chunks below the bind-parameter limit
//!   user input      -> always bind; identifiers (column names) cannot be bound, so allow-list them
//!   id IN (...)     -> QueryBuilder::separated, one placeholder per value; handle the empty list yourself
//!   LIKE user text  -> escape % and _ and add ESCAPE
//!   SQLite read-then-write transaction -> pool.begin_with("BEGIN IMMEDIATE")
//!
//! Read: https://github.com/launchbadge/sqlx ; https://www.sqlite.org/lang_upsert.html ; https://www.sqlite.org/lang_returning.html ; https://use-the-index-luke.com/no-offset
//!
//! TRY NEXT:
//!   - [S] Remove `max_connections(1)` from `fresh_db` and predict which test fails and why (what is `:memory:` per connection?).
//!   - [S] Add a checksum column to the migration table and fail if an applied script changes.
//!   - [M] Turn `claim_job` into a lease with a deadline so a crashed worker's job becomes claimable again.
//!   - [M] Compare EXPLAIN QUERY PLAN for keyset and OFFSET pagination before and after dropping the primary key.
//!   - [A] PROJECT: with `reqwest_axum_http_roundtrip`, put this repository behind HTTP and map `RepoError` to status codes.
#![allow(dead_code)]
use chrono::{DateTime, TimeZone, Utc};
use futures_util::TryStreamExt;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::types::Json;
use sqlx::{Acquire, FromRow, QueryBuilder, Row, Sqlite, SqlitePool};
use std::time::Duration;
use uuid::Uuid;

const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE, name TEXT NOT NULL, score INTEGER NOT NULL DEFAULT 0 CHECK (score >= 0));
         CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, cents INTEGER NOT NULL);",
    ),
    (2, "CREATE TABLE events (id BLOB PRIMARY KEY, at TEXT NOT NULL, payload TEXT NOT NULL);"),
    (3, "CREATE TABLE jobs (id INTEGER PRIMARY KEY, state TEXT NOT NULL DEFAULT 'queued', worker TEXT);"),
];

/// Applies every migration newer than what the database has seen. Returns how many ran.
async fn migrate(pool: &SqlitePool, migrations: &[(i64, &'static str)]) -> sqlx::Result<usize> {
    sqlx::query("CREATE TABLE IF NOT EXISTS _migrations (version INTEGER PRIMARY KEY)")
        .execute(pool)
        .await?;
    let mut applied = 0;
    for &(version, script) in migrations {
        let mut tx = pool.begin().await?;
        let seen: Option<i64> =
            sqlx::query_scalar("SELECT version FROM _migrations WHERE version = ?")
                .bind(version)
                .fetch_optional(&mut *tx)
                .await?;
        if seen.is_none() {
            sqlx::raw_sql(script).execute(&mut *tx).await?; // raw_sql: several statements, no parameters, trusted text only
            sqlx::query("INSERT INTO _migrations (version) VALUES (?)")
                .bind(version)
                .execute(&mut *tx)
                .await?;
            applied += 1;
        }
        tx.commit().await?;
    }
    Ok(applied)
}

/// A private in-memory database. Every `:memory:` CONNECTION is its own database, so the pool must hold exactly one
/// connection and never recycle it.
async fn fresh_db() -> SqlitePool {
    let options: SqliteConnectOptions = "sqlite::memory:".parse().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options.foreign_keys(true))
        .await
        .unwrap();
    assert_eq!(migrate(&pool, MIGRATIONS).await.unwrap(), 3);
    pool
}

// ---------------- error mapping at the repository boundary ----------------
#[derive(Debug)]
enum RepoError {
    EmailTaken,
    NotFound,
    Invalid,
    Db(sqlx::Error),
}
impl From<sqlx::Error> for RepoError {
    fn from(e: sqlx::Error) -> Self {
        use sqlx::error::ErrorKind;
        match &e {
            sqlx::Error::RowNotFound => RepoError::NotFound,
            sqlx::Error::Database(d) => match d.kind() {
                ErrorKind::UniqueViolation => RepoError::EmailTaken, // fine here: email is the only non-rowid unique column
                ErrorKind::CheckViolation
                | ErrorKind::ForeignKeyViolation
                | ErrorKind::NotNullViolation => RepoError::Invalid,
                _ => RepoError::Db(e),
            },
            _ => RepoError::Db(e),
        }
    }
}

#[derive(Debug, Clone, PartialEq, FromRow)]
struct User {
    id: i64,
    email: String,
    #[sqlx(rename = "name")]
    display_name: String,
    score: i64,
}

async fn create_user(pool: &SqlitePool, email: &str, name: &str) -> Result<User, RepoError> {
    // RETURNING gives back the row (with generated id and defaults) in the same round trip.
    Ok(
        sqlx::query_as::<_, User>("INSERT INTO users (email, name) VALUES (?, ?) RETURNING *")
            .bind(email)
            .bind(name)
            .fetch_one(pool)
            .await?,
    )
}
async fn get_user(pool: &SqlitePool, id: i64) -> Result<User, RepoError> {
    Ok(sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?)
}

async fn crud_and_fetch_styles() {
    let db = fresh_db().await;
    assert_eq!(
        migrate(&db, MIGRATIONS).await.unwrap(),
        0,
        "migrations are idempotent"
    );
    let ann = create_user(&db, "ann@x", "Ann").await.unwrap();
    assert_eq!((ann.id, ann.score), (1, 0));
    assert!(matches!(
        create_user(&db, "ann@x", "Other").await,
        Err(RepoError::EmailTaken)
    ));
    assert!(matches!(get_user(&db, 99).await, Err(RepoError::NotFound)));
    let none: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(99)
        .fetch_optional(&db)
        .await
        .unwrap();
    assert!(none.is_none());

    let r = sqlx::query("UPDATE users SET score = score + ?1 WHERE email = ?2")
        .bind(5)
        .bind("ann@x")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(r.rows_affected(), 1);
    let r = sqlx::query("UPDATE users SET score = 1 WHERE email = ?")
        .bind("nobody")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        r.rows_affected(),
        0,
        "an UPDATE that matches nothing is NOT an error: check rows_affected"
    );
    let r = sqlx::query("UPDATE users SET score = -1")
        .execute(&db)
        .await
        .map_err(RepoError::from);
    assert!(
        matches!(r, Err(RepoError::Invalid)),
        "CHECK constraint: {r:?}"
    );
    let r = sqlx::query("INSERT INTO orders (user_id, cents) VALUES (999, 1)")
        .execute(&db)
        .await
        .map_err(RepoError::from);
    assert!(matches!(r, Err(RepoError::Invalid)), "foreign key: {r:?}");
    // The raw SQLite extended result code is also available.
    let e = sqlx::query("INSERT INTO users (email, name) VALUES ('ann@x', 'a')")
        .execute(&db)
        .await
        .unwrap_err();
    assert_eq!(
        e.as_database_error().and_then(|d| d.code()).as_deref(),
        Some("2067")
    ); // SQLITE_CONSTRAINT_UNIQUE

    // Untyped rows and decode errors.
    let row = sqlx::query("SELECT name, score FROM users WHERE id = 1")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        (
            row.try_get::<String, _>("name").unwrap(),
            row.try_get::<i64, _>(1).unwrap()
        ),
        ("Ann".into(), 5)
    );
    assert!(matches!(
        row.try_get::<i64, _>("name"),
        Err(sqlx::Error::ColumnDecode { .. })
    ));
    assert!(matches!(
        row.try_get::<i64, _>("nope"),
        Err(sqlx::Error::ColumnNotFound(_))
    ));
    // NULL must decode into Option<T>. SUM over zero rows is NULL.
    let total: Option<i64> = sqlx::query_scalar("SELECT SUM(cents) FROM orders")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(total, None);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

async fn injection_and_dynamic_sql() {
    let db = fresh_db().await;
    let evil = "x'); DROP TABLE users; --";
    create_user(&db, evil, "mallory").await.unwrap();
    assert_eq!(
        get_user(&db, 1).await.unwrap().email,
        evil,
        "a bound value is data, never SQL"
    );
    // Column names and directions cannot be bound. Map user input onto an ALLOW-LIST of your own literals.
    fn order_by(key: &str) -> Option<&'static str> {
        match key {
            "name" => Some("name ASC, id ASC"),
            "score" => Some("score DESC, id ASC"),
            _ => None,
        }
    }
    assert!(order_by("1; DROP TABLE users").is_none());
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM users WHERE 1=1");
    qb.push(" AND score >= ").push_bind(0);
    qb.push(" AND name = ").push_bind("mallory");
    qb.push(" ORDER BY ").push(order_by("score").unwrap());
    assert!(qb.sql().contains("score >= ? AND name = ?"));
    assert_eq!(
        qb.build_query_as::<User>()
            .fetch_all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
}

async fn bulk_insert_stream_paginate() {
    let db = fresh_db().await;
    // SQLite limits bind parameters per statement (32766 since 3.32), so chunk: rows_per_chunk * columns must stay below it.
    let people: Vec<(String, String, i64)> = (0..2500)
        .map(|i| (format!("u{i}@x"), format!("user{i:04}"), (i * 37) % 101))
        .collect();
    let mut tx = db.begin().await.unwrap(); // one transaction: far faster, and all-or-nothing
    for chunk in people.chunks(500) {
        let mut qb: QueryBuilder<Sqlite> =
            QueryBuilder::new("INSERT INTO users (email, name, score) ");
        qb.push_values(chunk, |mut b, (email, name, score)| {
            b.push_bind(email).push_bind(name).push_bind(score);
        });
        assert_eq!(
            qb.build().execute(&mut *tx).await.unwrap().rows_affected(),
            chunk.len() as u64
        );
    }
    tx.commit().await.unwrap();

    // Streaming: rows are decoded one at a time. NOTE: the stream holds its pool connection until dropped.
    let mut stream = sqlx::query_scalar::<_, i64>("SELECT score FROM users").fetch(&db);
    let (mut n, mut sum) = (0, 0);
    while let Some(score) = stream.try_next().await.unwrap() {
        n += 1;
        sum += score;
    }
    drop(stream);
    assert_eq!((n, sum), (2500, people.iter().map(|p| p.2).sum::<i64>()));

    // Keyset pagination: "everything after the last row I saw" in a TOTAL order (score, id).
    let (mut cursor, mut keyset) = ((-1i64, 0i64), Vec::new());
    loop {
        let page: Vec<User> = sqlx::query_as(
            "SELECT * FROM users WHERE (score, id) > (?, ?) ORDER BY score, id LIMIT 300",
        )
        .bind(cursor.0)
        .bind(cursor.1)
        .fetch_all(&db)
        .await
        .unwrap();
        let Some(last) = page.last() else { break };
        cursor = (last.score, last.id);
        keyset.extend(page.into_iter().map(|u| u.id));
    }
    let all: Vec<i64> = sqlx::query_scalar("SELECT id FROM users ORDER BY score, id")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(
        keyset, all,
        "keyset pages concatenate to the full ordered result"
    );
    // OFFSET drifts when rows are deleted between page requests: rows get skipped.
    let page = |off: i64| {
        sqlx::query_scalar::<_, i64>("SELECT id FROM users ORDER BY id LIMIT 10 OFFSET ?").bind(off)
    };
    let first = page(0).fetch_all(&db).await.unwrap();
    sqlx::query("DELETE FROM users WHERE id <= 5")
        .execute(&db)
        .await
        .unwrap();
    let second = page(10).fetch_all(&db).await.unwrap();
    assert_eq!(
        (first[9], second[0]),
        (10, 16),
        "ids 11..=15 were never shown"
    );
}

async fn upsert() {
    let db = fresh_db().await;
    const UPSERT: &str = "INSERT INTO users (email, name, score) VALUES (?, ?, ?)
        ON CONFLICT(email) DO UPDATE SET score = users.score + excluded.score, name = excluded.name
        RETURNING id, score";
    let first: (i64, i64) = sqlx::query_as(UPSERT)
        .bind("a@x")
        .bind("A")
        .bind(3)
        .fetch_one(&db)
        .await
        .unwrap();
    let again: (i64, i64) = sqlx::query_as(UPSERT)
        .bind("a@x")
        .bind("A2")
        .bind(4)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!((first, again), ((1, 3), (1, 7)));
    let r =
        sqlx::query("INSERT INTO users (email, name) VALUES ('a@x', 'z') ON CONFLICT DO NOTHING")
            .execute(&db)
            .await
            .unwrap();
    assert_eq!(r.rows_affected(), 0);
}

async fn user_count(db: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await
        .unwrap()
}
const ADD: &str = "INSERT INTO users (email, name) VALUES (?, 'n')";

async fn add_two_atomically(db: &SqlitePool, a: &str, b: &str) -> Result<(), RepoError> {
    let mut tx = db.begin().await?;
    sqlx::query(ADD).bind(a).execute(&mut *tx).await?;
    sqlx::query(ADD).bind(b).execute(&mut *tx).await?; // `?` drops tx on error: rolled back
    tx.commit().await?;
    Ok(())
}

async fn transactions_and_savepoints() {
    let db = fresh_db().await;
    assert!(add_two_atomically(&db, "a", "a").await.is_err());
    assert_eq!(
        user_count(&db).await,
        0,
        "the first insert was rolled back with the second"
    );
    add_two_atomically(&db, "a", "b").await.unwrap();
    assert_eq!(user_count(&db).await, 2);

    let mut tx = db.begin().await.unwrap();
    sqlx::query(ADD).bind("c").execute(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(user_count(&db).await, 2);

    // Nested begin = SAVEPOINT: undo part of a transaction and keep the rest.
    let mut outer = db.begin().await.unwrap();
    sqlx::query(ADD)
        .bind("keep")
        .execute(&mut *outer)
        .await
        .unwrap();
    {
        let mut inner = outer.begin().await.unwrap();
        sqlx::query(ADD)
            .bind("discard")
            .execute(&mut *inner)
            .await
            .unwrap();
        // dropped without commit: ROLLBACK TO SAVEPOINT
    }
    let mut inner = outer.begin().await.unwrap();
    sqlx::query(ADD)
        .bind("keep2")
        .execute(&mut *inner)
        .await
        .unwrap();
    inner.commit().await.unwrap(); // RELEASE SAVEPOINT; durable only if the outer one commits
    outer.commit().await.unwrap();
    let emails: Vec<String> = sqlx::query_scalar("SELECT email FROM users ORDER BY id")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(emails, ["a", "b", "keep", "keep2"]);

    // PITFALL: using the POOL while holding a transaction needs a second connection. With one connection this deadlocks
    // until acquire_timeout. Here: a short timeout proves it.
    let held = db.begin().await.unwrap();
    let blocked = tokio::time::timeout(Duration::from_millis(100), db.acquire()).await;
    assert!(
        blocked.is_err(),
        "the pool is exhausted while the transaction is alive"
    );
    drop(held);
    assert!(db.acquire().await.is_ok());
}

/// An atomic claim: the UPDATE selects and marks in ONE statement, so two workers can never get the same job.
async fn claim_job(db: &SqlitePool, worker: &str) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "UPDATE jobs SET state = 'running', worker = ?
         WHERE id = (SELECT id FROM jobs WHERE state = 'queued' ORDER BY id LIMIT 1) RETURNING id",
    )
    .bind(worker)
    .fetch_optional(db)
    .await
}

async fn concurrency_with_a_file_database() {
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("q.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal) // readers do not block the single writer
        .busy_timeout(Duration::from_secs(10)); // writers queue instead of failing with SQLITE_BUSY
    let db = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .unwrap();
    migrate(&db, MIGRATIONS).await.unwrap();
    for _ in 0..200 {
        sqlx::query("INSERT INTO jobs DEFAULT VALUES")
            .execute(&db)
            .await
            .unwrap();
    }
    let mut workers = tokio::task::JoinSet::new();
    for w in 0..8 {
        let db = db.clone(); // a pool is an Arc inside: cloning is cheap
        workers.spawn(async move {
            let mut mine = vec![];
            while let Some(id) = claim_job(&db, &format!("w{w}")).await.unwrap() {
                mine.push(id);
            }
            mine
        });
    }
    let mut claimed: Vec<i64> = workers.join_all().await.into_iter().flatten().collect();
    claimed.sort_unstable();
    assert_eq!(
        claimed,
        (1..=200).collect::<Vec<_>>(),
        "every job claimed exactly once"
    );
    db.close().await;
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Payload {
    kind: String,
    n: u32,
}

async fn rich_types() {
    let db = fresh_db().await;
    let id = Uuid::from_u128(0x1234_5678_9abc_def0);
    let at: DateTime<Utc> = Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap();
    sqlx::query("INSERT INTO events (id, at, payload) VALUES (?, ?, ?)")
        .bind(id)
        .bind(at)
        .bind(Json(Payload {
            kind: "k".into(),
            n: 7,
        }))
        .execute(&db)
        .await
        .unwrap();
    let (got_id, got_at, Json(p)): (Uuid, DateTime<Utc>, Json<Payload>) =
        sqlx::query_as("SELECT id, at, payload FROM events")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(
        (got_id, got_at, p),
        (
            id,
            at,
            Payload {
                kind: "k".into(),
                n: 7
            }
        )
    );
    // What is actually stored: SQLite has few storage classes.
    let kinds: (String, String, String) =
        sqlx::query_as("SELECT typeof(id), typeof(at), typeof(payload) FROM events")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(kinds, ("blob".into(), "text".into(), "text".into()));
    let n: i64 = sqlx::query_scalar("SELECT json_extract(payload, '$.n') FROM events")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(n, 7);
    // Statements are prepared once per connection and cached by SQL text, which is one more reason to BIND values rather
    // than format them into the string (every distinct string is a new cache entry). `.persistent(false)` opts out.
    for i in 0..3 {
        let v: i64 = sqlx::query_scalar("SELECT ? * 2")
            .bind(i)
            .persistent(i % 2 == 0)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(v, i * 2);
    }
}

/// A closed set stored as text. Decoding an unknown string is an error, not a silent default.
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
enum JobState {
    Queued,
    Running,
    DeadLetter,
}
/// A newtype that binds and decodes exactly like its inner integer.
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(transparent)]
struct JobId(i64);

#[derive(Debug, PartialEq, FromRow)]
struct Owner {
    worker: Option<String>,
}
#[derive(Debug, PartialEq, FromRow)]
struct Job {
    id: JobId,
    state: JobState,
    #[sqlx(flatten)]
    owner: Owner, // columns of this row, grouped in the Rust type
    #[sqlx(default)]
    priority: i64, // Default::default() when the SELECT has no such column
    #[sqlx(try_from = "i64")]
    id_again: u8, // decode as i64, then convert; out of range is a decode error
    #[sqlx(skip)]
    cached: Vec<String>, // never read from the database
}

async fn derived_types_and_row_mapping() {
    let db = fresh_db().await;
    for state in [JobState::Queued, JobState::Running, JobState::DeadLetter] {
        sqlx::query("INSERT INTO jobs (state) VALUES (?)")
            .bind(state)
            .execute(&db)
            .await
            .unwrap();
    }
    let stored: Vec<String> = sqlx::query_scalar("SELECT state FROM jobs ORDER BY id")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(stored, ["queued", "running", "dead_letter"]);
    const SELECT: &str = "SELECT id, state, worker, id AS id_again FROM jobs WHERE id = ?";
    let job: Job = sqlx::query_as(SELECT)
        .bind(JobId(3))
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        job,
        Job {
            id: JobId(3),
            state: JobState::DeadLetter,
            owner: Owner { worker: None },
            priority: 0,
            id_again: 3,
            cached: vec![]
        }
    );
    // A value written by another program (or a newer version of this one).
    sqlx::query("UPDATE jobs SET state = 'paused' WHERE id = 1")
        .execute(&db)
        .await
        .unwrap();
    let e = sqlx::query_as::<_, Job>(SELECT)
        .bind(1)
        .fetch_one(&db)
        .await
        .unwrap_err();
    assert!(
        matches!(e, sqlx::Error::ColumnDecode { ref index, .. } if index.contains("state")),
        "{e:?}"
    );
    sqlx::query("UPDATE jobs SET id = 300 WHERE id = 2")
        .execute(&db)
        .await
        .unwrap();
    let e = sqlx::query_as::<_, Job>(SELECT)
        .bind(300)
        .fetch_one(&db)
        .await
        .unwrap_err();
    assert!(
        matches!(e, sqlx::Error::ColumnDecode { .. }),
        "300 does not fit u8: {e:?}"
    );

    // Narrow integer decodes are checked, never truncated.
    let e = sqlx::query_scalar::<_, i32>("SELECT 1 << 40")
        .fetch_one(&db)
        .await
        .unwrap_err();
    assert!(matches!(e, sqlx::Error::ColumnDecode { .. }), "{e:?}");
    // PITFALL: fetch_one does not mean "exactly one". Extra rows are ignored without an error.
    let first: i64 = sqlx::query_scalar("SELECT id FROM jobs ORDER BY id")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(first, 1);
    // To enforce uniqueness, ask for two and check.
    let two: Vec<i64> = sqlx::query_scalar("SELECT id FROM jobs ORDER BY id LIMIT 2")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(two.len(), 2);
}

/// `IN (?)` cannot take a list: each value needs its own placeholder. `IN ()` is valid in SQLite but not in most
/// databases, so return early for an empty list.
async fn users_by_ids(db: &SqlitePool, ids: &[i64]) -> sqlx::Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT email FROM users WHERE id IN (");
    let mut list = qb.separated(", ");
    for id in ids {
        list.push_bind(*id);
    }
    list.push_unseparated(") ORDER BY id");
    qb.build_query_scalar().fetch_all(db).await
}
fn like_escape(user: &str) -> String {
    user.replace('\\', r"\\")
        .replace('%', r"\%")
        .replace('_', r"\_")
}

async fn dynamic_lists_and_like() {
    let db = fresh_db().await;
    for email in ["a_b@x", "axb@x", "100%@x", "1000@x", r"back\slash@x"] {
        create_user(&db, email, "n").await.unwrap();
    }
    assert_eq!(
        users_by_ids(&db, &[3, 1, 99]).await.unwrap(),
        ["a_b@x", "100%@x"]
    );
    assert!(users_by_ids(&db, &[]).await.unwrap().is_empty());
    // One bound string for the whole list matches nothing, without an error.
    let wrong: Vec<String> = sqlx::query_scalar("SELECT email FROM users WHERE id IN (?)")
        .bind("1,3")
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(wrong.is_empty());
    // Composite keys: row values.
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT id FROM users WHERE (id, email) IN ");
    qb.push_tuples([(1i64, "a_b@x"), (2, "wrong")], |mut b, (id, email)| {
        b.push_bind(id).push_bind(email);
    });
    assert!(qb.sql().contains("((?, ?), (?, ?))"), "{}", qb.sql()); // push_tuples adds the outer parentheses
    assert_eq!(
        qb.build_query_scalar::<i64>().fetch_all(&db).await.unwrap(),
        [1]
    );

    // Binding stops injection but NOT wildcards: % and _ keep their LIKE meaning inside a bound value.
    let search = async |pattern: String| -> Vec<String> {
        sqlx::query_scalar(r"SELECT email FROM users WHERE email LIKE ? ESCAPE '\' ORDER BY id")
            .bind(pattern)
            .fetch_all(&db)
            .await
            .unwrap()
    };
    assert_eq!(search("a_b%".into()).await, ["a_b@x", "axb@x"]);
    assert_eq!(search(format!("{}%", like_escape("a_b"))).await, ["a_b@x"]);
    assert_eq!(
        search(format!("{}%", like_escape("100%"))).await,
        ["100%@x"]
    );
    assert_eq!(
        search(format!("{}%", like_escape(r"back\"))).await,
        [r"back\slash@x"]
    );
}

/// SQLite's BEGIN is DEFERRED: it takes no write lock until the first write. In WAL mode a transaction that has READ and
/// then tries to write after someone else committed gets SQLITE_BUSY_SNAPSHOT at once: waiting cannot help, because its
/// snapshot is stale, so busy_timeout does not apply.
async fn deferred_transaction_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("upgrade.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));
    // after_connect runs once for each new connection: the place for per-connection settings.
    let opened = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = opened.clone();
    let db = SqlitePoolOptions::new()
        .max_connections(2)
        .after_connect(move |conn, _| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move {
                sqlx::query("PRAGMA temp_store = MEMORY")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE counter (n INTEGER); INSERT INTO counter VALUES (0)")
        .execute(&db)
        .await
        .unwrap();
    const READ: &str = "SELECT n FROM counter";
    const BUMP: &str = "UPDATE counter SET n = n + 1";

    let mut reader_then_writer = db.begin().await.unwrap();
    let _: i64 = sqlx::query_scalar(READ)
        .fetch_one(&mut *reader_then_writer)
        .await
        .unwrap(); // snapshot taken here
    sqlx::query(BUMP).execute(&db).await.unwrap(); // another connection commits
    let e = sqlx::query(BUMP)
        .execute(&mut *reader_then_writer)
        .await
        .unwrap_err();
    assert_eq!(
        e.as_database_error().and_then(|d| d.code()).as_deref(),
        Some("517"),
        "SQLITE_BUSY_SNAPSHOT: {e}"
    );
    reader_then_writer.rollback().await.unwrap(); // the only way forward: retry the WHOLE transaction

    // FIX: take the write lock up front. Other writers now wait (up to busy_timeout) until we are done.
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let before: i64 = sqlx::query_scalar(READ).fetch_one(&mut *tx).await.unwrap();
    let other = {
        let db = db.clone();
        tokio::spawn(async move {
            sqlx::query(BUMP)
                .execute(&db)
                .await
                .map(|r| r.rows_affected())
        })
    };
    sqlx::query(BUMP).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        other.await.unwrap().unwrap(),
        1,
        "the other writer succeeds instead of failing"
    );
    let after: i64 = sqlx::query_scalar(READ).fetch_one(&db).await.unwrap();
    assert_eq!((before, after), (1, 3), "no lost update");
    assert!((1..=2).contains(&opened.load(std::sync::atomic::Ordering::SeqCst)));
    db.close().await;
}

async fn run() {
    crud_and_fetch_styles().await;
    injection_and_dynamic_sql().await;
    bulk_insert_stream_paginate().await;
    upsert().await;
    transactions_and_savepoints().await;
    concurrency_with_a_file_database().await;
    rich_types().await;
    derived_types_and_row_mapping().await;
    dynamic_lists_and_like().await;
    deferred_transaction_upgrade().await;
}

pub fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run());
    println!("sqlx/sqlite: all sections passed");
}
