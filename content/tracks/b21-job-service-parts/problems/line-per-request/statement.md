At 3 a.m. a customer says "my upload failed". Your logs hold thousands of interleaved lines from requests running at the same time. Which lines belong to that request?

The `tracing` crate answers this with **spans**. A span is a named period with fields. Every event logged inside it carries that context.

Write `observe(app)`. It returns the same router with request logging around **every** request, unknown paths (404) and wrong methods (405) included. Responses pass through unchanged.

**The span.** Each request runs inside its own span named `http`, at level INFO, with these fields:

- `method`: the request method, such as `GET`;
- `path`: the path **without** the query string, because queries often hold secrets;
- `client`: the value of the `x-client` header if it is 1 to 32 ASCII letters, digits or `-`. Otherwise `unknown`.

Everything a handler logs while it serves the request is inside that request's span. This holds after the handler has waited, while other requests are being served on the same thread. One request's logs are never inside another's span.

**The events.** `observe` logs exactly **one** event for each request, inside its span:

- when there is a response: an **INFO** event with a field `status`, the status code as a number (such as `404`);
- when the request's future is dropped **before** there is a response, because the client gave up: a **WARN** event, and no INFO event. It has no `status` field.

Event messages and any other fields are up to you. Log with the default target, and create no other spans. `observe` logs no other events. It installs no subscriber: the program that uses it does that.

(What happens when a handler panics is not tested.)
