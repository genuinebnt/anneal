---
title: Errors that carry their cause: chains, context, codes
summary: One error type for a whole system: why every layer's error becomes a cause instead of text, how a chain of causes and a layer of context are walked and reported, why `From` is written once per layer, and how a stable code and a client-safe message keep a database's inside private.
minutes: 12
---
A client sends `insert into orders select * from archive`. The statement fails. Somewhere below there is a disk that could not be read, or a column name that does not exist, or a number that does not fit; and between that cause and the client there are five layers, each with its own error type and its own idea of what a failure is. The client wants one answer: *what went wrong, in words I can read, with a code my program can switch on*. The operator wants a different one: *every layer it passed through, down to the file that could not be opened*. And nobody wants the second answer shown to the first reader, because it contains paths and pointers.

That is the job of a database's error type. It has to **keep the cause as a value**, **let each layer add what it was doing**, **carry a stable code**, and **have two faces**: a full report for the log and a short message for the client.

## The cause is a value, not a sentence

The easy way to convert an error is to turn it into text: `DbError::new(format!("{e}"))`. The result prints well and is useless for everything else. A caller can no longer ask *was this a permission problem?*, a retry loop cannot tell a timeout from a corrupt page, and the code that picks an error code has to parse English.

So the error **keeps the original as a value**, boxed behind `dyn Error`, and exposes it through `source()`. A caller walks the chain and, where it cares, asks each link what it is with `downcast_ref::<io::Error>()`.

```svg
caption: One failure, four links. `chain()` walks left to right through `source()`; `root_cause()` is the last link; the code is read from it, so the layers added on the way up never change it. `report()` is for the log, `client_message()` for the client. (Animated: the walk.)
<svg viewBox="0 0 760 290" role="img" aria-label="A chain of four errors from the outermost context to an I/O error, with the report, the code and the client message derived from it">
<style>
@keyframes ec-a{0%,18%{opacity:1}24%,100%{opacity:0}}
@keyframes ec-b{0%,22%{opacity:0}28%,42%{opacity:1}48%,100%{opacity:0}}
@keyframes ec-c{0%,46%{opacity:0}52%,66%{opacity:1}72%,100%{opacity:0}}
@keyframes ec-d{0%,70%{opacity:0}76%,94%{opacity:1}100%{opacity:0}}
.ec-a{opacity:0;animation:ec-a 10s ease-in-out infinite}.ec-b{opacity:0;animation:ec-b 10s ease-in-out infinite}
.ec-c{opacity:0;animation:ec-c 10s ease-in-out infinite}.ec-d{opacity:0;animation:ec-d 10s ease-in-out infinite}
</style>
<defs><marker id="ec-ar" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<rect class="hot" x="15" y="50" width="160" height="62" rx="4"/><text class="t-a" x="27" y="74">DbError</text><text class="fg sm" x="27" y="94">"starting up"</text>
<rect class="blue" x="205" y="50" width="160" height="62" rx="4"/><text class="t-b" x="217" y="74">DbError</text><text class="fg sm" x="217" y="94">"opening the log"</text>
<rect class="blue" x="395" y="50" width="160" height="62" rx="4"/><text class="t-b" x="407" y="74">DbError</text><text class="fg sm" x="407" y="94">"no such file"</text>
<rect class="live" x="585" y="50" width="160" height="62" rx="4"/><text class="t-g" x="597" y="74">io::Error</text><text class="fg sm" x="597" y="94">NotFound, "no such file"</text>
<path class="ln" d="M175 81 H203" marker-end="url(#ec-ar)"/><path class="ln" d="M365 81 H393" marker-end="url(#ec-ar)"/><path class="ln" d="M555 81 H583" marker-end="url(#ec-ar)"/>
<text class="mid dim sm" x="189" y="68">source()</text><text class="mid dim sm" x="379" y="68">source()</text><text class="mid dim sm" x="569" y="68">source()</text>
<rect class="ec-a" x="12" y="47" width="166" height="68" rx="5" style="fill:none;stroke:var(--acc);stroke-width:2.6"/>
<rect class="ec-b" x="202" y="47" width="166" height="68" rx="5" style="fill:none;stroke:var(--acc);stroke-width:2.6"/>
<rect class="ec-c" x="392" y="47" width="166" height="68" rx="5" style="fill:none;stroke:var(--acc);stroke-width:2.6"/>
<rect class="ec-d" x="582" y="47" width="166" height="68" rx="5" style="fill:none;stroke:var(--acc);stroke-width:2.6"/>
<text class="dim sm" x="15" y="136">context layers: what each part of the system was doing</text>
<text class="dim sm" x="395" y="136">made by From: says what its cause says</text>
<text class="mid t-g sm" x="665" y="136">root_cause()</text>
<text class="fg sm" x="15" y="176">report()</text><text class="t-g sm" x="95" y="176">starting up: opening the log: no such file</text>
<text class="fg sm" x="15" y="198">code()</text><text class="t-a sm" x="95" y="198">58030, read from the root cause: the layers on top never change it</text>
<text class="fg sm" x="15" y="220">client_message()</text><text class="t-r sm" x="135" y="220">ERROR 58030: internal error</text>
<text class="dim sm" x="15" y="256">The log gets the whole chain. The client gets the code and, for its own mistakes, one message; for a failure of the</text>
<text class="dim sm" x="15" y="272">system itself it gets nothing of the inside: the path and the layers could tell a stranger how the system is built.</text>
</svg>
```

## `From`, once per layer

`?` converts an error with `From`. It is tempting to write one `impl<E: Error> From<E> for DbError` that accepts everything, and it does not compile: the standard library already has `impl<T> From<T> for T`, and the moment `DbError` is itself an `Error` the two overlap. (`anyhow` avoids it by making its error type *not* implement `Error`.) A type that is a real `Error`, so that it can be boxed, downcast and put in another chain, takes one small `From` impl per layer. They are all the same three lines, and a macro writes them.

The lower layers also have to be real errors to be kept as causes: a plain enum with only `Debug` cannot go behind `dyn Error`. Giving each a `Display` and an empty `Error` impl is a few lines, and it is where the layer says, in words, what its variants mean.

## Context: say what you were doing

The cause says *what* went wrong; only the caller knows *what it was trying to do*. A layer adds that with a context: a new error whose message is "opening the log" and whose source is the old one. Context is cheap to write if it is a method on `Result`:

```text
read_page(id).context("reading page 7")?
```

and cheap to run if the message is built only on failure (`with_context(|| format!(...))`): a message formatted on every successful call would be a cost paid by the 99.99% that never fail.

## One layer's message, or all of them

`Display` shows **one** layer: the outermost. That is what a program prints in a status line. The log wants the story, so a **report** joins every layer's message, outermost first. An error made by `From` says exactly what its cause says, so the report shows that sentence once, not twice.

## Codes, and the client boundary

Messages are for people and change when someone improves the wording. A program needs something that does not change: a **code**. SQL standardised one, SQLSTATE: five characters, the first two the *class* (`22` data exception, `42` syntax or access rule, `58` system error, `XX` internal error). PostgreSQL, MySQL and most drivers carry it, and client libraries switch on it ("retry on `40001`").

Where does the code come from? From the **root cause**, not from the outermost layer. A context layer says what the caller was doing; it must not turn a division by zero into something else. That makes "adding context never changes the code" true by construction, not by discipline.

The last decision is what leaves the building. Errors in the client's own statement (a missing table, a division by zero) are the client's business: show the message. Errors of the system (class `58`, class `XX`) are not: a message that holds a path or a pointer value tells a stranger how the system is built, and tells an attacker where to look. The client sees the code and "internal error"; the log sees the chain.

## Invariants: what must always be true

- **The cause is kept as a value.** A walk of the chain can still ask each link what it is; nothing is flattened to text on the way up.
- **Context adds a layer and never changes the code.** The code is read from the innermost cause.
- **`Display` is one layer; the report is all of them,** outermost first, with a message that repeats its cause shown once.
- **The client sees a code and, for its own mistakes only, a message.** Internal failures say that they are internal.
- **The error is `Send + Sync + 'static`.** It crosses threads and channels and can be boxed without a lifetime in the way.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `throw Exception(type, msg)`; `catch (const Exception &e)` | `return Err(DbError::from(..))`; `match` or `?` |
| `std::exception::what()` | `Display` (one layer) and `report()` (the chain) |
| `std::nested_exception`, `std::rethrow_if_nested` | `source()` and `chain()` |
| `dynamic_cast<const IoError *>(&e)` | `downcast_ref::<io::Error>()` |
| PostgreSQL `ereport(ERROR, errcode(ERRCODE_UNDEFINED_TABLE), ...)` | a `code()` read from the root cause |

## In real code

### Using it: a chain of causes, walked and asked

```rust test
use std::error::Error;
use std::fmt;

/// An error that says something and may keep the error it wraps.
#[derive(Debug)]
struct Layer {
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for Layer {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|e| e as &(dyn Error + 'static))
    }
}

fn wrap(message: &str, cause: Layer) -> Layer {
    Layer { message: message.to_string(), source: Some(Box::new(cause)) }
}

/// Every error of the chain, the outermost first.
fn chain<'a>(e: &'a (dyn Error + 'static)) -> impl Iterator<Item = &'a (dyn Error + 'static)> + 'a {
    std::iter::successors(Some(e), |&e| e.source())
}

#[test]
fn the_chain_runs_from_the_outermost_layer_to_the_cause() {
    let base = Layer { message: "no such file".into(), source: None };
    let e = wrap("starting up", wrap("opening the log", base));
    let texts: Vec<String> = chain(&e).map(|c| c.to_string()).collect();
    assert_eq!(texts, ["starting up", "opening the log", "no such file"]);
}

#[test]
fn a_link_can_be_asked_what_it_is() {
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
    let e = Layer { message: "opening the log".into(), source: Some(Box::new(io)) };
    let kind = chain(&e).find_map(|c| c.downcast_ref::<std::io::Error>()).map(|c| c.kind());
    assert_eq!(kind, Some(std::io::ErrorKind::NotFound));
}
```

### Using it: context on a result, built only on failure

```rust test
use std::fmt;

#[derive(Debug)]
struct Failure {
    layers: Vec<String>, // outermost first
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.layers.join(": "))
    }
}

impl From<std::num::ParseIntError> for Failure {
    fn from(e: std::num::ParseIntError) -> Failure {
        Failure { layers: vec![e.to_string()] }
    }
}

trait Context<T> {
    fn with_context(self, message: impl FnOnce() -> String) -> Result<T, Failure>;
}

impl<T, E> Context<T> for Result<T, E>
where
    Failure: From<E>,
{
    fn with_context(self, message: impl FnOnce() -> String) -> Result<T, Failure> {
        self.map_err(|e| {
            let mut f = Failure::from(e);
            f.layers.insert(0, message());
            f
        })
    }
}

fn parse_id(text: &str) -> Result<i64, Failure> {
    text.parse::<i64>().with_context(|| format!("reading the id {text:?}"))
}

#[test]
fn the_message_names_what_was_being_done() {
    let e = parse_id("12x").unwrap_err();
    assert!(e.to_string().starts_with("reading the id \"12x\": "), "{e}");
}

#[test]
fn nothing_is_built_when_there_is_no_error() {
    let mut built = 0;
    let r: Result<i64, std::num::ParseIntError> = "7".parse::<i64>();
    let v = r.with_context(|| {
        built += 1;
        "unused".to_string()
    });
    assert_eq!(v.unwrap(), 7);
    assert_eq!(built, 0);
}
```

### In the exercises

- **3k-01 to 3k-05:** the whole module builds a `DbError`: keep the cause (01), convert every layer's error (02), add context and walk the chain (03), derive a code and a client-safe message (04), and check it across threads and random stacks of context (05).

### Where it is used

- **PostgreSQL:** `ereport` carries an SQLSTATE, a primary message, a detail and a hint; the server log gets all of it, the client gets what its protocol level allows.
- **Rust's `anyhow` and `eyre`:** the same chain and context, with a blanket `From` bought by not implementing `Error`.
- **std:** `Error::source` and `io::Error::new(kind, cause)` are the chain's two ends.

## Learn more

- [`std::error::Error`](https://doc.rust-lang.org/std/error/trait.Error.html) · [Rust API Guidelines: error types](https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err) · [PostgreSQL: error codes](https://www.postgresql.org/docs/current/errcodes-appendix.html) · [`anyhow`](https://docs.rs/anyhow)
