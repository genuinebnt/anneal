# cb-backend-reference

One coherent, readable backend: a multi-tenant "projects and tasks" SaaS API (organisations, users,
projects, tasks, comments, attachments). Every block a real web backend needs is here, in its own module,
wired together and tested. It is a **teaching reference**, not a product: read
[what it does not do](docs/PRODUCTION_CHECKLIST.md#known-gaps) before copying anything.

```sh
cd cookbook
cargo run  --release -p cb-backend-reference --bin backend-demo   # scripted end-to-end transcript
cargo test --release -p cb-backend-reference                      # the whole test pyramid
cargo run  --release -p cb-backend-reference --bin backend-demo -- serve   # real server on :8080
crates/cb-backend-reference/scripts/ci.sh                         # fmt, clippy, tests, demo
```

`serve` reads the TOML file named by `APP_CONFIG`, then `APP__SECTION__KEY` environment variables
(example: `APP__SERVER__BIND=127.0.0.1:9000`). Its mailer only logs recipient and subject, never the
body, so you cannot complete email verification against `serve` until you implement `ports::Mailer`.
The demo and tests use the in-memory mailer.

## Architecture

```text
 client ──HTTP──> axum router (/v1, built from the table in http/mod.rs)
                    │  observe: request id, span, metrics
                    │  security headers ─ CORS ─ compression ─ panic guard ─ timeout ─ body limit
                    │  per route: Accept check -> authenticate + rate limit -> idempotency
                    v
                 handlers ──> auth::policy (may this principal do this?)
                    │    └──> cache (stats)            objstore (attachments)
                    v
          ONE transaction:  domain row  +  audit_log row  +  outbox row
                    │
        ┌───────────┴──────────────  SQLite (WAL)  ─────────────────────┐
        │  jobs (lease, retry, DLQ)      outbox ──relay──> events       │
        └───────▲───────────▲───────────────│───────────────────│───────┘
                │           │               │ webhook jobs      │ wake-up
           scheduler     workers <──────────┘                   v
                         │    │                          SSE /v1/events
                      Mailer  webhook receiver (HMAC signed)
```

Rule of dependency: handlers know about repositories and policies; repositories know only SQL;
`ports` traits hide time, ids, mail and blobs. `App` (state.rs) is the single composition point.

## Life of a request: `PATCH /v1/tasks/{id}`

1. `observe` takes or creates `X-Request-Id`, opens a tracing span, and later records metrics under the
   route template.
2. The outer layers apply: security headers, CORS, compression, panic guard, timeout, body limit.
3. `require_json_accept` answers 406 before anything can be written.
4. `authenticate` resolves the bearer credential to a `Principal` (reading the user row, and for sessions
   checking the session is not revoked), then spends a rate-limit token.
5. The handler loads the task **scoped to the principal's org** (else 404), asks the policy whether this
   principal may modify it (else 403), requires `If-Match` (428) naming the loaded version (412), and
   validates every field (422, all errors at once).
6. In one transaction: `UPDATE ... WHERE version = ?`, refresh the search index, insert the audit row and
   the outbox event. Any failure rolls back all of it.
7. After the commit, the cache tag of the project is invalidated, and the new `ETag` is returned.
8. Later, the relay publishes the outbox row: SSE streams wake and one webhook job per hook is queued.

`PATCH` is not idempotency-wrapped: `If-Match` already makes a retry safe (it gets 412, then re-reads).

## Choices, and why

| Choice | Why |
|---|---|
| SQLite via `sqlx`, runtime queries | Real SQL, transactions and locking with no server. `query!` macros would need a database at compile time. |
| Repositories as functions on `&mut SqliteConnection` | The caller owns the transaction. In-memory SQLite is a better test double than a hand-written fake. |
| `chrono` (clock feature only) | Only "now in unix ms" is needed; all stored times are integers from an injected `Clock`. `time` would serve equally well. |
| HMAC tokens, not `jsonwebtoken` | One issuer and verifier, one algorithm, no header to trust. See `auth/token.rs`. |
| Hand-written validation | The rules are few, and every error must be collected into problem+json. `validator`/`garde` pay off with many types. |
| Hand-written metrics and OpenAPI | Shows the mechanism. Use `metrics`/OpenTelemetry and `utoipa`/`aide` for real work. |

## Reading order

1. `ports.rs`, `config.rs`, `error.rs`: the vocabulary.
2. `migrations/0001_init.sql`, then `db/mod.rs`, `db/projects.rs`, `db/tasks.rs`, `query.rs`.
3. `auth/policy.rs`, `auth/token.rs`, `auth/password.rs`, `auth/throttle.rs`, `auth/service.rs`.
4. `http/mod.rs` (the route table), `http/middleware.rs`, `http/extract.rs`, `http/etag.rs`.
5. `http/handlers/projects.rs` as the template, then the other handlers.
6. `idempotency.rs`, `ratelimit.rs`, `cache.rs`.
7. `jobs/`: `queue.rs`, `worker.rs`, `relay.rs`, `schedule.rs`, `webhook.rs`.
8. `objstore.rs`, `server.rs`, `metrics.rs`, `state.rs`.
9. `testkit.rs` and `tests/`. Start with `tests/faults.rs`: it states the guarantees most directly.

## Test pyramid

| Layer | Where |
|---|---|
| Pure logic (policy matrix, cursors, filters, headers, backoff, cron), with seeded random loops | unit tests in `src/` |
| Repositories on in-memory SQLite, including a keyset-vs-sort differential test | `src/db/`, `src/jobs/` |
| API through `tower::ServiceExt::oneshot` | `tests/api_auth.rs`, `tests/api_http.rs` |
| Fault injection: SQL triggers, locked database, failing mailer and receiver, timeout, cancellation | `tests/faults.rs` |
| Races on a multi-threaded runtime, asserting invariants only | `tests/concurrency.rs` |
| Real sockets with background tasks: SSE resume, graceful drain | `tests/loopback.rs` |

No assertion depends on how long something takes. Time-dependent behaviour uses `ManualClock`; the few
real timeouts in tests are hang guards, or force an operation that can never finish to give up.
A passing concurrency test does not prove the absence of races.

Block-by-block notes: [docs/BLOCKS.md](docs/BLOCKS.md). Going to production:
[docs/PRODUCTION_CHECKLIST.md](docs/PRODUCTION_CHECKLIST.md). Exercises: [docs/EXTEND.md](docs/EXTEND.md).
