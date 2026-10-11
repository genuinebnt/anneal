`no such file` is a true message and a useless one. Which file? Opened by whom, for what? Six layers up, the code that called `open` knew: it was starting the log. The layers in between knew nothing and should not have to. The answer is **context**: each layer, as the error passes up through it, adds one sentence about what it was doing, and the original error stays underneath, untouched.

This stage adds that to `DbError`, and the three ways of looking at the result: one layer for a status line, the whole chain for a log, the root cause for a program that wants to know what really happened.

> [!CHECK] `read_page(id)` fails with an I/O error "no such file". Three functions up, the log should say: "starting up: opening the log: reading page 7: no such file". Which of those pieces does each function know? And what must `Display` show for the top error, if the log line is the *report*?
> ||Each function knows only what *it* was doing: `read_page` knows the page, its caller the log, the top one that the database was starting. The I/O error knows only the file problem. `Display` shows **one layer**, the outermost ("starting up"): it is what a status line prints. The log line is the **report**, which joins every layer's message from the outside in. If `Display` printed everything, the report would repeat the cause once per layer.
>
> - Which function adds which piece, and in what order does the report read them?
> - What would a status line look like if `Display` showed the whole chain?
> - What is the root cause here, and who needs it?

## The task

In `src/common/db_error.rs`:

- **`context(message)`** on a `DbError`: a new error that says `message` and has the old one as its cause. Its `Display` is the message and nothing else.
- **`chain()`:** a boxed iterator over the error itself, then its cause, then the cause's cause, to the innermost.
- **`root_cause()`:** the last of the chain.
- **`report()`:** every layer's message from the outermost to the innermost, joined with `": "`. A layer that says exactly what the one below it says (an error made by `From` says what its cause says) is shown once.
- **`.context(..)` and `.with_context(|| ..)` on a `Result`** whose error converts into a `DbError`: convert, then add the layer. `with_context` builds its message only if there is an error.

## Your freedom

How the chain is walked (an iterator you write, `successors`, a loop), how the layers are stored. `chain()` returns a boxed iterator, so that the stub compiles before you write it. Whether the repeated-message rule is a comparison of strings or something smarter.

## The Rust toolbox

**An extension trait on `Result`.** `Context<T>` is implemented for every `Result<T, E>` whose `E` converts into a `DbError`, so `parse(x).context("...")?` works on any layer's result and on a `DbResult` alike.

**`successors`.** `std::iter::successors(first, |e| next)` turns "and then ask for the next one" into an iterator. The closure's types need a small lifetime care: `|&e| e.source()` copies the reference out.

**A closure for laziness.** `with_context(|| format!(..))` passes code, not a value: the message is not built unless it is needed.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): joining pieces into a report string.
- [S6 Trait objects](/t/s6-trait-objects): walking a chain of `&dyn Error`.

## Tests

- A context shows only its own message, and its source is the error it wraps.
- `report()` lists the layers outermost first, the cause once.
- `chain()` counts every layer; `root_cause()` is the original error.
- `.context` and `.with_context` on a result convert and wrap; the second builds its message only on failure; both stack across functions.

## Hints

### The chain is the whole design

`source()` already gives one step. `chain()` is "step until it runs out", and `root_cause()` and `report()` are one line each on top of it.

### Count the elements by hand first

An error made by `From` from an `io::Error`, with two contexts on top: write down the four elements and what each says. The report has three sentences, not four.

### `with_context` is `context` with a closure

Do not duplicate the logic: one calls the other, or both call a helper. What differs is only when the message is built.

## Performance

Context costs one allocation per layer, and only on the failure path. `report()` walks the chain once; a chain of a thousand layers is a thousand string pushes, not a thousand re-scans.

**Measure it.** Build an error with 2 000 layers and time `report()`. If it grows faster than the number of layers, something rescans.

## Experiment

Optional. Predict first, then run.

1. **Let `Display` show the whole chain.** Which tests fail? What does a status line look like now?
2. **Call `message()` eagerly in `with_context`.** Which test notices, and what would it cost on a path that rarely fails?

## Other designs

- **`anyhow`'s `{:#}` and `{:?}`:** the alternate format prints the chain on one line, the debug format prints it as a list ("Caused by:").
- **Structured context:** key-value fields (`table = "orders"`, `page = 7`) instead of sentences, so a log can be searched; PostgreSQL has detail and context fields.

## In BusTub

Not part of BusTub. The nearest thing is `std::nested_exception` in C++, which few codebases use.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `try { .. } catch (...) { std::throw_with_nested(Exception(..)); }` | `.context("..")` |
| `std::rethrow_if_nested` in a loop | `chain()` |

**Port rule:** an error crossing a layer gains a sentence, it does not lose its cause.

## Learn more

- [`std::iter::successors`](https://doc.rust-lang.org/std/iter/fn.successors.html) · [`anyhow::Context`](https://docs.rs/anyhow/latest/anyhow/trait.Context.html)
