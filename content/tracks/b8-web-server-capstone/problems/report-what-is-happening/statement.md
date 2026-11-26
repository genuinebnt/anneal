> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 8. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 9 of 10.** Measure the server. `Config` gains `metrics: Arc<Metrics>`; the server records into it. `metrics_handler(metrics)` is a handler serving `metrics.render()` with `Content-Type: text/plain; version=0.0.4`.

**`Histogram`** uses memory that does not grow with the number of records and takes any `u64`. `record` takes `&self` and is safe from many threads. `sum` saturates. For `0.0 <= q <= 1.0`, let the exact value be the one of rank `max(1, ceil(q * count))` in sorted order: `quantile(q)` is at least that value, at most that value plus an eighth of it (rounded down), and never above the largest recorded value. Empty: 0.

**The report** is Prometheus text: lines `name value`, and before each metric `# TYPE name counter|gauge|histogram`.

| Name | Meaning |
|---|---|
| `http_connections_accepted_total` (counter) | connections accepted, refused ones included |
| `http_connections_rejected_total` (counter) | connections refused because the server was full |
| `http_connections_active` (gauge) | connections a worker is serving now |
| `http_queue_depth` (gauge) | accepted connections waiting for a worker |
| `http_requests_total` (counter) | final responses sent: handler replies, error replies, 408 and 503; not `100 Continue` |
| `http_requests_reused_total` (counter) | requests that were not the first on their connection |
| `http_responses_total{class="Nxx"}` (counter) | the same responses by status class, `1xx` to `5xx`, all five always listed |
| `http_request_duration_us` (histogram) | time spent in `app.handle`, in microseconds |
| `http_request_duration_us_p50`, `..._p99` (gauge) | quantiles of those times |

The histogram has lines `http_request_duration_us_bucket{le="B"}` for B = 100, 250, 500, 1000, 2500, 5000, 10000, 50000, 100000 and `+Inf`, in that order; each counts the durations **less than or equal to** B. Then `..._sum` and `..._count`.

A response is counted before it is written, so a client that has received it finds it counted. The report served by a request does not yet count that request.
