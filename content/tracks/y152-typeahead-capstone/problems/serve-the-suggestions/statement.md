> **Spoiler before you open the editor.** The starter holds reference code for steps 1 to 3. It also holds `Latencies`, a compact form of the answer to **How long do requests take?** Solve those first.
>
> Solve **One error shape for the whole API** too: this step does not teach axum from the start.

**Capstone, step 4 of 4.** Put the service on the network. Write `router(service, clock)`.

The tests drive it in memory with `tower::ServiceExt::oneshot`. They may also write to the same `service` directly.

All time comes from `clock`. `seconds()` is the `now` of every service call.

**`GET /suggest?q=Pyhton&limit=3`** answers 200:

```json
{"q":"Pyhton","key":"pyhton","cached":false,
 "suggestions":[{"term":"python","score":12.0,"kind":"typo","dist":1}]}
```

`q` is as sent, and `""` when missing. `key` is its search key. `limit` is 5 when missing. When present, it must be a whole number from 0 to `u64::MAX` (18446744073709551615), in decimal digits. Such a number is brought into `1..=20`.

Any other value must answer some 4xx: for example `-1`, `2.5`, an empty value, `lots` or 18446744073709551616. Reading it as `Option<u64>` does this.

`score` is rounded as `(score * 1000.0).round() / 1000.0`. `kind` is `prefix`, `infix` or `typo`.

**`POST /events`** with `{"term":"Python"}` records one choice. It answers 200 `{"ok":true}`, or 404 for an unknown term.

**`POST /index`** with `{"terms":[{"term":"zig","count":3}]}` adds the pairs in one call of `add`. A missing `count` is 1. It answers 200 `{"added":1,"existing":0}`.

**Limits.** Text from a stranger has a size limit, counted in **characters**:

- a `q` or an event's `term` over `MAX_TEXT_CHARS` (128) answers 400;
- more than `MAX_BATCH` (1 000) pairs, or any term over 128 characters, answers 400. Then **nothing** is added.

Every 404 above and every 400 for a limit has the body `{"ok":false,"error":"..."}` with a non-empty message. A request with a bad `limit` (see above) or a bad JSON body must answer some 4xx. Its body is up to you.

**`GET /metrics`** answers 200 with plain text, one `name value` line each, ending in a newline. Lines that start with `#` are ignored. Values are whole numbers.

| Name | Value |
|---|---|
| `typeahead_suggest_requests_total` | suggest requests answered 200 |
| `typeahead_cache_hits_total` / `typeahead_cache_misses_total` | those answered with `cached` true / false |
| `typeahead_events_total` / `typeahead_unknown_events_total` | events answered 200 / 404 |
| `typeahead_terms_added_total` | the sum of `added` over `POST /index` answers |
| `typeahead_indexed_terms` | `service.len()` now |
| `typeahead_suggest_latency_us{quantile="0.5"}` | see below |
| `typeahead_suggest_latency_us{quantile="0.99"}` | see below |
| `typeahead_suggest_latency_us_sum` / `typeahead_suggest_latency_us_count` | their sum and number |

Counters belong to one router. A refused request counts in none of them.

**Latency.** For each suggest request answered 200, call `clock.micros()` **exactly twice**: before and after the call of `service.suggest`. The difference is its latency. Never call `micros()` otherwise.

 For a quantile `p`, let `exact` be the latency of rank `max(1, ceil(p * count))` in rising order. The reported value is at least `exact`, at most `exact + exact / 8`, and never above the largest latency. With no latencies it is 0. `Latencies` already does this.

**Not graded, and left out:** a real socket and a clean shutdown (see **Stop on time**), access control and rate limits. So are a limit on the number of terms, and moving slow searches off the async worker threads. The Cookbook service also compares two kinds of store under load, which needs timing and is not a task here.
