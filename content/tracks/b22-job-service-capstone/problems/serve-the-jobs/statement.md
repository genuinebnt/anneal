> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
> - Keep the jobs and accept each once
>
> It also gives away much of these preparation problems. Solve them first:
>
> - One error shape for the whole API
> - Charge the card once
>
> The code is a stand-in, because the app cannot yet carry your own code from one step to the next. You may replace it with your own; keep the public names.

**Capstone, step 3 of 8.** Put the store on the network. Write `router(store)`. The tests drive it in memory with `tower::ServiceExt::oneshot`. A job changed through the store shows in the router's answers.

**`POST /v1/jobs`** creates a job. Checks run in this order:

1. The `Idempotency-Key` header must be present and not empty, else `MissingKey`. (Keys are visible ASCII in the tests.)
2. The body goes through `parse_spec`. The `Content-Type` is not checked.
3. The store decides. A **new** job answers **201** with the job as JSON and `Location: /v1/jobs/{id}`. A **retry** answers **200** with the job as it is now and the header `idempotent-replayed: true`, and no `Location`. A new job's answer has no `idempotent-replayed` header.

A refused request creates nothing and does not use up the key.

**`GET /v1/jobs/{id}`** answers 200 with the job. An id that is unknown or not a `u64` is `NotFound`. That includes one that does not decode to text, such as `%FF`.

**`GET /v1/jobs?after=A&limit=L`** lists jobs with ids above `A`, in id order, each in the same JSON form as above:

```json
{"items": [ ...jobs... ], "next_after": 7}
```

- `after` defaults to 0. `limit` defaults to 10 and is brought into `1..=100`.
- `next_after` is the id of the last item when at least one more job exists after it. Otherwise it is `null`.
- A value of `after` or `limit` that is not a `u64`, empty included, is `BadQuery`, with `name=value` in its string. Every `u64` is valid. Other parameters are ignored.

**Everything else** keeps the error shape: an unknown path is `NoRoute`, and a known path with another method is `MethodNotAllowed`.

Walking the pages while new jobs arrive must show every job exactly once.
