//! RECIPE 603 (interview): Error handling, iterators and closures: the everyday idioms interviewers probe.
//! LEVEL: intermediate
//! Use in: interview preparation, code review of error types, explaining lazy iterator pipelines and closure captures.   Run: cargo run --release --example rust_errors_iterators_closures_qa
//! COMMON USES: library error types, CLI/application error reporting, data pipelines, callbacks and event handlers.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233474457/rust-interview-questions-2025-ownership-borrowing-lifetimes-traits-async-await-error-handling-smart-pointers-concurrency/
//! Read: https://www.techinterview.org/post/3233460429/rust-interview-questions/ and https://codeforgeek.com/rust-interview-questions/
//! Read: https://github.com/devinterview-io/rust-interview-questions and https://flakm.github.io/posts/rust_interview_questions
//!
//! QUESTION BANK
//! Q1. Result vs Option vs panic: when do you use each? [Easy] [asked: very common]
//! Q2. What does the `?` operator desugar to, and how does `From` conversion fit in? [Medium] [asked: very common]
//! Q3. When do you use `Box<dyn Error + Send + Sync>`? [Medium] [asked: common]
//! Q4. How do you write a custom error enum with `std::error::Error` and `source()`? [Medium] [asked: very common]
//! Q5. thiserror vs anyhow: library vs application. What do they generate? [Medium] [asked: very common]
//! Q6. What is your `unwrap`/`expect` policy? [Easy] [asked: very common]
//! Q7. What do `catch_unwind` and `panic = "abort"` do? [Hard] [asked: occasional]
//! Q8. Iterators are lazy: what does that mean and how do you prove it? [Easy] [asked: very common]
//! Q9. `iter()` vs `iter_mut()` vs `into_iter()`; what does `for x in &v` desugar to? [Medium] [asked: very common]
//! Q10. Adapters vs consumers; `collect` into Vec/HashMap/String/Result; how does `sum` infer its type? [Medium] [asked: very common]
//! Q11. `map` vs `for_each` vs `filter_map` vs `flat_map`. [Easy] [asked: common]
//! Q12. Tour of `zip`, `chain`, `enumerate`, `windows`, `chunks`, `peekable`, `rev`, `step_by`. [Easy] [asked: common]
//! Q13. Implement a custom `Iterator` and `IntoIterator for &MyType`. [Medium] [asked: common]
//! Q14. Why is iterator invalidation impossible in safe Rust? [Medium] [asked: common]
//! Q15. Are iterators really zero-cost? Compare a pipeline with a manual loop. [Medium] [asked: common]
//! Q16. How do closures capture variables (by ref, by mut ref, by move)? [Medium] [asked: very common]
//! Q17. How do you return closures, and what is E0373? [Medium] [asked: common]

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;
use std::panic::{self, AssertUnwindSafe};
use std::time::Instant;

fn quiet<R>(f: impl FnOnce() -> R) -> R {
    let old = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let r = f();
    panic::set_hook(old);
    r
}

// ---------------------------------------------------------------------------
// Q1. Result vs Option vs panic
// ---------------------------------------------------------------------------
/// ANSWER: `Option<T>` = a value may be legitimately absent (lookup miss, empty iterator); no error
/// information needed. `Result<T, E>` = an operation can fail for reasons the CALLER may handle or report
/// (I/O, parsing, validation); E explains why. `panic!` = a BUG or broken invariant that the program
/// cannot sensibly continue from (index out of bounds, violated precondition, `unreachable!`); it
/// unwinds (or aborts) and is not for expected failures. Libraries return Result; they panic only on
/// contract violations they document. Convert with `ok_or`, `ok()`, `transpose`.
///
/// REALLY TESTING: do you treat errors as values and panics as bugs.
/// WRONG: "use panic for any error, it is simpler" / "never panic".
/// FOLLOW-UP: `Option<Result<T,E>>` vs `Result<Option<T>,E>`: `transpose` swaps them.
fn find_user(id: u32) -> Option<&'static str> {
    [(1, "ann"), (2, "bob")]
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, n)| *n)
}
fn parse_age(s: &str) -> Result<u8, ParseIntError> {
    s.trim().parse::<u8>()
}
fn q1_result_option_panic() {
    assert_eq!(find_user(1), Some("ann"));
    assert_eq!(find_user(9), None); // absence is normal: Option
    assert_eq!(parse_age(" 42 "), Ok(42));
    assert!(parse_age("x").is_err()); // failure with a reason: Result
    assert_eq!(
        parse_age("300").unwrap_err().to_string(),
        "number too large to fit in target type"
    );
    // Conversions between them:
    assert_eq!(find_user(9).ok_or("missing"), Err("missing"));
    assert_eq!(Ok::<u8, ()>(1).ok(), Some(1));
    let o: Option<Result<u8, ParseIntError>> = Some(parse_age("7"));
    assert_eq!(o.transpose(), Ok(Some(7)));
    // A contract violation is a panic (a bug in the caller):
    let r = quiet(|| panic::catch_unwind(|| vec![1, 2, 3][std::hint::black_box(7)]));
    assert!(r.is_err());
}

// ---------------------------------------------------------------------------
// Q2. the ? operator
// ---------------------------------------------------------------------------
/// ANSWER: `expr?` on a Result is roughly
/// `match expr { Ok(v) => v, Err(e) => return Err(From::from(e)) }`. It (1) unwraps Ok, (2) early-returns
/// on Err after converting the error with `From::from`, so the function's error type may differ as long as
/// `From<E1> for E2` exists. It also works on `Option` (returns None) inside functions returning Option,
/// but you cannot mix the two without `ok_or`/`ok()`. The function must return `Result`/`Option`
/// (or implement `Try`); in `main` you may return `Result<(), E: Debug>`.
///
/// REALLY TESTING: the From conversion hidden in `?`, and the error E0277 when it is missing.
/// WRONG: "? just unwraps" / "? is exception handling".
/// FOLLOW-UP: what is the error if `From` is missing? E0277 "`?` couldn't convert the error to `X`".
#[derive(Debug, PartialEq)]
enum AppError {
    Parse(String),
    Negative(i64),
}
impl From<ParseIntError> for AppError {
    fn from(e: ParseIntError) -> Self {
        AppError::Parse(e.to_string())
    }
}
fn parse_positive(s: &str) -> Result<i64, AppError> {
    let n: i64 = s.parse()?; // ParseIntError -> AppError through From
    if n < 0 {
        return Err(AppError::Negative(n));
    }
    Ok(n)
}
// What `?` expands to (manually):
fn parse_positive_desugared(s: &str) -> Result<i64, AppError> {
    let n: i64 = match s.parse::<i64>() {
        Ok(v) => v,
        Err(e) => return Err(From::from(e)),
    };
    if n < 0 {
        Err(AppError::Negative(n))
    } else {
        Ok(n)
    }
}
fn first_char_upper(s: &str) -> Option<char> {
    let c = s.chars().next()?; // ? on Option inside Option fn
    Some(c.to_ascii_uppercase())
}
fn q2_question_mark() {
    for input in ["12", "-3", "zz"] {
        assert_eq!(parse_positive(input), parse_positive_desugared(input));
    }
    assert_eq!(parse_positive("5"), Ok(5));
    assert_eq!(parse_positive("-3"), Err(AppError::Negative(-3)));
    assert!(matches!(parse_positive("x"), Err(AppError::Parse(_))));
    assert_eq!(first_char_upper("rust"), Some('R'));
    assert_eq!(first_char_upper(""), None);
    // FAILING (E0277): `?` on a ParseIntError in a fn returning Result<_, String> without `From<ParseIntError> for String`
    //   fn bad(s: &str) -> Result<i32, String> { Ok(s.parse::<i32>()?) }
    // FIXED: map_err, or define the From impl.
    fn good(s: &str) -> Result<i32, String> {
        s.parse::<i32>().map_err(|e| e.to_string())
    }
    assert!(good("q").is_err());
    // FAILING (E0277): using `?` on Option inside a Result function: fn f() -> Result<(), E> { opt? }   -> use ok_or(E)?
}

// ---------------------------------------------------------------------------
// Q3. Box<dyn Error + Send + Sync>
// ---------------------------------------------------------------------------
/// ANSWER: `Box<dyn Error>` is a type-erased error: any `E: Error` converts into it via `From`, so `?` works
/// on mixed error types with no custom enum, and `&str`/`String` convert too (`"msg".into()`). Add
/// `+ Send + Sync + 'static` when the error crosses threads/tasks (`thread::spawn` results, tokio tasks),
/// and so it can be converted to `anyhow::Error`. Cost: callers cannot `match` on variants, only
/// `downcast_ref::<T>()` or print it. Good for binaries/prototypes/tests; libraries should expose concrete types.
///
/// REALLY TESTING: erased vs typed errors, and the Send + Sync part.
/// WRONG: "Box<dyn Error> is always fine for libraries".
/// FOLLOW-UP: why can `Box<dyn Error>` not itself implement `Error`? It would overlap with the `From<E> for Box<dyn Error>` blanket impl.
type BoxErr = Box<dyn Error + Send + Sync + 'static>;
fn mixed(s: &str) -> Result<usize, BoxErr> {
    let n: usize = s.parse()?; // ParseIntError
    if n == 0 {
        return Err("zero is not allowed".into()); // &str -> Box<dyn Error>
    }
    let v = std::str::from_utf8(std::hint::black_box(&[0xff]))?; // Utf8Error for any non-zero number
    Ok(v.len() + n)
}
fn q3_box_dyn_error() {
    assert_eq!(mixed("0").unwrap_err().to_string(), "zero is not allowed");
    let e = mixed("x").unwrap_err();
    assert!(
        e.downcast_ref::<ParseIntError>().is_some(),
        "typed info is recoverable by downcast"
    );
    let e2 = mixed("5").unwrap_err();
    assert!(e2.downcast_ref::<std::str::Utf8Error>().is_some());
    // Send + Sync lets the error cross a thread boundary:
    let from_thread = std::thread::spawn(|| mixed(""))
        .join()
        .unwrap()
        .unwrap_err();
    assert!(from_thread.to_string().contains("empty"));
    // Without Send + Sync it would not compile in spawn (E0277: `dyn Error` cannot be sent between threads safely).
}

// ---------------------------------------------------------------------------
// Q4. custom error enum with source()
// ---------------------------------------------------------------------------
/// ANSWER: Implement `Display` (human message, lower-case, no trailing period, does NOT repeat the
/// source), `Debug` (derive) and `Error` (usually empty, but override `source()` to expose the underlying cause
/// so reporters can print the chain "caused by ..."). Implement `From<Inner>` for each wrapped error so `?`
/// converts. Make it an enum with one variant per failure KIND callers might match on; mark it
/// `#[non_exhaustive]` in public libraries so adding variants is not a breaking change.
///
/// REALLY TESTING: the Error trait contract (source chain) vs stuffing text into strings.
/// WRONG: "`Display` should include the source error's text" (the reporter walks the chain; otherwise it is duplicated).
/// FOLLOW-UP: how do you print the full chain? Loop `while let Some(s) = err.source()`.
#[derive(Debug)]
enum ConfigError {
    Missing(String),
    BadNumber { key: String, source: ParseIntError },
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "missing key `{k}`"),
            ConfigError::BadNumber { key, .. } => write!(f, "key `{key}` is not a number"),
        }
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::BadNumber { source, .. } => Some(source),
            ConfigError::Missing(_) => None,
        }
    }
}
fn read_port(map: &HashMap<&str, &str>) -> Result<u16, ConfigError> {
    let raw = map
        .get("port")
        .ok_or_else(|| ConfigError::Missing("port".into()))?;
    raw.parse().map_err(|source| ConfigError::BadNumber {
        key: "port".into(),
        source,
    })
}
fn chain(e: &dyn Error) -> Vec<String> {
    let mut out = vec![e.to_string()];
    let mut cur = e.source();
    while let Some(s) = cur {
        out.push(s.to_string());
        cur = s.source();
    }
    out
}
fn q4_custom_error() {
    let mut m = HashMap::new();
    assert!(matches!(read_port(&m), Err(ConfigError::Missing(_))));
    m.insert("port", "80x");
    let err = read_port(&m).unwrap_err();
    assert_eq!(
        chain(&err),
        [
            "key `port` is not a number",
            "invalid digit found in string"
        ]
    );
    m.insert("port", "8080");
    assert_eq!(read_port(&m).unwrap(), 8080);
    let boxed: BoxErr = Box::new(err);
    assert!(boxed.source().is_some());
}

// ---------------------------------------------------------------------------
// Q5. thiserror vs anyhow
// ---------------------------------------------------------------------------
/// ANSWER: `thiserror` is for LIBRARIES: a derive macro that writes exactly what Q4 wrote by hand
/// (`Display` from `#[error("...")]`, `From` from `#[from]`, `source()` from `#[source]`), keeping a
/// concrete enum callers can match. `anyhow` is for APPLICATIONS: one opaque `anyhow::Error` (a boxed
/// `dyn Error + Send + Sync` with a backtrace) so `?` works on anything, plus `.context("while doing X")`
/// to add a layer to the chain, `bail!`, `ensure!`. Libraries: precise types; binaries: convenience and
/// good error reports. They compose: libraries use thiserror, the app wraps them in anyhow. Below,
/// the hand-rolled equivalents of what each crate generates (the crates are deliberately not used here).
///
/// REALLY TESTING: do you choose by audience (callers that match vs a human that reads).
/// WRONG: "anyhow is a replacement for thiserror" or "use anyhow in a public library API".
/// FOLLOW-UP: what does `.context()` do to `source()`? The new error wraps the old one as its source.
///
/// thiserror would write:   #[derive(Error, Debug)] enum E { #[error("io: {0}")] Io(#[from] io::Error) }
/// anyhow would write:      fn run() -> anyhow::Result<()> { read().context("loading config")?; Ok(()) }
#[derive(Debug)]
struct Context {
    msg: String,
    source: BoxErr,
}
impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}
impl Error for Context {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}
trait ResultExt<T> {
    fn context(self, msg: &str) -> Result<T, BoxErr>;
}
impl<T, E: Error + Send + Sync + 'static> ResultExt<T> for Result<T, E> {
    fn context(self, msg: &str) -> Result<T, BoxErr> {
        self.map_err(|e| {
            Box::new(Context {
                msg: msg.to_string(),
                source: Box::new(e),
            }) as BoxErr
        })
    }
}
fn load(raw: &str) -> Result<u16, BoxErr> {
    let n: u16 = raw.parse().context("parsing the port")?;
    if n < 1024 {
        return Err(format!("port {n} is privileged").into()); // like anyhow::bail!
    }
    Ok(n)
}
fn q5_thiserror_vs_anyhow() {
    assert_eq!(load("8080").unwrap(), 8080);
    let e = load("nope").unwrap_err();
    assert_eq!(
        chain(&*e),
        ["parsing the port", "invalid digit found in string"]
    );
    assert_eq!(load("80").unwrap_err().to_string(), "port 80 is privileged");
}

// ---------------------------------------------------------------------------
// Q6. unwrap / expect policy
// ---------------------------------------------------------------------------
/// ANSWER: `unwrap()` panics on None/Err without context; `expect("why this cannot fail")` panics with
/// YOUR message and is the documented way to assert an invariant ("regex literal is valid"). Policy: in
/// production paths propagate with `?`; allow `unwrap`/`expect` in tests, examples, `main` setup of constants,
/// and where the failure is provably impossible (comment it); use `unwrap_or`, `unwrap_or_else`,
/// `unwrap_or_default`, `map_or`, `if let`/`let else` for recoverable cases. Many teams enforce
/// `clippy::unwrap_used` outside tests.
///
/// REALLY TESTING: judgement about robustness.
/// WRONG: "unwrap is fine, it crashes anyway" / "expect is just a nicer unwrap" (it documents an invariant).
/// FOLLOW-UP: `unwrap_or(expensive())` vs `unwrap_or_else(|| expensive())` (eager vs lazy).
fn q6_unwrap_policy() {
    use std::cell::Cell;
    let evals = Cell::new(0);
    let expensive = || {
        evals.set(evals.get() + 1);
        7
    };
    assert_eq!(Some(1).unwrap_or(expensive()), 1);
    assert_eq!(evals.get(), 1, "unwrap_or evaluates its argument eagerly");
    assert_eq!(Some(1).unwrap_or_else(expensive), 1);
    assert_eq!(evals.get(), 1, "unwrap_or_else is lazy");
    assert_eq!("x".parse::<i32>().unwrap_or_default(), 0);
    let msg = quiet(|| panic::catch_unwind(|| None::<i32>.expect("config must have a port")))
        .unwrap_err()
        .downcast::<String>() // expect() formats its message, so the payload is a String
        .ok()
        .map(|b| *b);
    assert_eq!(msg.as_deref(), Some("config must have a port"));
    let Some(v) = find_user(1) else {
        panic!("unreachable in this demo")
    }; // let-else: handle the miss explicitly
    assert_eq!(v, "ann");
}

// ---------------------------------------------------------------------------
// Q7. catch_unwind / panic = abort
// ---------------------------------------------------------------------------
/// ANSWER: By default a panic UNWINDS the stack running destructors; `catch_unwind` stops the unwinding at a
/// boundary and returns `Err(Box<dyn Any + Send>)` (the payload). It is NOT try/catch for control flow: use
/// it at FFI boundaries (unwinding across `extern "C"` is UB), thread/task pools and test harnesses.
/// `panic = "abort"` in Cargo profiles aborts immediately: smaller/faster binaries, but no destructors and
/// `catch_unwind` cannot catch (and `JoinHandle::join` will not see Err). A panicking thread does not kill
/// the process in unwind mode; `Mutex` becomes poisoned. `AssertUnwindSafe` is a promise you accept
/// possibly-broken invariants.
///
/// REALLY TESTING: unwind vs abort trade-offs and where catching is legitimate.
/// WRONG: "catch_unwind is how Rust does exceptions".
/// FOLLOW-UP: what about a panic in a destructor during unwinding? Double panic aborts.
fn q7_catch_unwind() {
    struct Guard<'a>(&'a std::cell::Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let cleaned = std::cell::Cell::new(false);
    let r = quiet(|| {
        panic::catch_unwind(AssertUnwindSafe(|| {
            let _g = Guard(&cleaned);
            let v: Vec<i32> = Vec::new();
            v[std::hint::black_box(0)]
        }))
    });
    assert!(r.is_err());
    assert!(
        cleaned.get(),
        "unwinding ran the destructor (with panic=abort it would not)"
    );
    // A panicking thread is isolated: join returns Err, the process lives on.
    let joined = quiet(|| std::thread::spawn(|| panic!("worker died")).join());
    assert!(joined.is_err());
    // Cargo.toml: [profile.release] panic = "abort"   (not applicable to tests/benches; they always unwind)
}

// ---------------------------------------------------------------------------
// Q8. laziness
// ---------------------------------------------------------------------------
/// ANSWER: Iterator ADAPTERS (`map`, `filter`, `take`, `zip`...) only build a new iterator struct; NO element is
/// processed until a CONSUMER (`next`, `collect`, `sum`, `for`, `count`, `fold`...) pulls. Evaluation is
/// element-by-element (pull model), so `take(2)` after `map` runs the closure only twice, and infinite
/// iterators (`0..`) are fine. Unused adapters trigger the `#[must_use]` warning "iterators are lazy".
///
/// REALLY TESTING: pull-based evaluation, side effects, short-circuiting.
/// WRONG: "map runs immediately and stores a Vec" (that is Python 2 / eager languages).
/// FOLLOW-UP: in what order do side effects of `map` then `filter` run? Interleaved per element.
fn q8_laziness() {
    use std::cell::Cell;
    let calls = Cell::new(0);
    let pipeline = (1..=1_000_000).map(|x| {
        calls.set(calls.get() + 1);
        x * 2
    });
    assert_eq!(calls.get(), 0, "building the adapter did no work");
    let first_two: Vec<i32> = pipeline.take(2).collect();
    assert_eq!(first_two, [2, 4]);
    assert_eq!(calls.get(), 2, "only the pulled elements were computed");

    // Interleaving: element by element, not stage by stage.
    let log = std::cell::RefCell::new(Vec::new());
    let out: Vec<i32> = (1..=3)
        .map(|x| {
            log.borrow_mut().push(format!("map{x}"));
            x
        })
        .filter(|x| {
            log.borrow_mut().push(format!("filter{x}"));
            x % 2 == 1
        })
        .collect();
    assert_eq!(out, [1, 3]);
    assert_eq!(
        *log.borrow(),
        ["map1", "filter1", "map2", "filter2", "map3", "filter3"]
    );
    // Infinite iterator + short-circuit consumer:
    assert_eq!((0..).map(|x| x * x).find(|x| *x > 50), Some(64));
    // Unused adapter does nothing (and the compiler warns: unused `Map` that must be used):
    let touched = Cell::new(false);
    let _unused = (0..3).map(|_| touched.set(true));
    assert!(!touched.get());
}

// ---------------------------------------------------------------------------
// Q9. iter / iter_mut / into_iter
// ---------------------------------------------------------------------------
/// ANSWER: `iter()` yields `&T` (borrow), `iter_mut()` yields `&mut T`, `into_iter()` on the collection yields `T`
/// and CONSUMES it. `for x in v` calls `IntoIterator::into_iter(v)` (moves); `for x in &v` calls
/// `into_iter(&v)` == `v.iter()` (shared refs); `for x in &mut v` == `v.iter_mut()`. A `for` loop desugars to
/// `let mut it = IntoIterator::into_iter(expr); while let Some(x) = it.next() { body }`. Note that
/// `.into_iter()` on an ARRAY `[T; N]` yields values since edition 2021, and on a `&Vec` yields refs.
///
/// REALLY TESTING: ownership at the loop boundary.
/// WRONG: "iter() and into_iter() are the same".
/// FOLLOW-UP: why does using `v` after `for x in v` fail? Moved into the loop (E0382).
fn q9_iter_forms() {
    let mut v = vec![String::from("a"), String::from("b")];
    let lens: Vec<usize> = v.iter().map(|s| s.len()).collect(); // &String
    assert_eq!(lens, [1, 1]);
    for s in v.iter_mut() {
        s.push('!'); // &mut String
    }
    for s in &mut v {
        s.push('?');
    }
    assert_eq!(v, ["a!?", "b!?"]);
    let mut manual = Vec::new();
    {
        let mut it = IntoIterator::into_iter(&v); // what `for s in &v` does
        while let Some(s) = it.next() {
            manual.push(s.len());
        }
    }
    assert_eq!(manual, [3, 3]);
    let owned: Vec<String> = v.into_iter().collect(); // consumes v
    assert_eq!(owned.len(), 2);
    // println!("{:?}", v);   // error[E0382]: borrow of moved value: `v`
    let arr = [1, 2, 3];
    let by_value: Vec<i32> = arr.into_iter().collect(); // values (edition 2021+)
    let by_ref: Vec<&i32> = (&arr).into_iter().collect();
    assert_eq!((by_value.len(), *by_ref[0]), (3, 1));
}

// ---------------------------------------------------------------------------
// Q10. adapters, consumers, collect, sum
// ---------------------------------------------------------------------------
/// ANSWER: Adapters transform an iterator into another iterator (lazy); consumers drive it and produce a result.
/// `collect::<C>()` works for any `C: FromIterator`: `Vec`, `HashMap` (from pairs), `HashSet`, `String` (from chars/&str),
/// `Result<Vec<T>, E>` (stops at the FIRST error), `Option<Vec<T>>`. The target type decides, so you must
/// annotate (turbofish or let type). `sum`/`product` are generic over the output (`Sum` trait), so
/// `let t: i32 = it.sum();` or `sum::<i32>()` is required (E0283 otherwise).
///
/// REALLY TESTING: `FromIterator` and type inference.
/// WRONG: "collect always makes a Vec".
/// FOLLOW-UP: how do you collect Results without stopping at the first error? `partition`/`filter_map(Result::ok)`.
fn q10_collect_sum() {
    let v: Vec<i32> = (1..=4).collect();
    let m: HashMap<i32, i32> = v.iter().map(|&x| (x, x * x)).collect();
    assert_eq!(m[&3], 9);
    let hs: HashSet<i32> = [1, 1, 2].into_iter().collect();
    assert_eq!(hs.len(), 2);
    let s: String = ['a', 'b'].iter().collect();
    let s2: String = vec!["x", "y"].into_iter().collect();
    assert_eq!((s.as_str(), s2.as_str()), ("ab", "xy"));
    let ok: Result<Vec<i32>, ParseIntError> = "1,2,3".split(',').map(str::parse).collect();
    assert_eq!(ok.unwrap(), [1, 2, 3]);
    let bad: Result<Vec<i32>, ParseIntError> = "1,x,3".split(',').map(str::parse).collect();
    assert!(bad.is_err(), "short-circuits at the first Err");
    let opt: Option<Vec<u32>> = ["1", "2"].iter().map(|s| s.parse().ok()).collect();
    assert_eq!(opt, Some(vec![1, 2]));
    let total: i32 = v.iter().sum();
    let prod = v.iter().product::<i32>();
    let fsum: f64 = [0.5, 0.25].iter().sum();
    assert_eq!((total, prod, fsum), (10, 24, 0.75));
    // let t = v.iter().sum();   // error[E0283]: type annotations needed
    let (evens, odds): (Vec<i32>, Vec<i32>) = v.iter().partition(|x| **x % 2 == 0);
    assert_eq!((evens, odds), (vec![2, 4], vec![1, 3]));
    assert_eq!(v.iter().copied().reduce(|a, b| a.max(b)), Some(4));
    assert_eq!(
        v.iter().fold(String::new(), |acc, x| acc + &x.to_string()),
        "1234"
    );
}

// ---------------------------------------------------------------------------
// Q11. map / for_each / filter_map / flat_map
// ---------------------------------------------------------------------------
/// ANSWER: `map` transforms each item 1:1 and is lazy (returns an iterator). `for_each` is a CONSUMER run purely
/// for side effects (returns `()`); a plain `for` loop is usually clearer and allows `break`/`?`. `filter_map`
/// maps to `Option` and keeps the `Some`s: one pass replacing `.map(..).filter(..).map(unwrap)`. `flat_map`
/// maps each item to an iterable and flattens (1:N), also `flatten` for nested iterables/Options.
///
/// REALLY TESTING: choosing the right combinator and knowing which are lazy.
/// WRONG: "for_each is the lazy map" / using map for side effects (never runs unless consumed).
/// FOLLOW-UP: `Option` is an iterator of 0/1 items, hence `flatten` on `Vec<Option<T>>` drops Nones.
fn q11_map_family() {
    let words = ["1", "x", "3"];
    let nums: Vec<i32> = words.iter().filter_map(|w| w.parse().ok()).collect();
    assert_eq!(nums, [1, 3]);
    let chars: Vec<char> = ["ab", "c"].iter().flat_map(|s| s.chars()).collect();
    assert_eq!(chars, ['a', 'b', 'c']);
    let nested = vec![vec![1], vec![], vec![2, 3]];
    assert_eq!(nested.iter().flatten().count(), 3);
    assert_eq!(
        vec![Some(1), None, Some(3)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        [1, 3]
    );
    let mut seen = Vec::new();
    (1..=3).for_each(|x| seen.push(x * 10));
    assert_eq!(seen, [10, 20, 30]);
    // map with side effects but no consumer: nothing happens
    let mut side = 0;
    let _ = (0..3).map(|_| side += 1);
    assert_eq!(side, 0);
}

// ---------------------------------------------------------------------------
// Q12. adapter tour
// ---------------------------------------------------------------------------
/// ANSWER: `zip` pairs two iterators (stops at the shorter); `chain` concatenates; `enumerate` adds an index;
/// `windows(n)` (slices only) yields overlapping `&[T]` views; `chunks(n)` yields non-overlapping pieces
/// (last may be shorter; `chunks_exact` drops the remainder); `peekable` lets you look at the next item
/// without consuming (`peek`, `next_if`) which is the core of hand-written parsers; `rev` needs a
/// `DoubleEndedIterator`; `step_by(n)` takes every nth; also `skip`, `take`, `skip_while`, `take_while`,
/// `scan`, `last`, `min_by_key`, `unzip`, `cycle`.
///
/// REALLY TESTING: fluency; windows vs chunks is the usual trap.
/// WRONG: "windows and chunks are the same" or "rev works on any iterator".
/// FOLLOW-UP: why does `chars().rev()` work but `chars().len()` does not? (`Chars` is double-ended, not ExactSize.)
fn q12_adapter_tour() {
    let a = [1, 2, 3, 4, 5];
    let z: Vec<(i32, char)> = a.iter().copied().zip("abc".chars()).collect();
    assert_eq!(z, [(1, 'a'), (2, 'b'), (3, 'c')]); // shorter wins
    assert_eq!(a.iter().chain([9].iter()).count(), 6);
    assert_eq!(
        a.iter().enumerate().map(|(i, x)| i as i32 * x).sum::<i32>(),
        40
    );
    assert_eq!(
        a.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>(),
        [1, 1, 1, 1]
    );
    assert_eq!(a.chunks(2).map(|c| c.len()).collect::<Vec<_>>(), [2, 2, 1]);
    assert_eq!(a.chunks_exact(2).count(), 2);
    assert_eq!(a.iter().rev().take(2).collect::<Vec<_>>(), [&5, &4]);
    assert_eq!((0..10).step_by(4).collect::<Vec<_>>(), [0, 4, 8]);
    assert_eq!("héllo".chars().rev().collect::<String>(), "olléh");
    // peekable: tokenise digits without consuming the delimiter
    let mut it = "12+34".chars().peekable();
    let mut tokens = Vec::new();
    while let Some(&c) = it.peek() {
        if c.is_ascii_digit() {
            let mut n = String::new();
            while let Some(d) = it.next_if(|d| d.is_ascii_digit()) {
                n.push(d);
            }
            tokens.push(n);
        } else {
            tokens.push(c.to_string());
            it.next();
        }
    }
    assert_eq!(tokens, ["12", "+", "34"]);
    let (xs, ys): (Vec<i32>, Vec<i32>) = [(1, 2), (3, 4)].into_iter().unzip();
    assert_eq!((xs, ys), (vec![1, 3], vec![2, 4]));
    assert_eq!(
        (1..=5)
            .scan(0, |acc, x| {
                *acc += x;
                Some(*acc)
            })
            .collect::<Vec<_>>(),
        [1, 3, 6, 10, 15]
    );
    assert_eq!([3, 1, 2].iter().max_by_key(|x| **x), Some(&3));
}

// ---------------------------------------------------------------------------
// Q13. custom iterator, IntoIterator for &T
// ---------------------------------------------------------------------------
/// ANSWER: Implement `Iterator` with one method, `next(&mut self) -> Option<Self::Item>`; every adapter and
/// consumer comes for free. For a collection type, implement `IntoIterator` three times so `for` works on all
/// forms: for `MyType` (owning), `&MyType` (shared) and `&mut MyType`; the reference versions return a
/// borrowing iterator struct with a lifetime (or delegate to the inner `slice::Iter`). Implement
/// `FromIterator` to support `collect`, `Extend` for `extend`, and `DoubleEndedIterator`/`ExactSizeIterator`
/// when applicable.
///
/// REALLY TESTING: the trait plumbing behind `for`.
/// WRONG: "implement Iterator on the collection itself" (that makes the collection its own cursor and consumable).
/// FOLLOW-UP: why a separate iterator struct? So several independent iterations can be active at once.
struct Fib {
    a: u64,
    b: u64,
}
impl Iterator for Fib {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let r = self.a;
        self.a = self.b;
        self.b += r;
        Some(r)
    }
}
struct Playlist {
    songs: Vec<String>,
}
impl<'a> IntoIterator for &'a Playlist {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.songs.iter()
    }
}
impl IntoIterator for Playlist {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;
    fn into_iter(self) -> Self::IntoIter {
        self.songs.into_iter()
    }
}
impl FromIterator<String> for Playlist {
    fn from_iter<I: IntoIterator<Item = String>>(it: I) -> Self {
        Playlist {
            songs: it.into_iter().collect(),
        }
    }
}
fn q13_custom_iterator() {
    let evens: Vec<u64> = Fib { a: 0, b: 1 }.filter(|x| x % 2 == 0).take(4).collect();
    assert_eq!(evens, [0, 2, 8, 34]);
    let list: Playlist = ["a", "bb", "ccc"].iter().map(|s| s.to_string()).collect(); // FromIterator
    let mut total = 0;
    for s in &list {
        total += s.len(); // borrows: list is still usable
    }
    assert_eq!(total, 6);
    assert_eq!((&list).into_iter().count(), 3);
    let joined: Vec<String> = list.into_iter().map(|s| s.to_uppercase()).collect(); // owning
    assert_eq!(joined, ["A", "BB", "CCC"]);
}

// ---------------------------------------------------------------------------
// Q14. iterator invalidation
// ---------------------------------------------------------------------------
/// ANSWER: An iterator over a collection holds a borrow of it (`slice::Iter<'a, T>` contains `&'a [T]`) for as
/// long as the iterator lives. Mutating the collection needs `&mut`, which the borrow rules forbid
/// while any shared borrow is live (E0502). So the C++ bug class (push during range-for reallocates and dangles) is
/// rejected at compile time with no runtime cost. Safe alternatives: collect-then-apply, `retain`, `drain`,
/// indices, `std::mem::take` the Vec and rebuild.
///
/// REALLY TESTING: whether you connect the borrow checker to a concrete C++ bug.
/// WRONG: "Rust checks at runtime like Java's ConcurrentModificationException".
/// FOLLOW-UP: what about `RefCell<Vec<_>>`? The check returns at runtime as a panic (BorrowMutError).
fn q14_invalidation() {
    let mut v = vec![1, 2, 3];
    // FAILING (E0502): for x in &v { if *x == 2 { v.push(4); } }
    //   error[E0502]: cannot borrow `v` as mutable because it is also borrowed as immutable
    let extra: Vec<i32> = v.iter().filter(|x| **x == 2).map(|x| x + 2).collect();
    v.extend(extra); // FIXED: the iterator is dead before the mutation
    assert_eq!(v, [1, 2, 3, 4]);
    let drained: Vec<i32> = v.drain(1..3).collect();
    assert_eq!((drained, v.clone()), (vec![2, 3], vec![1, 4]));
    // The same idea at runtime with RefCell: the check moves to a panic.
    let cell = std::cell::RefCell::new(vec![1, 2]);
    let r = quiet(|| {
        panic::catch_unwind(AssertUnwindSafe(|| {
            for _x in cell.borrow().iter() {
                cell.borrow_mut().push(9);
            }
        }))
    });
    assert!(r.is_err());
}

// ---------------------------------------------------------------------------
// Q15. zero-cost abstraction
// ---------------------------------------------------------------------------
/// ANSWER: "Zero-cost" (Stroustrup): you do not pay for what you do not use, and what you use you could
/// not hand-code better. Iterator chains are monomorphised and inlined into a single loop (often
/// auto-vectorised) with no allocation and no virtual calls, so a `filter/map/sum` pipeline compiles to
/// essentially the same machine code as a manual loop; bounds checks can even disappear. It is a property
/// of OPTIMISED builds (`--release`); in debug builds adapters are real function calls and can be 10x slower.
/// Caveats: `Box<dyn Iterator>` reintroduces dynamic dispatch, and `collect` into a temporary Vec allocates.
///
/// REALLY TESTING: does the candidate know the claim and its limits.
/// WRONG: "iterators are slower than for loops".
/// FOLLOW-UP: how to verify? Benchmarks (criterion), `cargo asm`/Godbolt, comparing results first.
fn q15_zero_cost() {
    let data: Vec<u64> = (0..2_000_000).collect();
    let t0 = Instant::now();
    let pipeline: u64 = data.iter().filter(|x| **x % 3 == 0).map(|x| x * 2).sum();
    let t_iter = t0.elapsed();
    let t1 = Instant::now();
    let mut manual = 0u64;
    for i in 0..data.len() {
        let x = data[i];
        if x % 3 == 0 {
            manual += x * 2;
        }
    }
    let t_loop = t1.elapsed();
    assert_eq!(pipeline, manual, "identical result");
    println!(
        "  [info] iterator pipeline {t_iter:?} vs manual loop {t_loop:?} (release mode; printed, not asserted)"
    );
    // No allocation difference either: both only read `data`. size of the adapter chain is just its captured state:
    let chain = data.iter().filter(|x| **x % 3 == 0).map(|x| x * 2);
    assert!(std::mem::size_of_val(&chain) <= 16);
}

// ---------------------------------------------------------------------------
// Q16. closure captures
// ---------------------------------------------------------------------------
/// ANSWER: A closure is an anonymous struct holding its captures. Rust captures each variable in the LEAST
/// restrictive way the body needs: by shared reference if it only reads, by `&mut` if it mutates, by value
/// if it moves/consumes it. `move` forces by-value capture of everything (Copy types are copied), needed when
/// the closure must outlive the scope (threads, returned closures). A `move` closure that only reads is still `Fn`.
/// Since edition 2021 closures capture disjoint FIELDS, not whole structs.
///
/// REALLY TESTING: the connection between capture mode, borrowing and the Fn traits.
/// WRONG: "closures always borrow" / "move means the closure is FnOnce".
/// FOLLOW-UP: after `move`, is the original usable? Only if the captured value is Copy.
fn q16_captures() {
    let name = String::from("n");
    let by_ref = || name.len();
    assert_eq!(by_ref() + by_ref(), 2);
    assert_eq!(name, "n"); // still readable: only shared borrowed

    let mut hits = 0;
    let mut by_mut = || hits += 1; // exclusive borrow for the closure's lifetime
    by_mut();
    by_mut();
    // println!("{hits}");   // error[E0502] here: `hits` is still mutably borrowed by `by_mut` (used below)
    by_mut();
    assert_eq!(hits, 3); // NLL: the closure is dead now

    let n = 5; // Copy: move copies it
    let by_move = move || n + 1;
    assert_eq!((by_move(), n), (6, 5));

    let s = String::from("moved");
    let consume = move || s; // by value; s unusable afterwards
    // println!("{s}");   // error[E0382]: borrow of moved value: `s`
    assert_eq!(consume(), "moved");

    // Disjoint field capture (2021): borrowing one field while the closure holds another is fine.
    struct S {
        a: String,
        b: Vec<i32>,
    }
    let mut st = S {
        a: "a".into(),
        b: vec![],
    };
    let mut push_b = |x| st.b.push(x);
    let a_len = st.a.len(); // allowed: closure captured only `st.b`
    push_b(1);
    assert_eq!((a_len, st.b.len()), (1, 1));
}

// ---------------------------------------------------------------------------
// Q17. returning closures / E0373
// ---------------------------------------------------------------------------
/// ANSWER: A closure that BORROWS a local cannot be returned or sent to a thread because the local dies first:
/// error E0373 "closure may outlive the current function, but it borrows `x`"; the fix is `move`. A single closure
/// is returned as `impl Fn(..)`; different closures from different branches need `Box<dyn Fn(..)>` (each closure has
/// its own unnameable type). A function that takes a callback should accept `impl Fn..`/generic `F` (static dispatch)
/// unless it needs to STORE heterogeneous callbacks (then `Box<dyn Fn>`).
///
/// REALLY TESTING: lifetimes + closures + dispatch together.
/// WRONG: "add `'static` to fix it" (the data is local; move/own it).
/// FOLLOW-UP: can you return a closure that mutates captured state? `impl FnMut`, call needs `mut` binding.
fn make_counter(start: u32) -> impl FnMut() -> u32 {
    let mut n = start;
    move || {
        n += 1;
        n
    }
}
fn make_op(kind: u8) -> Box<dyn Fn(i32) -> i32> {
    let k = 10;
    match kind {
        0 => Box::new(move |x| x + k),
        _ => Box::new(move |x| x * k),
    }
}
fn q17_returning_closures() {
    // FAILING (E0373):
    //   fn bad(x: i32) -> impl Fn(i32) -> i32 { |y| x + y }
    //   error[E0373]: closure may outlive the current function, but it borrows `x`, which is owned by the current function
    //   help: to force the closure to take ownership of `x`, use the `move` keyword:  move |y| x + y
    let mut c = make_counter(10);
    assert_eq!((c(), c()), (11, 12));
    let mut other = make_counter(0); // independent state per closure
    assert_eq!(other(), 1);
    assert_eq!(make_op(0)(1), 11);
    assert_eq!(make_op(1)(2), 20);
    // Higher-order function with static dispatch:
    fn twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
        f(f(x))
    }
    assert_eq!(twice(|x| x + 3, 0), 6);
    // Scoped threads allow borrowing closures without `move`:
    let data = vec![1, 2, 3];
    let total = std::thread::scope(|s| s.spawn(|| data.iter().sum::<i32>()).join().unwrap());
    assert_eq!(total, 6);
}

fn main() {
    q1_result_option_panic();
    println!("Q1: ok");
    q2_question_mark();
    println!("Q2: ok");
    q3_box_dyn_error();
    println!("Q3: ok");
    q4_custom_error();
    println!("Q4: ok");
    q5_thiserror_vs_anyhow();
    println!("Q5: ok");
    q6_unwrap_policy();
    println!("Q6: ok");
    q7_catch_unwind();
    println!("Q7: ok");
    q8_laziness();
    println!("Q8: ok");
    q9_iter_forms();
    println!("Q9: ok");
    q10_collect_sum();
    println!("Q10: ok");
    q11_map_family();
    println!("Q11: ok");
    q12_adapter_tour();
    println!("Q12: ok");
    q13_custom_iterator();
    println!("Q13: ok");
    q14_invalidation();
    println!("Q14: ok");
    q15_zero_cost();
    println!("Q15: ok");
    q16_captures();
    println!("Q16: ok");
    q17_returning_closures();
    println!("Q17: ok");
}
