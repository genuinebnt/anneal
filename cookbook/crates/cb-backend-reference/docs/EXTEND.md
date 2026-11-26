# Exercises

Each has an acceptance test to write **first**. Use `testkit::TestApp`. Copy `projects` as the template.

## Warm-up
1. **Webhook deletion.** Add `DELETE /v1/webhooks/{id}`. Done when: another tenant gets 404, members get
   403, the audit row is written atomically, and the OpenAPI contract tests still pass. What should
   happen to jobs already queued for that hook? (Read `jobs/worker.rs`.)
2. **Paginate the audit log** with the cursor from `query.rs`. Done when rows added during paging neither
   duplicate nor hide older ones.
3. **Jitter.** Add jitter to `backoff_ms` while keeping the tests deterministic (what must be injected?).

## Intermediate
4. **A new aggregate, end to end: labels.** Migration, repository, policy rules, handlers, routes, schemas,
   events, many-to-many with tasks, a `label=` filter. Done when `another_tenant_cannot_touch_anything`
   covers the new routes and `writes_audit_and_outbox_are_atomic` covers the new writes.
5. **Password reset.** Reuse `email_tokens` with a new purpose. Done when the answer does not reveal whether
   an account exists, the token is single use, and every session is revoked afterwards.
6. **Retention.** Schedule purges for events, outbox and expired tokens. What does an SSE client see when
   its `Last-Event-ID` is older than the oldest event? Design that answer.
7. **Another auth provider.** Add OpenID Connect login (authorization code with PKCE) behind an
   `IdentityProvider` trait with a fake issuer for tests. Decide how external identities link to users, and
   why linking by unverified email is an account takeover. Start from `examples/web/oauth2_pkce_flow.rs`.

## Advanced
8. **Atomic idempotency.** Store the response in the handler's own transaction. Prove it with a fault
   injected between the write and the response. What has to change in the middleware's design?
9. **Postgres.** Port the repositories. Replace the queue claim with `FOR UPDATE SKIP LOCKED` and FTS5 with
   `tsvector`. Which concurrency assumptions that SQLite's single writer gave you for free disappear?
   (Look at `auth::service::refresh` and `idempotency::claim` under READ COMMITTED.)
10. **Two replicas.** Move rate limiting to a shared store; broadcast cache invalidations and SSE wake-ups.
    Decide whether the limiter fails open or closed when the store is down, and test that decision.
11. **Multi-region read replicas.** Route reads to a replica and writes to the primary. Give clients
    read-your-writes (a position token returned by writes and checked on reads). Which reads must always
    go to the primary? (Hint: think about revocation.) Add a fake replica whose lag the test controls.
