# Practice: web

How to use this page: run the recipe (`cargo run --release --example <name>`), read the transcript it prints, answer Prove you got it, then do Try next.
Most checks here are an in-process request (`oneshot`) or a `curl -v` against the loopback server; write the request first, then the change.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/web/`.

## auth_basic_bearer_jwt
**Prove you got it**
1. What status and header must a 401 carry for Basic?
2. Why must a JWT verifier fix the accepted algorithm instead of reading `alg` from the token?

**Try next**
- [S] Add expiry (`exp`) and `nbf` checks with an injectable clock; test the boundary second.
- [M] Add key rotation by `kid` and test an old token still verifies until retired.
- [A] Cross-check your HS256 against the [Keats/jsonwebtoken](https://github.com/Keats/jsonwebtoken) crate on 1000 random claims.

**Use it in:** API auth. Real code: [Keats/jsonwebtoken](https://github.com/Keats/jsonwebtoken).
**Pitfall:** accepting `alg: none` or confusing HS and RS keys.

## axum_errors
**Prove you got it**
1. Which part of an internal error is logged and which part reaches the client?
2. How does `IntoResponse` keep one JSON shape for all errors?

**Try next**
- [S] Add a variant for validation errors with field names and test the JSON.
- [M] Add a request id from the middleware recipe to every error body.
- [A] Add `thiserror` and `source()` chains, and assert the log contains the chain but the body does not.

**Use it in:** all services. Real code: [tokio-rs/axum](https://github.com/tokio-rs/axum) examples, [dtolnay/thiserror](https://github.com/dtolnay/thiserror).
**Pitfall:** leaking a database error string to the client.

## axum_idempotency
**Prove you got it**
1. What does a retry with the same key and a different body return?
2. What if two identical requests arrive concurrently?

**Try next**
- [S] Add key expiry and test with a fake clock.
- [M] Handle the concurrent case with an in-flight marker; test with 20 parallel requests that the side effect runs once.
- [A] Persist keys in a file or SQLite and restart the server in the test.

**Use it in:** payments. Real code: [svix/svix-webhooks](https://github.com/svix/svix-webhooks).
**Pitfall:** storing the key after the side effect rather than before.

## axum_middleware
**Prove you got it**
1. Which routes does a layer cover relative to where you add it?
2. How do you pass a value from middleware to handlers?

**Try next**
- [S] Add a timing layer that sets `Server-Timing`.
- [M] Add a layer that rejects bodies over a limit before reading them.
- [A] Compose three layers and write a test asserting their order using a recorded trace.

**Use it in:** logging, auth. Real code: [tower-rs/tower-http](https://github.com/tower-rs/tower-http).
**Pitfall:** layer order surprises (outermost runs first on request).

## axum_router_state
**Prove you got it**
1. Why can you test a Router without a socket?
2. How is shared state typed and cloned?

**Try next**
- [S] Add a path param route with a 404 test.
- [M] Add a state-mutating endpoint under a `Mutex`, then 50 concurrent `oneshot` calls, asserting totals.
- [A] Replace the in-memory state with a trait and run the same tests against two implementations.

**Use it in:** any axum app. Real code: [tokio-rs/axum](https://github.com/tokio-rs/axum).
**Pitfall:** holding a `std::sync::Mutex` guard across `.await`.

## axum_sse_replay
**Prove you got it**
1. What does the browser send on reconnect?
2. Why persist event ids?

**Try next**
- [S] Add a test for `Last-Event-ID` beyond the last id.
- [M] Add a live tail via a broadcast channel after the replay, without gaps or duplicates (test by sending during replay).
- [A] Add a retention limit and return a clear "gap" event if the client is too far behind.

**Use it in:** notifications. Real code: [tokio-rs/axum](https://github.com/tokio-rs/axum) SSE examples.
**Pitfall:** replaying and subscribing in the wrong order loses events.

## cookies_sessions_csrf
**Prove you got it**
1. Which cookie attributes defend against XSS theft and cross-site sends?
2. Why do signed cookies need a MAC check in constant time?

**Try next**
- [S] Add session rotation on login and test the old id is dead.
- [M] Implement the double-submit CSRF token and test 5 attack requests.
- [A] Add an absolute and an idle session timeout with a fake clock.

**Use it in:** web login. Real code: [tokio-rs/axum](https://github.com/tokio-rs/axum) (cookie extractors in axum-extra).
**Pitfall:** session fixation if the id is not rotated.

## cors_security_headers
**Prove you got it**
1. Which requests trigger a preflight?
2. Why can credentials not go with `*`?

**Try next**
- [S] Add a test for a disallowed origin with no CORS headers returned.
- [M] Add `Vary: Origin` checks for cached responses.
- [A] Compare your layer with the `tower-http` `CorsLayer` over 30 generated requests.

**Use it in:** SPAs. Real code: [tower-rs/tower-http](https://github.com/tower-rs/tower-http).
**Pitfall:** reflecting any Origin with credentials.

## http_caching_conditional
**Prove you got it**
1. What is the difference between strong and weak ETags?
2. What does `If-Match` protect?

**Try next**
- [S] Add `Range` with a 416 case.
- [M] Add `Vary: Accept-Encoding` and test the cache keys.
- [A] Build the client cache and assert the number of full bodies fetched in a replayed trace.

**Use it in:** CDNs. Real code: [hyperium/hyper](https://github.com/hyperium/hyper).
**Pitfall:** using mtime-only validators on files that change within a second.

## http_client_resilience
**Prove you got it**
1. What is retried and what is not?
2. Which timeout covers a slow body?

**Try next**
- [S] Add a test that a 400 is not retried.
- [M] Add a circuit breaker and assert it opens after N failures.
- [A] Add a hedged request and measure p99 against a server with bimodal latency.

**Use it in:** service calls. Real code: [seanmonstar/reqwest](https://github.com/seanmonstar/reqwest).
**Pitfall:** retrying non-idempotent POST without keys.

## http_client_server_loopback
**Prove you got it**
1. What does chunked framing look like on the wire?
2. What does `Content-Length` mismatch do?

**Try next**
- [S] Send a request with a header split across two TCP writes.
- [M] Parse a pipelined pair of requests from one buffer.
- [A] Write a minimal client that does keep-alive with 100 sequential requests and counts connections.

**Use it in:** debugging HTTP. Real code: [hyperium/hyper](https://github.com/hyperium/hyper).
**Pitfall:** relying on one `read` to get a whole request.

## jsonrpc_graphql_dataloader
**Prove you got it**
1. How many queries does the N+1 cause and how many with the loader?
2. Which JSON-RPC requests get no response?

**Try next**
- [S] Add a batch with a notification and an error and assert the response array.
- [M] Add caching per request in the loader and test it does not leak between requests.
- [A] Compare against [async-graphql/async-graphql](https://github.com/async-graphql/async-graphql) DataLoader on the same schema.

**Use it in:** API gateways. Real code: [paritytech/jsonrpsee](https://github.com/paritytech/jsonrpsee), [async-graphql/async-graphql](https://github.com/async-graphql/async-graphql).
**Pitfall:** a loader shared across requests leaks data between users.

## multipart_forms_uploads
**Prove you got it**
1. How is the boundary chosen and where must it not appear?
2. Why limit part sizes?

**Try next**
- [S] Add a filename with unicode and quotes and test parsing.
- [M] Stream a large upload to disk with a size cap and assert memory stays flat.
- [A] Run your parser against 1000 mutated multipart bodies and assert no panics.

**Use it in:** file uploads. Real code: [tokio-rs/axum](https://github.com/tokio-rs/axum) multipart extractor.
**Pitfall:** trusting the client filename (path traversal).

## oauth2_pkce_flow
**Prove you got it**
1. What attack does PKCE stop?
2. Why check `state`?

**Try next**
- [S] Add a test with a wrong `code_verifier` and assert failure.
- [M] Make codes single-use and short-lived; test replay.
- [A] Add refresh tokens with rotation and reuse detection.

**Use it in:** social login. Real code: [ramosbugs/oauth2-rs](https://github.com/ramosbugs/oauth2-rs), [ory/hydra](https://github.com/ory/hydra).
**Pitfall:** open redirect through unvalidated `redirect_uri`.

## rest_api_patterns
**Prove you got it**
1. Why does offset pagination duplicate or skip rows?
2. When do you use 409 vs 412?

**Try next**
- [S] Add a cursor test under concurrent inserts.
- [M] Add `PATCH` with `If-Match` and test a lost update is rejected.
- [A] Write a contract test suite that any implementation of the API must pass, and run it against two.

**Use it in:** public APIs. Real code: [tokio-rs/axum](https://github.com/tokio-rs/axum).
**Pitfall:** returning 200 with an error body.

## reverse_proxy_load_balancer
**Prove you got it**
1. Which headers are hop-by-hop?
2. Why buffer small bodies?

**Try next**
- [S] Add `X-Forwarded-For` and test its chain.
- [M] Add health checks and ejection; test with a dead backend.
- [A] Add least-connections balancing and compare tail latency with round-robin on a skewed backend.

**Use it in:** edge proxies. Real code: [cloudflare/pingora](https://github.com/cloudflare/pingora), [envoyproxy/envoy](https://github.com/envoyproxy/envoy).
**Pitfall:** retrying non-idempotent requests on another backend.

## webhooks_signatures
**Prove you got it**
1. Why sign the timestamp into the MAC?
2. What does the receiver do with a duplicate event id?

**Try next**
- [S] Test the tolerance window at both edges.
- [M] Add secret rotation with two valid secrets.
- [A] Add a dead-letter queue and a replay tool; test an event is delivered at least once and processed once.

**Use it in:** payments and CI. Real code: [svix/svix-webhooks](https://github.com/svix/svix-webhooks).
**Pitfall:** verifying the parsed JSON instead of the raw bytes.

## Chapter projects

Other-chapter stems used: `hmac_hkdf_pbkdf2` and `chacha20_poly1305` (cryptography), `retry_backoff_jitter`, `token_bucket`, `raft_consensus` (distributed), `serde_defaults` (encoding_compression), `bloom_filter` (search_vectors), `job_service_axum` (capstones).

### Small (about 2 h): hardened endpoint
Combine `axum_errors`, `axum_middleware`, `cors_security_headers`, `serde_defaults`.
Milestones: (1) typed errors; (2) request id and body limit; (3) CORS and headers.
Acceptance: 15 table-driven requests produce expected status and headers; oversize bodies rejected before read.
Stretch: rate limit with `token_bucket`.

### Intermediate (a weekend): webhook delivery service
Combine `webhooks_signatures`, `http_client_resilience`, `retry_backoff_jitter`, `axum_idempotency`.
Milestones: (1) sender with signing; (2) retries with jitter and DLQ; (3) receiver with replay protection and idempotency.
Acceptance: against a server that fails 40% of calls, every event is processed exactly once and the DLQ holds only those over the retry cap.
Stretch: secret rotation.

### Advanced (a week): API gateway
Combine `reverse_proxy_load_balancer`, `auth_basic_bearer_jwt`, `rate_limiter_lab` (capstones), `http_caching_conditional`, `oauth2_pkce_flow`.
Milestones: (1) proxy with health checks; (2) JWT validation; (3) per-key limits; (4) conditional caching; (5) PKCE login flow in front of an app.
Acceptance: load test with 3 backends, one failing, keeps success above 99%; limit fairness test passes; token tests include expiry and wrong key.
Stretch: replace the single gateway with two plus consistent hashing (`consistent_hashing`).
