Two people open the same wiki page. Both edit it. The one who saves second wipes out the first one's work, and nobody notices.

HTTP has a standard way to stop this. A reader gets a **version tag** with the page. A writer must say which version their edit started from. If the page has moved on, the write is refused.

Write `router()`, a small page service with axum. It starts with no pages. Every page has a version: 1 when created, one more after each successful write. Its tag is `"v1"`, `"v2"` and so on, **with the double quotes**.

A page in JSON is `{"title": "Home", "body": "Hello"}`.

**`GET /pages/{slug}`** answers 200 with the page as JSON and its tag in the `ETag` header. An unknown page is 404.

**`PUT /pages/{slug}`** creates or replaces a page. The checks run in this order, and the first that fails decides the status:

1. The request must carry a precondition. With neither `If-Match` nor `If-None-Match`, answer **428**. With both, answer **400**. An `If-None-Match` value other than `*` is also **400**.
2. The body must be a JSON object with a string `title` and an optional string `body` (default `""`; `null` is not a string). Any other field is refused. Anything that does not fit is **400**. The `Content-Type` header is not checked.
3. The precondition must hold, or the answer is **412**:
   - `If-None-Match: *` holds only when the page does **not** exist.
   - `If-Match` holds only when the page exists and the value is exactly its current tag. Anything else fails: an old tag, `v2` without quotes, `W/"v2"`, `*` or a list of tags. (Real HTTP allows more forms.)

A **412 for an existing page** carries its current tag in `ETag`. A 412 for a missing page has no `ETag`.

On success, answer with the stored page as JSON (both fields) and the new tag in `ETag`. A new page is **201** with `Location: /pages/{slug}`. A replaced page is **200**.

**For the tests.** Also write `router_with_hook(hook)`. It is `router()`, and its write handler also calls `hook(slug)` each time a precondition is found to **hold**: right after that comparison, as the very next thing, before anything is changed. A refused request does not call it. `router()` behaves like `router_with_hook(|_| {})`. The tests use the hook to pause one write and send a second one in the meantime. It may take a long time, and it never calls the router on its own thread.

The check and the change must still be one step. The tests only probe the gap right after the hook.

A refused request changes nothing. When many writes naming the same version arrive at the same moment, **exactly one** succeeds. Pages do not affect each other. The bodies of refused requests are up to you.
