# Track: Networking and web services

About 4 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Wire formats

**Goal:** Bytes on the wire.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`binary_format_parsing`](../../examples/text_and_parsing/binary_format_parsing.rs) · advanced | Parse safely. |
| 1.2 | [`resp_protocol`](../../examples/networking/resp_protocol.rs) · advanced | A text/binary protocol. |
| 1.3 | [`http1_parser`](../../examples/networking/http1_parser.rs) · advanced | HTTP framing. |
| 1.4 | [`websocket_frames`](../../examples/networking/websocket_frames.rs) · intermediate | Framing and masking. |
| 1.5 | [`dns_message_codec`](../../examples/networking/dns_message_codec.rs) · advanced | Compression pointers. |

### Step 2: Sockets and event loops

**Goal:** How servers wait.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`tcp_framed_server`](../../examples/networking/tcp_framed_server.rs) · advanced | Length-prefixed protocols. |
| 2.2 | [`kqueue_epoll_raw`](../../examples/networking/kqueue_epoll_raw.rs) · advanced | Readiness APIs. |
| 2.3 | [`mio_event_loop`](../../examples/networking/mio_event_loop.rs) · advanced | An event loop. |
| 2.4 | [`udp_sockets_reliable`](../../examples/networking/udp_sockets_reliable.rs) · advanced | Reliability on UDP. |
| 2.5 | [`tcp_congestion_control_sim`](../../examples/networking/tcp_congestion_control_sim.rs) · advanced | Congestion control. |

### Step 3: Services

**Goal:** Production concerns.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`axum_router_state`](../../examples/web/axum_router_state.rs) · intermediate | Routing and state. |
| 3.2 | [`axum_errors`](../../examples/web/axum_errors.rs) · intermediate | Error responses. |
| 3.3 | [`axum_middleware`](../../examples/web/axum_middleware.rs) · intermediate | Layers. |
| 3.4 | [`rest_api_patterns`](../../examples/web/rest_api_patterns.rs) · advanced | Pagination, versions. |
| 3.5 | [`axum_idempotency`](../../examples/web/axum_idempotency.rs) · intermediate | Safe retries. |
| 3.6 | [`auth_basic_bearer_jwt`](../../examples/web/auth_basic_bearer_jwt.rs) · advanced | AuthN. |
| 3.7 | [`http_client_resilience`](../../examples/web/http_client_resilience.rs) · advanced | Timeouts/retries. |
| 3.8 | [`reverse_proxy_load_balancer`](../../examples/web/reverse_proxy_load_balancer.rs) · advanced | Proxying. |
| 3.9 | [`webhooks_signatures`](../../examples/web/webhooks_signatures.rs) · advanced | Verify callbacks. |
| 3.10 | [`job_service_axum`](../../examples/capstones/job_service_axum.rs) · capstone | Capstone. |
