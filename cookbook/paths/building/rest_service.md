# Building: A production-style REST service



Use this when you have a real project in front of you. For each stage: **attempt your design first**, then read the listed recipes to compare, steal what is better and note what you missed.

### Step 1: Skeleton

**Goal:** Routing, state, errors.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`axum_router_state`](../../examples/web/axum_router_state.rs) · intermediate | State and handlers. |
| 1.2 | [`axum_errors`](../../examples/web/axum_errors.rs) · intermediate | One error type to responses. |
| 1.3 | [`axum_middleware`](../../examples/web/axum_middleware.rs) · intermediate | Auth, logging, limits. |
| 1.4 | [`tracing_logs`](../../examples/dev_tools/tracing_logs.rs) · intermediate | Structured logs. |

### Step 2: API quality

**Goal:** What clients feel.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`rest_api_patterns`](../../examples/web/rest_api_patterns.rs) · advanced | Pagination, filtering. |
| 2.2 | [`axum_idempotency`](../../examples/web/axum_idempotency.rs) · intermediate | Retries. |
| 2.3 | [`http_caching_conditional`](../../examples/web/http_caching_conditional.rs) · advanced | ETags. |
| 2.4 | [`cors_security_headers`](../../examples/web/cors_security_headers.rs) · intermediate | Headers. |
| 2.5 | [`axum_sse_replay`](../../examples/web/axum_sse_replay.rs) · intermediate | Streaming. |

### Step 3: Security

**Goal:** Who is calling.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`auth_basic_bearer_jwt`](../../examples/web/auth_basic_bearer_jwt.rs) · advanced | Tokens. |
| 3.2 | [`cookies_sessions_csrf`](../../examples/web/cookies_sessions_csrf.rs) · advanced | Sessions. |
| 3.3 | [`oauth2_pkce_flow`](../../examples/web/oauth2_pkce_flow.rs) · advanced | OAuth. |
| 3.4 | [`webhooks_signatures`](../../examples/web/webhooks_signatures.rs) · advanced | Signed callbacks. |
| 3.5 | [`constant_time_side_channels`](../../examples/cryptography/constant_time_side_channels.rs) · intermediate | Comparisons. |
| 3.6 | [`secure_random_and_passwords`](../../examples/cryptography/secure_random_and_passwords.rs) · intermediate | Hashing. |

### Step 4: Resilience

**Goal:** Failure of dependencies.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`http_client_resilience`](../../examples/web/http_client_resilience.rs) · advanced | Timeouts, retries. |
| 4.2 | [`retry_backoff_jitter`](../../examples/distributed/retry_backoff_jitter.rs) · simple | Backoff. |
| 4.3 | [`token_bucket`](../../examples/distributed/token_bucket.rs) · intermediate | Limit. |
| 4.4 | [`rate_limiter_lab`](../../examples/capstones/rate_limiter_lab.rs) · capstone | Compare limiters. |
| 4.5 | [`graceful_shutdown`](../../examples/async/graceful_shutdown.rs) · intermediate | Deploy safely. |
