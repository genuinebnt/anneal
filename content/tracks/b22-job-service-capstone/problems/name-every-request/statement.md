> **Spoiler before you open the editor.** The starter of this step contains reference code that solves these earlier steps:
>
> - Refuse a bad job and say why
> - Keep the jobs and accept each once
> - Post a job, read it and page through jobs
> - Run the jobs, a few at a time
> - Cancel a job
> - Watch a job as it happens
> - Shut the service down
>
> It also gives away much of these preparation problems. Solve them first:
>
> - One error shape for the whole API
> - Charge the card once
> - Pages that hold still while the list changes
> - Background jobs within a memory budget
> - Run one piece of work and say how it ended
> - A progress feed you can leave and rejoin
> - Live prices in the browser
> - Close the door, then wait for everyone to leave
>
> The code is a stand-in, because the app cannot yet carry your own code from one step to the next. You may replace it with your own; keep the public names.

**Capstone, step 8 of 8.** When a client reports a problem, you need to find its request in the logs. Give every request an id, send it back, and log under it.

Every response from `router`, whatever its status, carries an **`x-request-id`** header. This includes unknown paths, wrong methods and event streams.

- If the request's `x-request-id` is 1 to 64 **visible ASCII** characters (so no spaces or tabs), send the same value back.
- Otherwise make a new one. It starts with `req-` and is never made twice by the same process, even across routers.

**Logging** uses `tracing`:

- each request runs inside its own span named `request`, with the fields `request_id` (the id sent back), `method` and `path` (without the query string);
- for each request, exactly **one** event with a field `status` (the status code as a number) is logged inside that span, at level INFO. For an event stream it is logged when the response starts, not when the body ends;
- other events you log must not have a `status` field. Messages and other fields are up to you. Log with the default target;
- install no subscriber.

Nothing else changes: all routes and rules of the earlier steps hold. Other headers and bodies pass through untouched, and an event stream still arrives event by event.

**Optional afterwards** (not graded): write a small `main` that serves the router with `axum::serve` and calls `shutdown` on Ctrl-C. Then compare your service with the Cookbook's version and its scripted scenarios.
