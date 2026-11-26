# Production checklist

"Here" says what the crate does. Items marked **you** are not done.

## Configuration and secrets
- [ ] All settings typed and validated at startup. Here: `Config::validate`; prod refuses dev secrets, debug routes and a temporary database.
- [ ] Secrets from a secret manager, never in git or images. Here: redacted `Secret` type; compose requires them from the environment. **you**: the manager.
- [ ] Secret rotation. **you**: token, URL-signing and webhook keys are single; add a key id and accept the previous key during rotation.
- [ ] Review `Config::default()` limits (body, upload, rate, timeouts, argon2 cost) against your traffic and hardware.

## Migrations: expand / contract
`sqlx::migrate!` runs at startup here. With several replicas, run migrations as a separate deploy step.
Old and new code run at the same time during a rollout, so every change must suit both:
1. **Expand**: add nullable columns or new tables; old code ignores them.
2. Deploy code that writes both shapes and still reads the old one.
3. **Backfill** in small batches (short write locks), then verify.
4. Deploy code that reads the new shape.
5. **Contract**: in a later release, drop the old shape.
Never edit a migration that has been applied anywhere (sqlx checks checksums). `0001_init.sql` was still
edited during development of this crate because no database had been deployed.

## Observability
- [ ] Structured logs with request ids. Here: `init_tracing`, `observe`. **you**: ship and retain them.
- [ ] Metrics. Here: request rate, errors and duration by route; job counts by state. **you**: pool saturation, oldest unpublished outbox row, oldest queued job, SSE connection count.
- [ ] Distributed tracing. **you**: OpenTelemetry export and `traceparent` propagation into jobs and webhooks.
- [ ] Do not expose `/metrics`, `/readyz` or `/debug/config` publicly. Here they are unauthenticated.

## Example SLOs and alerts (starting points, not measurements of this code)
| Objective | Example target | Signal |
|---|---|---|
| Availability: non-5xx / all requests | 99.9 % over 30 days | `http_requests_total` |
| Read latency p99 | under 300 ms | duration histogram |
| Verification mail delay p95 | under 60 s | **you**: job age metric |
| First webhook attempt p95 | under 30 s | **you**: job age metric |

Alert on error-budget burn rate rather than single spikes. Page on any growth of `jobs{state="dead"}`.

## Backups and recovery
- [ ] Use SQLite's online backup API, `VACUUM INTO`, or Litestream. Copying a live WAL database file is not safe.
- [ ] Back up object storage too; database rows refer to objects.
- [ ] State RPO and RTO, and rehearse a restore. An untested backup is not a backup.
- [ ] Decide what restoring replays: unpublished outbox rows and queued jobs run again.

## Security review

| Check | Where it is handled |
|---|---|
| Tenant isolation on every query | `db/*` filters by `org_id`; `tests/api_auth.rs::another_tenant_cannot_touch_anything` |
| Authorization in one place | `auth/policy.rs`, matrix test |
| Password storage | Argon2id, per-password salt, `auth/password.rs` |
| Account enumeration | Login: same answer and a dummy hash. Registration still answers 409 (known trade-off, `auth/service.rs`) |
| Brute force | `auth/throttle.rs`. **you**: per-address limits, MFA, breached-password check |
| Anonymous abuse | **you**: nothing limits registration or other unauthenticated routes; do it at the edge |
| Session theft | Short access tokens, refresh rotation, reuse detection, immediate revocation |
| Credentials at rest | Refresh, email and API tokens hashed; delivered mail payloads scrubbed. Queued and dead mail jobs still hold live tokens, and webhook secrets are stored readable (**you**: encrypt) |
| Injection | Bound parameters; sort columns from an enum; FTS input quoted (`query.rs`) |
| CSRF | Tokens in `Authorization`, no cookies; JSON content type required |
| CORS | Explicit allow-list; `*` rejected at startup |
| Security headers | `http/middleware.rs`; HSTS in prod. **you**: TLS termination |
| Error leakage | `error.rs` hides internal causes; panics become a generic 500 |
| Resource limits | Body and upload limits, timeout, page limits. **you**: connection limits, header-read timeout, SSE connections per tenant, storage quotas |
| Uploads | Sniffed type allow-list, server-chosen keys, download as attachment. **you**: malware scanning. Sniffing reads only the first bytes: it classifies, it does not validate |
| SSRF through webhooks | Scheme and literal-address checks in prod, no redirects. **you**: DNS rebinding and NAT64 forms are not covered; use an egress proxy |
| Dependencies | **you**: `cargo audit` / `cargo deny` in CI |

## Known gaps

- **Idempotency is not atomic with the write.** A crash between the two leaves the key "in progress" for up to 24 h.
- **Delivery is at least once.** Mail and webhooks can repeat. Webhook receivers should dedupe on `x-delivery-id`.
- **No ordering guarantee** for webhook deliveries.
- **No retention** for `outbox`, `events`, `refresh_tokens`, `email_tokens`, dead jobs, soft-deleted rows or the audit log. Only finished jobs and idempotency keys are purged.
- **Orphan objects** are possible after a crash during upload, and objects of deleted tasks are never removed. An issued download URL keeps working until it expires, even if the task is deleted.
- **One process only.** Rate limiter, throttle, cache and SSE wake-ups are in memory; the cache keeps a counter per tag forever.
- **No backoff jitter**, and the worker has no lease heartbeat: a job must finish within 80 % of the lease.
- **Missing features:** password reset, resending verification (if that mail job dies, the account is stuck), member removal and role change, webhook update/delete, API key listing and expiry, dead-letter admin API, WebSocket.
- **Unpaginated:** audit log and webhook list. Comments are cut off at 500.
- A replayed idempotent response keeps status, body, content type and `ETag`, but not `Location`.
- A 405 response is problem+json but carries no `request_id` in the body.
- The container files have not been built or run.
