# 07. Networking and web services

Prerequisite tracks: `paths/tracks/09_networking_web.md`, `03_concurrency_async.md`; building guide `paths/building/rest_service.md`. Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

Test strategy: run the real server on port 0 (OS-assigned) inside the test, drive it with a client, and check **headers and status codes, not just bodies**. For load: report p50/p99 with a fixed request count and warm-up, never assert on absolute speed.

---

## 7.1 `httpd`: an HTTP/1.1 parser and static file server  `S`

**Goal.** Parse HTTP/1.1 requests from a TCP stream (request line, headers, `Content-Length` and chunked bodies), serve files from a directory with correct `Content-Type`, `ETag`, range requests and keep-alive, with path-traversal protection.
**Learn.** Incremental parsing from partial reads, header limits, framing rules, `304` and `206` semantics, security of path joins.
**Read before.** `networking/http1_parser`, `networking/tcp_framed_server`, `web/http_caching_conditional`. **During.** `capstones/mini_http_server_thread_pool`, `os_and_storage/zero_copy_sendfile_splice`.
**Milestones.**
1. Parse a complete request in one buffer; 20 table-driven valid and 20 invalid cases (400).
2. Handle requests split across arbitrary read boundaries (feed 1 byte at a time in a test).
3. Static serving: `GET`, `HEAD`, 404, 405 with `Allow`, directory index.
4. `If-None-Match` -> 304; `Range: bytes=0-99` -> 206 with `Content-Range`.
5. Limits: header block max 16 KiB, max 100 headers, body max 1 MiB; slow-loris timeout 10 s.
**Acceptance.** `GET /../../etc/passwd`, `%2e%2e/`, and backslash variants return 403/404; 1-byte-at-a-time test passes; `curl`, `wrk -t2 -c64 -d5s` run without errors (report req/s); a 4 GiB sparse file streams with constant memory.
**Data.** A 50-file fixture dir; a list of 15 malicious paths; `h2spec`-like malformed request corpus you write.
**Write-up.** How do you detect the end of headers without allocation per byte? Why must `Content-Length` and `Transfer-Encoding` together be rejected?
**Stretch.** `Connection: keep-alive` pipelining; gzip with `Accept-Encoding`.
**Resembles.** [hyper](https://github.com/hyperium/hyper), nginx static serving.

## 7.2 `shortly`: a URL shortener with sharded storage  `M`

**Goal.** `POST /links` -> short code; `GET /:code` -> 301/302; counters; custom aliases; expiry. Storage sharded across N backends (files or embedded DBs) by a consistent-hash router, with codes generated collision-free.
**Learn.** Base62 and ID generation (counter + Feistel/bijection vs random + retry), sharding by code, hot keys and caching, redirect semantics (301 vs 302 caching), analytics counting without write amplification.
**Read before.** `web/rest_api_patterns`, `distributed/consistent_hashing`, `web/axum_router_state`. **During.** `data_structures/lru_cache`, `web/http_caching_conditional`, `search_vectors/bloom_filter`, `web/axum_idempotency`.
**Milestones.**
1. Single-node: 7-char base62 codes from a counter run through a bijective scrambler (so codes are not enumerable).
2. Validate URLs (scheme allowlist, length <= 2,048, no private IP hosts: SSRF guard).
3. Shard by `hash(code)` across 4 backends; routing table versioned.
4. LRU cache for hot codes (capacity 10,000) plus negative caching with Bloom filter.
5. Click counting: in-memory sharded counters flushed every 1 s in batches.
6. Add a shard (4 -> 5) while serving; only about 1/5 of codes move; reads fall back to old owner during migration.
**Acceptance.** 10M generated codes: zero collisions (assert via bitset); load test 20k req/s with Zipf(1.0) codes shows cache hit rate over 80%; resharding test loses no link (checksum of all (code, url) before/after); SSRF cases (`http://127.0.0.1`, `http://[::1]`, `http://169.254.169.254`, decimal-IP forms) are rejected.
**Data.** Generator of 1M URLs; Zipfian click traffic; SSRF corpus of 20 URLs.
**Write-up.** How many codes does 7 chars of base62 give (62^7 ~ 3.5e12) and when do you run out? What is the cost of a 301 for analytics?
**Stretch.** Geo-distributed read replicas; abuse detection with the 7.4 limiter.
**Resembles.** bit.ly-style services; [Vitess](https://github.com/vitessio/vitess) for the sharding story.

## 7.3 `jobapi`: a REST service with idempotency, pagination and rate limiting  `M`

**Goal.** A production-style REST API for creating and querying jobs: `Idempotency-Key` on POST, cursor pagination, ETag/If-Match optimistic concurrency, structured errors (RFC 7807 problem+json), request IDs, auth with bearer tokens, graceful shutdown, and OpenAPI-ish docs.
**Learn.** Idempotency semantics (same key + same body -> same response; same key + different body -> 422), cursor vs offset pagination, `tower` middleware stacks, error mapping, tracing spans per request.
**Read before.** `capstones/job_service_axum`, `web/axum_idempotency`, `web/axum_errors`, `web/rest_api_patterns`. **During.** `web/axum_middleware`, `web/auth_basic_bearer_jwt`, `async/graceful_shutdown`, `dev_tools/tracing_logs`, `patterns/tower_service_layer`.
**Milestones.**
1. CRUD with SQLite/`rusqlite` or in-memory store behind a trait; typed errors -> status codes.
2. Idempotency middleware: store `(key, request hash) -> stored response` for 24 h; concurrent duplicates wait on the first.
3. Cursor pagination with opaque base64 cursors (keyset on `(created_at, id)`), stable under inserts.
4. `ETag` on GET and `If-Match` on PUT -> 412 on mismatch.
5. Per-token rate limit (7.4 limiter) with `429` + `Retry-After` + `RateLimit-*` headers.
6. SIGTERM: stop accepting, finish in-flight (max 10 s), exit 0.
**Acceptance.** 50 concurrent POSTs with the same key create exactly one job and return identical bodies; paginating 10,000 jobs while inserting 1,000 more yields no duplicates and no skips among the original 10,000; every error response parses as problem+json with a request id; a request in flight at SIGTERM completes.
**Data.** Scripted client (Rust test or `hurl`) with 30 scenarios; generator for 10k jobs.
**Write-up.** What do you return if the first request is still running when the duplicate arrives? Why keyset instead of `OFFSET`?
**Stretch.** Outbox pattern publishing job-created events; OpenTelemetry traces.
**Resembles.** [axum](https://github.com/tokio-rs/axum) for the framework; Stripe's public API docs on idempotent requests for the semantics.

## 7.4 `limitd`: a rate-limiter service with five algorithms  `M`

**Goal.** Implement fixed window, sliding window log, sliding window counter, token bucket and GCRA behind one trait, expose `POST /check {key, cost}` -> `{allowed, remaining, retry_after}`, and compare them on fairness, burst behaviour and memory.
**Learn.** Algorithm trade-offs, boundary burst problem of fixed windows, atomicity (CAS loops or per-key locks), clock injection, per-tenant limits and quotas, distributed approximation.
**Read before.** `capstones/rate_limiter_lab`, `distributed/token_bucket`. **During.** `concurrency/sharded_map`, `concurrency/atomics_ordering`, `perf/bench_harness`.
**Milestones.**
1. `trait Limiter { fn check(&self, key, cost, now) -> Decision }` with a fake clock.
2. All five algorithms; each passes the same conformance suite (limit 100/min, never admits more than allowed by its own definition).
3. Demonstrate the fixed-window 2x boundary burst (200 requests in 2 seconds around the boundary) and that the other four bound it.
4. Concurrency: 16 threads hitting 1,000 keys, no over-admission for GCRA/token bucket (atomic update per key).
5. Eviction of idle keys (TTL 10 min) keeps memory bounded for 10M distinct keys.
6. HTTP server with `RateLimit-Limit/Remaining/Reset` headers.
**Acceptance.** Over a simulated hour with bursty and steady clients, admitted <= `limit * windows + burst` for each algorithm with an exact stated bound; memory per key reported (bytes) for each; 1M checks/s single-node in-process; token bucket and GCRA agree exactly on decisions for the same parameters.
**Data.** Traffic generators: steady, 10x burst every 30 s, Poisson, and one abusive key at 100x.
**Write-up.** Which would you choose for an API gateway and which for a login endpoint? What does distributed limiting cost in accuracy?
**Stretch.** Redis-script version with a local-plus-global hybrid; AIMD adaptive concurrency limiter.
**Resembles.** [Envoy ratelimit](https://github.com/envoyproxy/ratelimit), [Pingora](https://github.com/cloudflare/pingora).

## 7.5 `flagd`: a feature-flag service with streaming updates  `M`

**Goal.** Flags with boolean/string/JSON variants, targeting rules (attribute equals/in/semver/percentage rollout), consistent bucketing by user id, an SSE stream pushing changes, and an SDK crate with a local cache that works if the server is down.
**Learn.** Rule-engine design, deterministic hashing for rollouts (same user, same bucket across restarts), SSE with replay (`Last-Event-ID`), audit log, stale-while-revalidate client design.
**Read before.** `web/axum_sse_replay`, `data_structures/hash_functions`, `web/http_caching_conditional`. **During.** `text_and_parsing/expression_parsing_three_ways`, `patterns/state_machine`, `web/http_client_resilience`, `encoding_compression/serde_custom`.
**Milestones.**
1. Flag model + evaluator: first matching rule wins, default variant otherwise; evaluation returns the reason.
2. Percentage rollout: `bucket = hash(flag_key + user_id) % 10000`; 20% rollout puts 20% +/- 1% of 100k users in.
3. Monotone rollout: raising 20% -> 30% never flips a user already in.
4. SSE endpoint with event ids and replay from `Last-Event-ID` after disconnect.
5. SDK: local cache, background refresh with backoff, evaluates offline from last snapshot.
6. Audit log of every change (who, before, after) as append-only NDJSON.
**Acceptance.** Property test: 100k users across 10 rollout percentages are monotone and balanced; SDK keeps serving after server kill; reconnect after 50 missed events replays exactly those 50 in order; evaluation throughput over 1M/s per core.
**Data.** 100k synthetic users with attributes (country, plan, app version); 30 flags with 1-5 rules.
**Write-up.** How do you avoid flapping users when a rule's attribute is missing? What is the consistency model of the SDK?
**Stretch.** Experiment assignment with sticky variants and exposure events.
**Resembles.** [Unleash](https://github.com/Unleash/unleash), [Flagsmith](https://github.com/flagsmith/flagsmith).

## 7.6 `notifyd`: a notification system with retries and dedupe  `M`

**Goal.** Accept notification requests (email/SMS/webhook mocks) through an API, queue them durably, render templates, deliver with retries and per-channel rate limits, dedupe by idempotency key, honor user preferences and quiet hours, and expose delivery status.
**Learn.** Outbox/queue durability, at-least-once + dedupe = effectively once, dead-letter queues, templating safety (escaping), webhook signing, priority queues and fairness across tenants.
**Read before.** `web/webhooks_signatures`, `distributed/retry_backoff_jitter`, `distributed/saga_orchestrator`. **During.** `os_and_storage/wal_append_log`, `capstones/job_service_axum`, `distributed/token_bucket`, `patterns/state_machine`.
**Milestones.**
1. Durable queue (file WAL or SQLite) with states `queued/sending/sent/failed/dead`.
2. Workers with visibility timeout 30 s: a crashed worker's message reappears.
3. Template engine with HTML escaping; missing variables are an error, not an empty string.
4. Webhook delivery signed with HMAC-SHA256, timestamp tolerance 5 min, replay protection.
5. Per-tenant fair scheduling (round-robin across tenants) + per-provider rate limit.
6. Preferences and quiet hours (user timezone) defer, not drop.
**Acceptance.** Chaos test: provider mock fails 30% and times out 10%; 10,000 notifications -> each reaches `sent` exactly once from the provider's view when dedupe keys are used (provider mock counts duplicates), or dead after 6 attempts; a heavy tenant (9,000 msgs) does not delay a light tenant's 10 msgs beyond 2 s.
**Data.** Tenant/user generator with timezones; provider mock with seeded failure profile.
**Write-up.** Where can a duplicate still slip through? What do you show users for "sent" when the provider's ack is lost?
**Stretch.** Digest batching ("5 new comments"); web push with VAPID.
**Resembles.** [Novu](https://github.com/novuhq/novu), Stripe webhooks.

## 7.7 `balancer`: a reverse proxy and load balancer  `M`

**Goal.** An L7 reverse proxy with round-robin, least-connections and power-of-two-choices, active health checks, passive ejection, connection pooling to backends, timeouts, retries for idempotent requests, and hot reload of config.
**Learn.** Streaming bodies without buffering, hop-by-hop headers, pool lifecycle, health state machines, p2c's near-optimal balance, config reload via `ArcSwap`/`RwLock<Arc<_>>`.
**Read before.** `web/reverse_proxy_load_balancer`, `web/http_client_server_loopback`, `async/async_patterns`. **During.** `networking/tcp_framed_server`, `distributed/retry_backoff_jitter`, `concurrency/snapshot_cow_rcu`, `web/cors_security_headers`.
**Milestones.**
1. Forward GET/POST streaming both ways; strip hop-by-hop headers; add `X-Forwarded-For`.
2. Round-robin and least-connections with a backend pool of 4 mock servers.
3. Health checks every 2 s: 3 failures -> out, 2 successes -> in.
4. Passive ejection on 5 consecutive 5xx; retry idempotent requests once on a different backend.
5. P2C balancing; compare max/mean load against random and round-robin under variable backend latency.
6. SIGHUP reload swaps the backend list without dropping in-flight requests.
**Acceptance.** Kill a backend under load of 1,000 req/s: error rate under 1% for the failover window and 0% after ejection; P2C max backend load <= 1.15x mean with 4 backends of unequal speed vs random 1.5x or worse; reload under load drops zero requests; 100 MB upload streams with under 10 MB RSS growth.
**Data.** Mock backends with configurable latency (exp distribution) and failure rate.
**Write-up.** Which requests are safe to retry? What happens to long-lived WebSocket connections on reload?
**Stretch.** TLS termination with `rustls`; request-level circuit breaking (4.1).
**Resembles.** [Pingora](https://github.com/cloudflare/pingora), Envoy.

## 7.8 `metricsx`: a metrics and observability library  `L`

**Goal.** A library with counters, gauges, histograms (log-linear buckets) and labels, a registry, Prometheus text exposition, and span timing integrated with `tracing`, with cardinality protection and negligible overhead on the hot path.
**Learn.** Atomics-based counters, sharded/thread-local aggregation, histogram bucket design and quantile estimation error, label cardinality explosion, exposition format, scrape consistency.
**Read before.** `dev_tools/tracing_logs`, `concurrency/atomics_ordering`, `perf/false_sharing`. **During.** `concurrency/sharded_map`, `data_structures/hyperloglog`, `perf/bench_harness`, `web/axum_middleware`, 🔜 `dev_tools/metrics_histograms` (planned).
**Milestones.**
1. `Counter` and `Gauge` using atomics; 8-thread test sums exactly.
2. Per-thread sharded counters merged on scrape; benchmark vs a single atomic at 16 threads (report ns/op).
3. Histogram with log-linear buckets (powers of two with 8 sub-buckets); p50/p99 error under 12.5% relative.
4. Labels with an interner and a cardinality cap (default 10,000 series, then drop + count overflow).
5. `/metrics` in Prometheus text format; validate with a small parser you write (and `promtool check metrics` if available).
6. `axum` middleware recording request count and latency histogram by route template (not raw path).
**Acceptance.** Recording a counter costs under 5 ns uncontended and under 20 ns at 16 threads sharded (report your numbers); quantile error bound verified on 1M samples from log-normal; exposition passes `promtool`; 1M distinct label values never grows memory beyond the cap.
**Data.** Latency samples from log-normal, bimodal and heavy-tailed (Pareto) generators.
**Write-up.** Why is the raw path a dangerous label? What consistency do you guarantee between `_count`, `_sum` and buckets during a scrape?
**Stretch.** Exemplars linking buckets to trace ids; OpenTelemetry export (`opentelemetry-rust`).
**Resembles.** [Prometheus](https://github.com/prometheus/prometheus), [tracing](https://github.com/tokio-rs/tracing), [OpenTelemetry Rust](https://github.com/open-telemetry/opentelemetry-rust).

---

## Katas (30 minutes each)

1. Parse `Content-Range`, `Range` and `Cache-Control: max-age=60, public` headers into typed structs; 20 valid and 10 invalid cases.
2. Write a base62 encoder/decoder for `u64`; round-trip 100,000 random values and check the max length is 11.
3. Implement HMAC-SHA256 request signing `t=...,v1=...`; verify with 5-minute tolerance and show a replay is rejected.
4. Write a token-bucket middleware in `axum` returning `429` with `Retry-After`; test with 10 requests in a burst of 5.
5. Build a `/healthz` and `/readyz` pair where `readyz` goes red while a startup migration runs; test the transition.
6. Implement `Retry-After` parsing for both delta-seconds and HTTP-date forms; 8 cases including garbage.
7. Write a TCP echo server and a client that sends 1,000 messages in random chunk sizes; verify the byte stream is identical (framing is your job).
8. Implement a DNS A-record query encoder per RFC 1035 with name compression off; check bytes against a captured `dig` packet.
9. Create a CORS preflight handler that echoes only allowlisted origins; test 6 origins including `null` and a lookalike domain.
10. Decode a JWT without verification, then verify an HS256 signature in constant time; test tampering of header, payload and signature.
