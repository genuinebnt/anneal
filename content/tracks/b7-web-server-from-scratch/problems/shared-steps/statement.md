Every request to your service goes through the same chores: check a key, write a log line, retry, make sure that a bug in one handler produces an error reply instead of taking something down. You want to write each chore once and stack the ones you need around any handler.

A **step** is a function `(call, next) -> Answer`. `next` runs everything inside this step, ending at the handler. A step may

- call `next` and return its answer, changed or not;
- not call it, and answer itself;
- call it with a different `Call`;
- call it more than once.

`Pipeline::new(handler)` has no steps. `wrap(step)` adds one; the **first** step added is the outermost. `run(call)` sends a call through. A pipeline can be shared by many threads, which run calls at the same time.

**`shield(report)`** builds a step that protects what is outside it from a panic in anything inside it. When something inside panics, the step returns `Answer { status: 500, body: "internal error" }` and calls `report` once with the panic's message (the text given to `panic!`; for any other payload, `"unknown panic"`). The message never appears in an answer. Steps outside the shield see the 500 as an ordinary answer; a panic in a step outside it is not caught. After a panic the pipeline works as before.
