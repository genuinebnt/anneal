# Web Programming

axum routing, state, errors, SSE, middleware, idempotency; HTTP clients, caching, cookies, auth, CORS, REST patterns.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

17 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`axum_router_state`](axum_router_state.rs) | routes, shared state, JSON in/out, path params, and testing a router WITHOUT a socket | intermediate | every REST service in Rust: routes, shared state, JSON extractors; handlers tested without sockets. | axum, http-body-util, serde, tokio, tower |
| [`axum_errors`](axum_errors.rs) | one error type -> consistent HTTP error responses | intermediate | public APIs that need one stable error shape and no leaked internals. | axum, http-body-util, serde-json, thiserror, tokio, tower |
| [`axum_sse_replay`](axum_sse_replay.rs) | Server-Sent Events with `Last-Event-ID` replay | intermediate | live progress feeds: build logs, job status, notifications; reconnect-safe via Last-Event-ID. | axum, futures-util, http-body-util, tokio, tower |
| [`axum_middleware`](axum_middleware.rs) | middleware with `from_fn`: correlation ids on every request and response | intermediate | request ids/correlation ids across microservices; the same layer shape hosts auth, metrics, tracing. | axum, tokio, tower |
| [`axum_idempotency`](axum_idempotency.rs) | idempotency keys for POST | intermediate | payments, order creation, job submission: safe client retries without duplicates (Stripe-style idempotency). | axum, http-body-util, serde-json, tokio, tower |
| [`http_client_server_loopback`](http_client_server_loopback.rs) | the anatomy of an HTTP/1.1 exchange: raw bytes on the wire, then the same endpoints with reqwest | intermediate | debugging API clients with curl-style traces; writing contract tests against a real socket; | axum, bytes, futures-util, reqwest, serde, serde-json, tokio |
| [`http_caching_conditional`](http_caching_conditional.rs) | HTTP caching and conditional requests: ETag, Last-Modified, Cache-Control, Vary, Range, If-Match | advanced | CDN/browser caching of APIs and assets (fingerprinted files with `immutable`), conditional | axum, bytes, futures-util, reqwest, tokio, tower-http |
| [`cookies_sessions_csrf`](cookies_sessions_csrf.rs) | cookies, server-side sessions, signed cookies and CSRF protection, by hand | advanced | login/logout for server-rendered apps and SPAs with a same-site API; "remember me" vs session | axum, bytes, reqwest, serde, tokio |
| [`auth_basic_bearer_jwt`](auth_basic_bearer_jwt.rs) | authentication on the wire: Basic, API keys, Bearer tokens and a from-scratch HS256 JWT | advanced | protecting REST APIs and microservices (service-to-service JWTs), mobile/SPA access+refresh | axum, bytes, reqwest, serde-json, tokio |
| [`cors_security_headers`](cors_security_headers.rs) | CORS from first principles, security headers, body limits, timeouts and Retry-After | intermediate | SPA front end on another origin calling an API; public read-only APIs (`*` without | axum, bytes, futures-util, reqwest, serde-json, tokio, tower-http |
| [`rest_api_patterns`](rest_api_patterns.rs) | REST API design patterns, shown as real HTTP transcripts | advanced | public/partner APIs, mobile backends, admin consoles with infinite scroll, payment-style APIs | axum, reqwest, serde-json, tokio |
| [`webhooks_signatures`](webhooks_signatures.rs) | webhooks end to end: HMAC signatures, retries with backoff, replay protection, dead letters | advanced | payment providers (Stripe), Git hosts (GitHub X-Hub-Signature-256), chat/CRM/CI integrations, | axum, reqwest, serde-json, tokio |
| [`oauth2_pkce_flow`](oauth2_pkce_flow.rs) | OAuth 2.0 authorization code flow with PKCE, three servers, full redirect chain | advanced | "Sign in with ..." buttons, mobile/SPA apps (public clients: PKCE replaces the client secret), | axum, reqwest, serde-json, tokio |
| [`http_client_resilience`](http_client_resilience.rs) | a resilient HTTP client against a deliberately flaky server | advanced | service-to-service calls, SDKs for third-party APIs, artifact/model/file downloaders, CI fetchers, anything | futures-util, reqwest, serde-json, tokio |
| [`jsonrpc_graphql_dataloader`](jsonrpc_graphql_dataloader.rs) | JSON-RPC 2.0, a mini GraphQL executor, and the DataLoader fix for N+1 | advanced | GraphQL gateways (GitHub, Shopify), BFFs for mobile clients, JSON-RPC in Ethereum nodes / LSP / MCP / editors, | axum, futures-util, reqwest, serde-json, tokio |
| [`multipart_forms_uploads`](multipart_forms_uploads.rs) | form posts and file uploads: urlencoded + multipart/form-data parsed from scratch | intermediate | avatar/document/video uploads, CSV import, HTML form handling, API gateways that must bound memory, antivirus/scan queues. | axum, futures-util, reqwest, serde-json, tokio |
| [`reverse_proxy_load_balancer`](reverse_proxy_load_balancer.rs) | a reverse proxy / load balancer in front of three backends (axum + reqwest) | advanced | nginx/HAProxy/Envoy-style edge proxies, API gateways, canary routing, per-tenant sticky routing, service-mesh sidecars. | axum, bytes, futures-util, reqwest, serde-json, tokio |
