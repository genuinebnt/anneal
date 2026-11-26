Build the HTTP surface of a tiny user service with axum, where **every** failure has the same JSON shape:

```json
{"error": {"code": "not_found", "message": "user 7 not found"}}
```

First implement `IntoResponse` for `ApiError`:

| Variant | Status | `code` | `message` |
|---|---|---|---|
| `NotFound { id }` | 404 | `not_found` | `user <id> not found` |
| `Invalid(fields)` | 422 | `invalid` | `validation failed`, plus `"fields": [{"field": .., "problem": ..}]` inside `error` |
| `Conflict(what)` | 409 | `conflict` | `what` |
| `BadRequest(message)` | 400 | `bad_request` | the message |
| `UnsupportedMediaType` | 415 | `unsupported_media_type` | `expected application/json` |
| `NoRoute` | 404 | `no_route` | `no such route` |
| `MethodNotAllowed` | 405 | `method_not_allowed` | `method not allowed` |
| `Internal(cause)` | 500 | `internal` | `internal error`. The cause must **not** appear anywhere in the response |

Only `Invalid` has a `fields` key. Then write `router(store)`:

- `GET /users/{id}`: 200 with `{"id": 1, "name": "ada"}`, or `NotFound`.
- `POST /users` with `{"name": "ada"}`: 201 with the new user. The name is trimmed. If it is empty, `Invalid` with
  field `name`, problem `required`; if it is longer than 20 characters, problem `too long`. A name already taken is
  `Conflict("name already taken")`. A failing `Store` is `Internal`.
- Anything extraction rejects uses the same shape. A missing or wrong content type is `UnsupportedMediaType`. A body
  that is not valid JSON, has the wrong types or lacks `name` is `BadRequest`, as is an `id` that is not a `u64`. The
  message of a `BadRequest` from an extractor is up to you.
- An unknown path is `NoRoute`; a known path with another method is `MethodNotAllowed`.

`Store` is given. `Store::insert` returns `Err` when the "database" is down.
