# Building blocks

Paths are relative to the crate's `src/`.

| # | Block | Module | Production crate / alternative | Pitfalls |
|---|---|---|---|---|
| 1 | Layered, typed config; redacted secrets | `config.rs` | `figment`, `config`; a secret manager | Silently ignored typos (`deny_unknown_fields`); env values coerced to the wrong type; secrets in `Debug`, logs or parse errors; shipping dev secrets to prod |
| 2 | State and dependency injection | `ports.rs`, `state.rs`, `testkit.rs` | Same pattern; `mockall` for mocks | Calling `now()` or an RNG directly makes tests flaky; fake repositories hide SQL behaviour |
| 3 | HTTP: versioned routes, extractors, problem+json, negotiation, limits, timeout, compression, CORS, headers | `http/mod.rs`, `http/extract.rs`, `http/middleware.rs`, `error.rs` | `axum`, `tower-http`; `problemdetails` | Layer order; framework rejections in a second error format; leaking SQL text; a body-buffering layer that bypasses the limit; CORS `*` |
| 4 | Accounts, sessions, API keys, RBAC, tenancy, throttling | `auth/` | `oauth2`/`openidconnect` or a hosted identity provider; `casbin`, Cedar, OpenFGA | Account enumeration by message or timing; hashing on async threads; rolling back a reuse revocation; throttle check-then-count races; a query without the tenant filter |
| 5 | Migrations, repositories, unit of work, optimistic lock, soft delete, audit, keyset paging, search | `db/`, `query.rs`, `../migrations/` | `sqlx`, `diesel`, `sea-orm`; Postgres | Deferred transactions failing mid-way with `SQLITE_BUSY`; audit outside the transaction; `OFFSET` paging; no tie-breaker; raw input in `ORDER BY` or `MATCH`; soft-deleted children still visible |
| 6 | Idempotency keys | `idempotency.rs` | Stripe's design as a model | Check-then-insert races; no fingerprint; keys shared across tenants; storing 5xx; storing secrets; cancellation leaving a key stuck |
| 7 | Rate limiting | `ratelimit.rs` | `governor`, Redis scripts, gateway | Per-process limits multiplied by replicas; unbounded key map; clock steps minting tokens |
| 8 | Conditional GET; singleflight cache with tags | `http/etag.rs`, `cache.rs` | `moka`, Redis, a CDN | Tenant missing from the key; authorizing only on a miss; stale load stored after invalidation; invalidating before commit |
| 9 | Durable queue, workers, outbox, scheduler, mail, webhooks | `jobs/` | `apalis`, `sqlxmq`, SQS, Kafka, Temporal | Assuming exactly-once; no lease fencing; poison jobs without a dead-letter queue; holding a DB connection during HTTP; SSRF and redirects |
| 10 | Streaming uploads, object store, sniffing, presigned URLs | `objstore.rs`, `http/handlers/files.rs` | `object_store`, `aws-sdk-s3`; `infer` for sniffing; antivirus scan | Path traversal; trusting `Content-Type`; shared or leftover temp files; serving uploads inline on your own origin; orphan objects |
| 11 | Server-sent events with resume | `http/handlers/events.rs` | `axum` SSE; Redis or NATS for fan-out; WebSocket for two-way | Lost wake-ups; no replay; revoked users still streaming; streams blocking shutdown |
| 12 | Tracing, JSON logs, metrics, health | `server.rs` (`init_tracing`), `http/middleware.rs`, `metrics.rs`, `http/handlers/ops.rs` | `tracing`, OpenTelemetry, `metrics` | Unbounded label cardinality; secrets or signed URLs in logs; liveness checks that depend on the database |
| 13 | Graceful shutdown | `server.rs`, `jobs/worker.rs` | `axum::serve`, `tokio-util` | No deadline; cancelling a job mid-way; endless streams; load balancer still routing |
| 14 | OpenAPI with contract tests | `http/openapi.rs` | `utoipa`, `aide` | A second route list that drifts; schemas nobody checks against the server |
| 15 | Test pyramid | `../tests/`, unit tests | `proptest`, `wiremock`, `testcontainers` | Sleeping instead of controlling time; concurrency tests that assert one interleaving; tests that cannot fail |
| 16 | CI script, container | `../scripts/ci.sh`, `../Dockerfile`, `../docker-compose.yml` | CI service, `cargo-deny`, distroless | Root user; secrets baked into layers; shell-form entrypoint swallowing SIGTERM |

## Standalone recipes

Smaller, self-contained programs on the same ideas, under `cookbook/examples/`:

| Topic | Recipe |
|---|---|
| Passwords, accounts, TOTP | [backend_password_accounts_totp_mfa](../../../examples/web/backend_password_accounts_totp_mfa.rs) |
| Authorization models | [backend_authz_rbac_abac_policies](../../../examples/web/backend_authz_rbac_abac_policies.rs) |
| Validation extractors | [backend_request_validation_extractors](../../../examples/web/backend_request_validation_extractors.rs) |
| Bearer tokens | [auth_basic_bearer_jwt](../../../examples/web/auth_basic_bearer_jwt.rs) |
| Middleware | [axum_middleware](../../../examples/web/axum_middleware.rs) |
| CORS and security headers | [cors_security_headers](../../../examples/web/cors_security_headers.rs) |
| Conditional requests | [http_caching_conditional](../../../examples/web/http_caching_conditional.rs) |
| Idempotency | [axum_idempotency](../../../examples/web/axum_idempotency.rs) |
| Outbox and inbox | [transactional_outbox_inbox_sqlite](../../../examples/microservices/transactional_outbox_inbox_sqlite.rs) |
| Webhook signatures | [webhooks_signatures](../../../examples/web/webhooks_signatures.rs) |
| Multipart uploads | [multipart_forms_uploads](../../../examples/web/multipart_forms_uploads.rs) |
| SSE replay | [axum_sse_replay](../../../examples/web/axum_sse_replay.rs) |
| Graceful shutdown | [graceful_shutdown](../../../examples/async/graceful_shutdown.rs) |
