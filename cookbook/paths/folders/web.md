# Chapter `web`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`axum_errors`](../../examples/web/axum_errors.rs) | intermediate | one error type -> consistent HTTP error responses. |
| 2 | [`axum_idempotency`](../../examples/web/axum_idempotency.rs) | intermediate | idempotency keys for POST. |
| 3 | [`axum_middleware`](../../examples/web/axum_middleware.rs) | intermediate | middleware with `from_fn`: correlation ids on every request and response. |
| 4 | [`axum_router_state`](../../examples/web/axum_router_state.rs) | intermediate | routes, shared state, JSON in/out, path params, and testing a router WITHOUT a socket. |
| 5 | [`axum_sse_replay`](../../examples/web/axum_sse_replay.rs) | intermediate | Server-Sent Events with `Last-Event-ID` replay. |
| 6 | [`cors_security_headers`](../../examples/web/cors_security_headers.rs) | intermediate | CORS from first principles, security headers, body limits, timeouts and Retry-After |
| 7 | [`http_client_server_loopback`](../../examples/web/http_client_server_loopback.rs) | intermediate | the anatomy of an HTTP/1.1 exchange: raw bytes on the wire, then the same endpoints with reqwest |
| 8 | [`multipart_forms_uploads`](../../examples/web/multipart_forms_uploads.rs) | intermediate | form posts and file uploads: urlencoded + multipart/form-data parsed from scratch |
| 9 | [`auth_basic_bearer_jwt`](../../examples/web/auth_basic_bearer_jwt.rs) | advanced | authentication on the wire: Basic, API keys, Bearer tokens and a from-scratch HS256 JWT |
| 10 | [`cookies_sessions_csrf`](../../examples/web/cookies_sessions_csrf.rs) | advanced | cookies, server-side sessions, signed cookies and CSRF protection, by hand |
| 11 | [`http_caching_conditional`](../../examples/web/http_caching_conditional.rs) | advanced | HTTP caching and conditional requests: ETag, Last-Modified, Cache-Control, Vary, Range, If-Match |
| 12 | [`http_client_resilience`](../../examples/web/http_client_resilience.rs) | advanced | a resilient HTTP client against a deliberately flaky server |
| 13 | [`jsonrpc_graphql_dataloader`](../../examples/web/jsonrpc_graphql_dataloader.rs) | advanced | JSON-RPC 2.0, a mini GraphQL executor, and the DataLoader fix for N+1 |
| 14 | [`oauth2_pkce_flow`](../../examples/web/oauth2_pkce_flow.rs) | advanced | OAuth 2.0 authorization code flow with PKCE, three servers, full redirect chain |
| 15 | [`rest_api_patterns`](../../examples/web/rest_api_patterns.rs) | advanced | REST API design patterns, shown as real HTTP transcripts |
| 16 | [`reverse_proxy_load_balancer`](../../examples/web/reverse_proxy_load_balancer.rs) | advanced | a reverse proxy / load balancer in front of three backends (axum + reqwest) |
| 17 | [`webhooks_signatures`](../../examples/web/webhooks_signatures.rs) | advanced | webhooks end to end: HMAC signatures, retries with backoff, replay protection, dead letters |
