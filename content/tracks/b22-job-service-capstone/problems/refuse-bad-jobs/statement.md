**Capstone, step 1 of 8.** You are building a service that resizes images in the background. Clients send a job over HTTP. Before there is a service, decide what a job is and how the service says no.

**A job request** is a JSON object:

| Field | Rule | If left out |
|---|---|---|
| `source` | string, not blank (empty or only white space) | not allowed |
| `width`, `height` | whole numbers from 1 to 8192 | not allowed |
| `format` | `png`, `jpg` or `webp`, exactly | `png` |
| `work_ms` | whole number, at most 10000; how long the simulated work takes | `20` |

**`parse_spec(body)`** turns the bytes into a `Spec`, keeping each string as sent.

- If the body is not such a JSON object, the answer is `ApiError::InvalidJson(why)`. That covers broken or non-UTF-8 input and text after the object. It also covers a missing required field, `null` or a wrong type, a number too big for its Rust type, and **any field not in the table**.
- Otherwise every rule is checked. If any fails, the answer is `ApiError::Validation(details)` with **one entry per bad field**, in the table's order. Each entry is `field: what is wrong`, such as `width: must be from 1 to 8192`. The wording after `: ` is up to you.

**Errors as HTTP.** Implement `IntoResponse` for `ApiError`. Every error is sent as `application/json` with this body, with exactly these keys:

```json
{"error": {"code": "validation_failed", "message": "...", "details": ["width: ..."]}}
```

`message` is non-empty text. `details` holds the entries of `Validation` and is `[]` for every other variant. For `NotFound`, `NotCancellable` and `BadQuery`, the message contains the variant's string.

| Variant | Status | `code` |
|---|---|---|
| `MissingKey` | 400 | `missing_idempotency_key` |
| `InvalidJson` | 422 | `invalid_json` |
| `Validation` | 422 | `validation_failed` |
| `KeyReuse` | 409 | `idempotency_key_reuse` |
| `NotFound` | 404 | `job_not_found` |
| `NotCancellable` | 409 | `not_cancellable` |
| `BadQuery` | 400 | `bad_query` |
| `ShuttingDown` | 503 | `shutting_down` |
| `NoRoute` | 404 | `no_route` |
| `MethodNotAllowed` | 405 | `method_not_allowed` |

Later steps use these variants. Keep `Debug`, `Clone` and `PartialEq` on `ApiError` and `Spec`.
