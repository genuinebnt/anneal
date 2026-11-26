//! RECIPE 1001 (std): Result<T, E> look-up page: combinators, collecting, `?` with From, Box<dyn Error>, exit codes.
//! LEVEL: simple
//! Use in: parsing, validation, I/O wrappers, CLI main, library error design.   Run: cargo run --release --example std_result_methods
//! COMMON USES: every fallible std API (parse, fs, net), config loaders, validation pipelines that collect all
//! errors or stop at the first, CLI `main() -> ExitCode`, converting library errors at crate boundaries.
//! METHODS: (everyday first) Result::map, Result::map_err, Result::and_then, Result::unwrap_or, Result::unwrap_or_else, Result::ok, Result::is_ok, Result::is_err,
//! METHODS: Result::expect, Result::unwrap_or_default, Result::or_else, Result::is_ok_and, Result::is_err_and, Result::err, Result::as_ref, Result::as_mut,
//! METHODS: Result::as_deref, Result::copied, Result::cloned, Result::map_or, Result::map_or_else, Result::and, Result::or, Result::transpose,
//! METHODS: Result::inspect, Result::inspect_err, Result::iter, Result::into_iter, Result::unwrap_err, Result::expect_err, Result::sum, Result::product,
//! METHODS: Iterator::try_fold, Iterator::try_for_each, Iterator::partition, Iterator::collect, std::process::ExitCode, std::process::Termination,
//! METHODS: std::error::Error, Box<dyn Error>::downcast_ref, From::from
//! METHODS: Result::flatten, Result::map_or_default, Result::as_deref_mut, Result::iter_mut
//! DOCS: https://doc.rust-lang.org/std/result/enum.Result.html
//! DEFINED IN: `Result` core (`core::result`); `Error` trait core (`core::error`, in core since 1.81); `Box` alloc; `ExitCode`/`Termination` std (`std::process`).
//! NOT COVERED (and why): unwrap_unchecked/unwrap_err_unchecked (unsafe), into_ok/into_err (unstable in 1.98.1).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   - transform Ok                      -> map                      - transform Err (add context/convert) -> map_err
//!   - chain a fallible step             -> and_then / `?`           - recover from an Err               -> or_else
//!   - default on Err                    -> unwrap_or / unwrap_or_else / unwrap_or_default
//!   - drop the error, keep Option       -> ok()                     - keep only the error -> err()
//!   - test Ok + predicate               -> is_ok_and / is_err_and   - look without moving -> as_ref / as_deref
//!   - Vec<Result<T,E>> -> Result<Vec<T>,E> -> collect               - sum fallibly -> sum::<Result<_,_>>
//!   - stop at first error in a fold     -> try_fold / try_for_each  - keep both oks and errs -> partition
//!   - Option<Result> <-> Result<Option> -> transpose                - log without changing -> inspect / inspect_err
//!   - mix error types in one fn         -> `?` with From, or Box<dyn Error + Send + Sync>
//!   - process exit code from main       -> main() -> ExitCode / Result<(), E> (E: Debug)
//! Read: https://doc.rust-lang.org/std/result/ , https://doc.rust-lang.org/book/ch09-00-error-handling.html , https://doc.rust-lang.org/std/process/trait.Termination.html
//! TRY NEXT:
//!   - [S] In section 5 change the input so the 3rd item is the first bad one and the 5th is also bad; predict which error `collect` returns and which `partition` keeps.
//!   - [S] Remove the `impl From<ParseIntError> for AppError` and see what the `?` error says; then re-add it.
//!   - [M] Replace AppError with a `#[non_exhaustive]` enum plus `source()`, and print the chain with the walker from std_any_error_marker_traits.
//!   - [M] Write `fn validate_all(rows) -> Result<Vec<Row>, Vec<RowError>>` that accumulates every error (partition) and compare to the fail-fast `collect`.
//!   - [A] PROJECT: combine with std_option_methods and std_env_args_vars: a config loader whose `main` returns ExitCode (2 = usage, 1 = invalid config) and is tested through the pure `run(args, env)` function.
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;
use std::process::{ExitCode, Termination};

const COVERED: &[&str] = &[
    "map",
    "map_err",
    "and_then",
    "unwrap_or",
    "unwrap_or_else",
    "ok",
    "is_ok",
    "is_err",
    "expect",
    "unwrap_or_default",
    "or_else",
    "is_ok_and",
    "is_err_and",
    "err",
    "as_ref",
    "as_mut",
    "as_deref",
    "copied",
    "cloned",
    "map_or",
    "map_or_else",
    "and",
    "or",
    "transpose",
    "flatten",
    "map_or_default",
    "as_deref_mut",
    "iter_mut",
    "inspect",
    "inspect_err",
    "iter",
    "into_iter",
    "unwrap_err",
    "expect_err",
    "sum",
    "product",
];

#[derive(Debug, PartialEq)]
enum AppError {
    Parse(ParseIntError),
    Negative(i64),
    Empty,
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Parse(e) => write!(f, "parse error: {e}"),
            AppError::Negative(n) => write!(f, "negative value {n}"),
            AppError::Empty => f.write_str("empty input"),
        }
    }
}
impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AppError::Parse(e) => Some(e),
            _ => None,
        }
    }
}
// `?` calls From::from on the error: this impl is what lets ParseIntError flow into AppError.
impl From<ParseIntError> for AppError {
    fn from(e: ParseIntError) -> Self {
        AppError::Parse(e)
    }
}

fn parse_pos(s: &str) -> Result<i64, AppError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(AppError::Empty);
    }
    let n: i64 = s.parse()?; // ParseIntError -> AppError via From
    if n < 0 {
        Err(AppError::Negative(n))
    } else {
        Ok(n)
    }
}

// ---- 0. Everyday core ----------------------------------------------------------------------------------------
fn section_everyday() {
    assert_eq!("12".parse::<i32>().map(|n| n * 2), Ok(24));
    // map_err: attach context / convert type. Prefer `?` + From when the conversion is mechanical.
    let e = "x"
        .parse::<i32>()
        .map_err(|e| format!("bad port: {e}"))
        .unwrap_err();
    assert!(e.starts_with("bad port: invalid digit"));
    assert_eq!(
        parse_pos("5").and_then(|n| if n > 3 {
            Ok(n)
        } else {
            Err(AppError::Negative(n))
        }),
        Ok(5)
    );
    assert_eq!(parse_pos("zz").unwrap_or(-1), -1);
    assert_eq!(
        parse_pos("zz").unwrap_or_else(|e| e.to_string().len() as i64) > 0,
        true
    );
    assert_eq!("7".parse::<u8>().ok(), Some(7));
    assert_eq!(
        "700".parse::<u8>().err().map(|e| e.to_string()),
        Some("number too large to fit in target type".into())
    );
    assert!(parse_pos("1").is_ok() && parse_pos("-1").is_err());
    assert_eq!("".parse::<i32>().unwrap_or_default(), 0);
    println!("0. everyday core ok");
}

// ---- 1. Recover, predicates, borrowing views -----------------------------------------------------------------
fn section_recovery_and_views() {
    // or_else: try a fallback that may fail too (parse hex if decimal fails).
    let any_radix = |s: &str| {
        s.parse::<i64>()
            .or_else(|_| i64::from_str_radix(s.trim_start_matches("0x"), 16))
    };
    assert_eq!(any_radix("10"), Ok(10));
    assert_eq!(any_radix("0xff"), Ok(255));
    assert!(any_radix("zz").is_err());
    // is_ok_and / is_err_and (1.70).
    assert!(parse_pos("5").is_ok_and(|n| n == 5));
    assert!(parse_pos("-5").is_err_and(|e| matches!(e, AppError::Negative(_))));
    // and / or: the eager, value-based versions (rarely better than and_then/or_else).
    assert_eq!(Ok::<i32, ()>(1).and(Ok::<&str, ()>("b")), Ok("b"));
    assert_eq!(Err::<i32, &str>("a").or(Ok::<i32, &str>(2)), Ok(2));
    // map_or / map_or_else collapse to a plain value.
    assert_eq!(parse_pos("4").map_or(0, |n| n + 1), 5);
    assert_eq!(
        parse_pos("x").map_or_else(|e| e.to_string().len(), |n| n as usize) > 5,
        true
    );
    // as_ref / as_mut: inspect a Result you still need; as_deref: Result<String,E> -> Result<&str,&E>.
    let r: Result<String, AppError> = Ok("hi".into());
    assert_eq!(r.as_ref().map(|s| s.len()), Ok(2));
    assert_eq!(r.as_deref(), Ok("hi"));
    let mut r2: Result<Vec<i32>, ()> = Ok(vec![1]);
    if let Ok(v) = r2.as_mut() {
        v.push(2);
    }
    assert_eq!(r2, Ok(vec![1, 2]));
    // copied / cloned: Result<&T,E> -> Result<T,E>.
    let n = 5;
    assert_eq!(Ok::<&i32, ()>(&n).copied(), Ok(5));
    assert_eq!(
        Ok::<&String, ()>(&"s".to_string()).cloned(),
        Ok("s".to_string())
    );
    // inspect / inspect_err (1.76): logging in the middle of a chain.
    let mut log = vec![];
    let _ = "x"
        .parse::<i32>()
        .inspect(|v| log.push(format!("ok {v}")))
        .inspect_err(|e| log.push(format!("err {e}")));
    assert_eq!(log.len(), 1);
    // expect_err / unwrap_err: for tests that require failure.
    assert_eq!(parse_pos("").expect_err("must fail"), AppError::Empty);
    // iter / into_iter: a Result is a 0/1-element iterator (flatten drops Errs).
    let rs = vec!["1", "x", "3"].into_iter().map(str::parse::<i32>);
    assert_eq!(rs.flatten().collect::<Vec<_>>(), vec![1, 3]);
    // PITFALL: unwrap on Err panics with the Debug of E, so E needs Debug and should be informative.
    assert!(std::panic::catch_unwind(|| parse_pos("zz").unwrap()).is_err());
    println!("1. recovery/predicates/views ok");
}

// ---- 2. Transpose ---------------------------------------------------------------------------------------------
fn section_transpose() {
    let maybe: Option<&str> = Some("12");
    let r: Result<Option<i32>, ParseIntError> = maybe.map(str::parse::<i32>).transpose();
    assert_eq!(r, Ok(Some(12)));
    assert_eq!(None::<&str>.map(str::parse::<i32>).transpose(), Ok(None));
    assert!(Some("z").map(str::parse::<i32>).transpose().is_err());
    // and back: Result<Option<T>,E> -> Option<Result<T,E>> (useful in iterator adaptors with filter_map).
    assert_eq!(Ok::<Option<i32>, ()>(None).transpose(), None);
    // flatten (since 1.89): Result<Result<T, E>, E> -> Result<T, E>. Both layers must share the error type E.
    let nested: Result<Result<u8, &str>, &str> = Ok(Err("inner"));
    assert_eq!(nested.flatten(), Err("inner"));
    assert_eq!(Ok::<Result<u8, &str>, &str>(Ok(1)).flatten(), Ok(1));
    assert_eq!(
        Err::<Result<u8, &str>, &str>("outer").flatten(),
        Err("outer")
    );
    // map_or_default (since 1.98): map Ok, or T::default() on Err (the error is dropped).
    assert_eq!(Ok::<&str, ()>("abc").map_or_default(str::len), 3);
    assert_eq!(Err::<&str, ()>(()).map_or_default(str::len), 0);
    // iter_mut / as_deref_mut: edit the Ok value in place without moving the Result.
    let mut r: Result<String, ()> = Ok("ab".into());
    if let Some(s) = r.as_deref_mut().ok() {
        s.make_ascii_uppercase();
    }
    for s in r.iter_mut() {
        s.push('!');
    }
    assert_eq!(r.as_deref(), Ok("AB!"));
    println!("2. transpose/flatten ok");
}

// ---- 3. Collect / Sum / Product via FromIterator --------------------------------------------------------------
fn section_collect_sum() {
    let good = ["1", "2", "3"];
    let bad = ["1", "x", "y"];
    let parse = |v: &[&str]| {
        v.iter()
            .map(|s| s.parse::<i32>())
            .collect::<Result<Vec<_>, _>>()
    };
    assert_eq!(parse(&good), Ok(vec![1, 2, 3]));
    // collect stops at the FIRST Err; later items are never parsed (short circuit).
    let mut seen = 0;
    let r: Result<Vec<i32>, _> = bad
        .iter()
        .inspect(|_| seen += 1)
        .map(|s| s.parse::<i32>())
        .collect();
    assert!(r.is_err());
    assert_eq!(seen, 2, "iteration stopped at 'x'");
    // sum / product of Results.
    assert_eq!(
        good.iter()
            .map(|s| s.parse::<i32>())
            .sum::<Result<i32, _>>(),
        Ok(6)
    );
    assert_eq!(
        good.iter()
            .map(|s| s.parse::<i32>())
            .product::<Result<i32, _>>(),
        Ok(6)
    );
    // collect into other containers: Result<String,_>, Result<HashMap,_>, Result<(),_>.
    let s: Result<String, ()> = [Ok('a'), Ok('b')].into_iter().collect();
    assert_eq!(s, Ok("ab".to_string()));
    let unit: Result<(), &str> = [Ok(()), Err("boom"), Ok(())].into_iter().collect();
    assert_eq!(unit, Err("boom"));
    println!("3. collect/sum/product ok");
}

// ---- 4. partition / try_fold / try_for_each -------------------------------------------------------------------
fn section_accumulate_and_early_exit() {
    // partition: keep every error (report all invalid rows), unlike collect which stops at the first.
    let rows = ["1", "x", "3", "-4", ""];
    let (oks, errs): (Vec<_>, Vec<_>) = rows.iter().map(|s| parse_pos(s)).partition(|r| r.is_ok());
    let oks: Vec<i64> = oks.into_iter().map(Result::unwrap).collect();
    let errs: Vec<AppError> = errs.into_iter().map(Result::unwrap_err).collect();
    assert_eq!(oks, vec![1, 3]);
    assert_eq!(errs.len(), 3);
    assert_eq!(errs[2], AppError::Empty);
    // try_fold: running total that aborts on overflow (checked_add returns Option; Result works too).
    let total = [1u8, 100, 200]
        .iter()
        .try_fold(0u8, |acc, &x| acc.checked_add(x));
    assert_eq!(total, None);
    let total: Result<u32, String> = ["1", "2", "3"].iter().try_fold(0u32, |acc, s| {
        s.parse::<u32>().map(|n| acc + n).map_err(|e| e.to_string())
    });
    assert_eq!(total, Ok(6));
    // try_for_each: side effects with early exit.
    let mut sent = vec![];
    let r: Result<(), String> = (1..10).try_for_each(|i| {
        if i == 4 {
            return Err(format!("failed at {i}"));
        }
        sent.push(i);
        Ok(())
    });
    assert_eq!((r, sent), (Err("failed at 4".to_string()), vec![1, 2, 3]));
    println!("4. partition/try_fold/try_for_each ok");
}

// ---- 5. `?`, From, Box<dyn Error>, downcast, source chains ----------------------------------------------------
fn sum_line(line: &str) -> Result<i64, AppError> {
    line.split(',').map(parse_pos).sum()
}
fn app_boundary(line: &str) -> Result<i64, Box<dyn Error + Send + Sync>> {
    let n = sum_line(line)?; // AppError -> Box<dyn Error>
    if n > 100 {
        return Err("sum too large".into()); // &str -> Box<dyn Error>
    }
    Ok(n)
}

fn section_question_mark_and_boxed() {
    assert_eq!(sum_line("1,2,3"), Ok(6));
    assert!(matches!(sum_line("1,a"), Err(AppError::Parse(_))));
    assert_eq!(
        app_boundary("50,60").unwrap_err().to_string(),
        "sum too large"
    );
    let e = app_boundary("1,-2").unwrap_err();
    // downcast_ref recovers the concrete type; use sparingly (matching on a real enum is better).
    assert_eq!(e.downcast_ref::<AppError>(), Some(&AppError::Negative(-2)));
    assert!(e.downcast_ref::<std::io::Error>().is_none());
    // source() chain: Parse -> ParseIntError.
    let e = parse_pos("q").unwrap_err();
    assert!(e.source().is_some());
    println!("5. ? / From / Box<dyn Error> ok");
}

// ---- 6. main returning Result / ExitCode / Termination --------------------------------------------------------
// `fn main() -> Result<(), E>` prints `Error: {E:?}` and exits 1 on Err. `-> ExitCode` gives full control.
struct Outcome(Result<(), AppError>);
impl Termination for Outcome {
    fn report(self) -> ExitCode {
        match self.0 {
            Ok(()) => ExitCode::SUCCESS,
            Err(AppError::Empty) => ExitCode::from(2), // usage-like error
            Err(_) => ExitCode::FAILURE,
        }
    }
}
fn run(input: &str) -> Outcome {
    Outcome(sum_line(input).map(|_| ()))
}

fn section_exit_codes() {
    // ExitCode has no PartialEq (it is opaque); compare Debug output, or keep an enum and map at the edge.
    assert_eq!(
        format!("{:?}", run("1,2").report()),
        format!("{:?}", ExitCode::SUCCESS)
    );
    assert_eq!(
        format!("{:?}", run(",").report()),
        format!("{:?}", ExitCode::from(2))
    );
    assert_eq!(
        format!("{:?}", run("a").report()),
        format!("{:?}", ExitCode::FAILURE)
    );
    // Real mains: fn main() -> ExitCode { run(&arg).report() }  (see std_env_args_vars for the testable shape)
    println!("6. Termination/ExitCode ok");
}

// ---- 7. Production tips and tricks ---------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Libraries: typed error enum + `?`; apps: Box<dyn Error + Send + Sync>. Never unwrap input-derived values.
    // 2. `.ok()` silently discards the error: log it first (inspect_err) or you will debug blind (clippy::manual_ok_err).
    let mut errs = 0;
    let v = "x".parse::<i32>().inspect_err(|_| errs += 1).ok();
    assert_eq!((v, errs), (None, 1));
    // 3. `map_err(|e| e.into())` -> use `?` (clippy::useless_conversion / question_mark).
    // 4. Result in a loop with `?` instead of unwrap: wrap loop body in a closure/fn.
    // 5. Fail-fast vs accumulate: collect::<Result<Vec,_>> vs partition (see section 4); choose per UX.
    // 6. `#[must_use]` is already on Result: `let _ = fallible();` is a deliberate discard (clippy::let_underscore_must_use).
    // 7. Result<(), E> with try_for_each beats a for loop with early return only when the body is already a closure.
    // 8. match-on-Result that only forwards is `?` (clippy::question_mark).
    fn f(s: &str) -> Result<i32, ParseIntError> {
        match s.parse::<i32>() {
            Ok(v) => Ok(v),
            Err(e) => Err(e),
        } // <- clippy::question_mark / needless_match: write `s.parse()`
    }
    assert_eq!(f("3"), Ok(3));
    // 9. Don't format errors into Strings at each layer; keep `source()` so the top level can print the chain.
    let e: Box<dyn Error> = Box::new(AppError::Parse("x".parse::<i32>().unwrap_err()));
    let mut depth = 0;
    let mut cur: Option<&dyn Error> = Some(e.as_ref());
    while let Some(c) = cur {
        depth += 1;
        cur = c.source();
    }
    assert_eq!(depth, 2);
    // 10. unwrap_or_else(|e| ..) for "log and use default" at the edge; `unwrap_or_default` when the default is natural.
    let port: u16 = "bad".parse().unwrap_or_default();
    assert_eq!(port, 0);
    // 11. `expect("why this cannot fail")` documents invariants (clippy::expect_used is a restriction lint teams opt into).
    let n: u8 = "255".parse().expect("literal fits in u8");
    assert_eq!(n, 255);
    // 12. Convert at API edges with #[non_exhaustive] error enums so adding variants is not a breaking change.
    println!("7. production tips ok");
}

fn main() {
    let mut seen = std::collections::HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_recovery_and_views();
    section_transpose();
    section_collect_sum();
    section_accumulate_and_early_exit();
    section_question_mark_and_boxed();
    section_exit_codes();
    section_production_tips();
    println!("all {} Result methods exercised; done", COVERED.len());
}
