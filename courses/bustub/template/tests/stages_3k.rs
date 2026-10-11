//! Tests for module 3k: one error type for the whole database. They use only what the stage pages name: `From` for each layer's error,
//! `Display`, `source()`, `chain()`, `root_cause()`, `report()`, `context`, `code()` and `client_message()`.

use std::error::Error;
use std::io;
use std::sync::mpsc;
use std::thread;

use proptest::prelude::*;

use bustub::catalog::name_catalog::NameError;
use bustub::common::db_error::{Context, DbError, DbResult};
use bustub::common::exception::{Exception, ExceptionType};
use bustub::sql::mini_lexer::{LexError, LexErrorKind};
use bustub::sql::resolve::ResolveError;
use bustub::storage::disk::page_bitmap::FreeError;
use bustub::storage::table::checksum::SealError;
use bustub::types::int_ops::IntOpError;
use bustub::types::int_parse::IntError;
use bustub::types::like_match::LikeError;

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "no such file")
}

/// The first cause of the error (walking `source()`, which stage 3k-01 provides) that is an `E`.
fn cause<E: Error + 'static>(e: &DbError) -> Option<&E> {
    std::iter::successors(e.source(), |&c| c.source()).find_map(|c| c.downcast_ref::<E>())
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

// ---- 3k-01 · One type for every failure ------------------------------------------------------------------------------------

#[test]
fn s3k_01_an_io_error_converts_and_the_message_is_kept() {
    let e = DbError::from(not_found());
    assert_eq!(e.to_string(), "no such file", "an error made from an I/O error says what the I/O error says");
}

#[test]
fn s3k_01_the_io_error_is_kept_as_a_value_not_flattened_to_text() {
    let e = DbError::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
    let kept = cause::<io::Error>(&e).expect("the I/O error must still be in the chain of causes");
    assert_eq!(kept.kind(), io::ErrorKind::PermissionDenied, "the kept I/O error is the original one, with its kind");
}

#[test]
fn s3k_01_an_engine_exception_converts_and_the_message_is_kept() {
    let ex = Exception::new(ExceptionType::DivideByZero, "1 / 0");
    let e = DbError::from(ex.clone());
    assert_eq!(e.to_string(), ex.to_string(), "an error made from an Exception says what the Exception says");
}

#[test]
fn s3k_01_the_exception_is_kept_as_a_value() {
    let ex = Exception::new(ExceptionType::OutOfRange, "300 does not fit in TINYINT");
    let e = DbError::from(ex.clone());
    assert_eq!(cause::<Exception>(&e), Some(&ex), "the Exception, with its kind and message, must be in the chain of causes");
}

#[test]
fn s3k_01_source_is_the_cause_and_the_chain_ends_there() {
    let e = DbError::from(not_found());
    let source = e.source().expect("an error made from a cause has that cause as its source");
    assert!(source.is::<io::Error>(), "the source is the I/O error itself, not a copy of its text");
    assert!(source.source().is_none(), "the I/O error here has no cause of its own, so the chain ends");
}

#[test]
fn s3k_01_the_error_moves_across_threads_and_into_a_boxed_error() {
    assert_send_sync_static::<DbError>();
    let e = DbError::from(not_found());
    let text = thread::spawn(move || e.to_string()).join().unwrap();
    assert_eq!(text, "no such file", "a DbError moved to another thread and kept its message");
    let boxed: Box<dyn Error + Send + Sync> = DbError::from(not_found()).into();
    assert_eq!(boxed.to_string(), "no such file", "a DbError is a std::error::Error: it converts into a boxed error");
    assert!(boxed.downcast::<DbError>().is_ok(), "and the DbError can be taken back out of the box");
}

#[test]
fn s3k_01_question_mark_converts_in_a_function_that_returns_a_dberror() {
    fn read_missing() -> Result<Vec<u8>, DbError> {
        Ok(std::fs::read("/nonexistent-bustub-directory/missing.db")?)
    }
    let e = read_missing().expect_err("the file does not exist");
    assert_eq!(cause::<io::Error>(&e).map(|c| c.kind()), Some(io::ErrorKind::NotFound), "`?` turned the I/O error into a DbError that keeps it");
}

// ---- 3k-02 · Every layer's error, and the question mark ---------------------------------------------------------------------

/// A lower layer's error converts into a `DbError` that says what the error says, in words that contain `phrases`, and keeps the error.
fn check<E>(mk: impl Fn() -> E, phrases: &[&str], what: &str)
where
    E: Error + PartialEq + Send + Sync + 'static,
    DbError: From<E>,
{
    let text = mk().to_string().to_lowercase();
    for p in phrases {
        assert!(text.contains(p), "{what}: the message {text:?} should contain {p:?}");
    }
    let d = DbError::from(mk());
    assert_eq!(d.to_string().to_lowercase(), text, "{what}: the DbError says what its cause says");
    assert_eq!(cause::<E>(&d), Some(&mk()), "{what}: the cause is kept as a value");
}

#[test]
fn s3k_02_integer_errors_are_described_and_kept() {
    check(|| IntError::Empty, &["empty"], "IntError::Empty");
    check(|| IntError::Invalid, &["invalid"], "IntError::Invalid");
    check(|| IntError::OutOfRange, &["out of range"], "IntError::OutOfRange");
    check(|| IntOpError::Overflow, &["overflow"], "IntOpError::Overflow");
    check(|| IntOpError::DivisionByZero, &["division by zero"], "IntOpError::DivisionByZero");
}

#[test]
fn s3k_02_pattern_and_name_errors_are_described_and_kept() {
    check(|| LikeError::TrailingEscape, &["escape"], "LikeError::TrailingEscape");
    check(|| NameError::Exists, &["already exists"], "NameError::Exists");
    check(|| NameError::Missing, &["does not exist"], "NameError::Missing");
    check(|| NameError::Invalid, &["invalid"], "NameError::Invalid");
}

#[test]
fn s3k_02_lexer_errors_say_what_and_where() {
    let cases = [
        (LexErrorKind::UnterminatedString, "unterminated string"),
        (LexErrorKind::UnterminatedIdent, "unterminated"),
        (LexErrorKind::UnterminatedComment, "unterminated comment"),
        (LexErrorKind::BadChar, "unexpected character"),
        (LexErrorKind::IntegerOverflow, "out of range"),
    ];
    for (kind, phrase) in cases {
        let what = format!("{kind:?}");
        check(|| LexError { pos: 17, kind: match &kind {
            LexErrorKind::UnterminatedString => LexErrorKind::UnterminatedString,
            LexErrorKind::UnterminatedIdent => LexErrorKind::UnterminatedIdent,
            LexErrorKind::UnterminatedComment => LexErrorKind::UnterminatedComment,
            LexErrorKind::BadChar => LexErrorKind::BadChar,
            LexErrorKind::IntegerOverflow => LexErrorKind::IntegerOverflow,
        } }, &[phrase, "17"], &what);
    }
}

#[test]
fn s3k_02_resolver_page_and_checksum_errors_are_described_and_kept() {
    check(|| ResolveError::UnknownTable, &["unknown table"], "ResolveError::UnknownTable");
    check(|| ResolveError::UnknownColumn, &["unknown column"], "ResolveError::UnknownColumn");
    check(|| ResolveError::Ambiguous(vec![0, 1]), &["ambiguous"], "ResolveError::Ambiguous");
    check(|| FreeError::DoubleFree, &["already free"], "FreeError::DoubleFree");
    check(|| FreeError::OutOfRange, &["out of range"], "FreeError::OutOfRange");
    check(|| SealError::TooShort, &["too short"], "SealError::TooShort");
    check(|| SealError::Mismatch, &["checksum"], "SealError::Mismatch");
}

#[test]
fn s3k_02_different_causes_of_one_type_say_different_things() {
    let ints: Vec<String> = [IntError::Empty, IntError::Invalid, IntError::OutOfRange].into_iter().map(|e| DbError::from(e).to_string()).collect();
    let names: Vec<String> = [NameError::Exists, NameError::Missing, NameError::Invalid].into_iter().map(|e| DbError::from(e).to_string()).collect();
    for texts in [ints, names] {
        let mut unique = texts.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), texts.len(), "every variant needs its own message, got {texts:?}");
    }
}

#[test]
fn s3k_02_question_mark_converts_the_error_of_every_layer() {
    fn lift<E>(r: Result<(), E>) -> DbResult<()>
    where
        DbError: From<E>,
    {
        r?;
        Ok(())
    }
    assert!(lift(Err(IntError::Empty)).is_err(), "IntError converts with `?`");
    assert!(lift(Err(IntOpError::Overflow)).is_err(), "IntOpError converts with `?`");
    assert!(lift(Err(LikeError::TrailingEscape)).is_err(), "LikeError converts with `?`");
    assert!(lift(Err(NameError::Missing)).is_err(), "NameError converts with `?`");
    assert!(lift(Err(LexError { pos: 0, kind: LexErrorKind::BadChar })).is_err(), "LexError converts with `?`");
    assert!(lift(Err(ResolveError::UnknownColumn)).is_err(), "ResolveError converts with `?`");
    assert!(lift(Err(FreeError::DoubleFree)).is_err(), "FreeError converts with `?`");
    assert!(lift(Err(SealError::Mismatch)).is_err(), "SealError converts with `?`");
    assert!(lift(Ok::<(), IntError>(())).is_ok(), "and an Ok passes through untouched");
}

// ---- 3k-03 · Context that travels with the error -----------------------------------------------------------------------------

#[test]
fn s3k_03_context_puts_a_new_message_on_top() {
    let e = DbError::from(not_found()).context("opening the log");
    assert_eq!(e.to_string(), "opening the log", "the outermost message is the context, and only the context");
}

#[test]
fn s3k_03_the_source_of_a_context_is_the_error_it_wraps() {
    let e = DbError::from(not_found()).context("opening the log");
    assert_eq!(e.source().expect("a context has a source").to_string(), "no such file", "the layer below says what the original error said");
}

#[test]
fn s3k_03_report_lists_every_layer_outermost_first_and_a_repeat_once() {
    let e = DbError::from(not_found()).context("opening the log").context("starting up");
    assert_eq!(e.report(), "starting up: opening the log: no such file", "layers outermost first; the cause is not repeated");
    assert_eq!(DbError::from(not_found()).report(), "no such file", "an error with no context reports just its message");
}

#[test]
fn s3k_03_root_cause_is_the_original_error() {
    let e = DbError::from(not_found()).context("opening the log").context("starting up");
    let root = e.root_cause();
    assert_eq!(root.downcast_ref::<io::Error>().map(|c| c.kind()), Some(io::ErrorKind::NotFound), "the innermost error is the I/O error");
}

#[test]
fn s3k_03_chain_counts_every_layer() {
    let e = DbError::from(not_found());
    assert_eq!(e.chain().count(), 2, "the DbError made by `From`, then its cause");
    let e = e.context("a").context("b");
    assert_eq!(e.chain().count(), 4, "two context layers add two elements");
    assert_eq!(e.chain().next().map(|c| c.to_string()).as_deref(), Some("b"), "the chain starts at the error itself");
}

#[test]
fn s3k_03_context_on_a_result_converts_and_wraps() {
    let r: Result<i64, IntError> = Err(IntError::Empty);
    let e = r.context("parsing the id").expect_err("the result was an error");
    assert_eq!(e.to_string(), "parsing the id", "the context is the outer message");
    assert_eq!(cause::<IntError>(&e), Some(&IntError::Empty), "the integer error is still in the chain");
    assert_eq!(e.report(), format!("parsing the id: {}", IntError::Empty), "and the report names both layers");
    assert_eq!(Ok::<i64, IntError>(7).context("unused").unwrap(), 7, "an Ok passes through");
}

#[test]
fn s3k_03_with_context_builds_the_message_only_on_failure() {
    let mut built = 0;
    let ok: Result<u8, IntError> = Ok(1);
    let _ = ok.with_context(|| {
        built += 1;
        "never needed".to_string()
    });
    assert_eq!(built, 0, "no error: the closure must not run");
    let err: Result<u8, IntError> = Err(IntError::Invalid);
    let e = err
        .with_context(|| {
            built += 1;
            format!("reading column {}", 3)
        })
        .unwrap_err();
    assert_eq!(built, 1, "an error: the closure runs once");
    assert_eq!(e.to_string(), "reading column 3", "and its message is the context");
}

#[test]
fn s3k_03_context_stacks_on_a_dbresult() {
    fn inner() -> DbResult<()> {
        Err(NameError::Missing).context("looking up the table")
    }
    fn outer() -> DbResult<()> {
        inner().context("running the statement")
    }
    let e = outer().unwrap_err();
    assert_eq!(e.report(), format!("running the statement: looking up the table: {}", NameError::Missing), "contexts added in two functions stack in call order");
}

// ---- 3k-04 · Codes and what a client may see -------------------------------------------------------------------------------

fn engine(kind: ExceptionType) -> DbError {
    DbError::from(Exception::new(kind, "message"))
}

#[test]
fn s3k_04_engine_exceptions_have_their_codes() {
    let table = [
        (ExceptionType::Invalid, "22023"),
        (ExceptionType::OutOfRange, "22003"),
        (ExceptionType::Decimal, "22003"),
        (ExceptionType::Conversion, "22018"),
        (ExceptionType::DivideByZero, "22012"),
        (ExceptionType::UnknownType, "42704"),
        (ExceptionType::MismatchType, "42804"),
        (ExceptionType::IncompatibleType, "42804"),
        (ExceptionType::NotImplemented, "0A000"),
        (ExceptionType::Io, "58030"),
        (ExceptionType::Execution, "XX000"),
        (ExceptionType::Optimizer, "XX000"),
    ];
    for (kind, code) in table {
        assert_eq!(engine(kind).code(), code, "the code of an Exception of kind {kind:?}");
    }
}

#[test]
fn s3k_04_the_other_layers_have_their_codes() {
    let table: Vec<(&str, DbError, &str)> = vec![
        ("io error", DbError::from(not_found()), "58030"),
        ("IntError::Empty", IntError::Empty.into(), "22P02"),
        ("IntError::Invalid", IntError::Invalid.into(), "22P02"),
        ("IntError::OutOfRange", IntError::OutOfRange.into(), "22003"),
        ("IntOpError::Overflow", IntOpError::Overflow.into(), "22003"),
        ("IntOpError::DivisionByZero", IntOpError::DivisionByZero.into(), "22012"),
        ("LikeError", LikeError::TrailingEscape.into(), "22025"),
        ("NameError::Exists", NameError::Exists.into(), "42P07"),
        ("NameError::Missing", NameError::Missing.into(), "42P01"),
        ("NameError::Invalid", NameError::Invalid.into(), "42602"),
        ("LexError", LexError { pos: 4, kind: LexErrorKind::BadChar }.into(), "42601"),
        ("ResolveError::UnknownTable", ResolveError::UnknownTable.into(), "42P01"),
        ("ResolveError::UnknownColumn", ResolveError::UnknownColumn.into(), "42703"),
        ("ResolveError::Ambiguous", ResolveError::Ambiguous(vec![0, 1]).into(), "42702"),
        ("FreeError::DoubleFree", FreeError::DoubleFree.into(), "XX000"),
        ("FreeError::OutOfRange", FreeError::OutOfRange.into(), "XX000"),
        ("SealError::TooShort", SealError::TooShort.into(), "XX001"),
        ("SealError::Mismatch", SealError::Mismatch.into(), "XX001"),
    ];
    for (what, e, code) in table {
        assert_eq!(e.code(), code, "the code of {what}");
    }
}

#[test]
fn s3k_04_a_code_is_five_characters_of_digits_and_capitals() {
    for e in [DbError::from(not_found()), engine(ExceptionType::NotImplemented), NameError::Missing.into(), SealError::Mismatch.into()] {
        let code = e.code();
        assert_eq!(code.len(), 5, "{code} must have five characters");
        assert!(code.chars().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase()), "{code} may only have digits and capital letters");
    }
}

#[test]
fn s3k_04_context_never_changes_the_code() {
    let plain = DbError::from(ResolveError::UnknownColumn);
    let wrapped = DbError::from(ResolveError::UnknownColumn).context("binding the select list").context("running the statement");
    assert_eq!(wrapped.code(), plain.code(), "the code comes from the innermost cause, so layers on top do not change it");
    assert_eq!(wrapped.code(), "42703", "and it is the code of an unknown column");
}

#[test]
fn s3k_04_a_clients_own_mistakes_are_shown_with_their_message() {
    let e = DbError::from(NameError::Exists).context("creating table orders");
    assert_eq!(e.client_message(), "ERROR 42P07: creating table orders", "code, then the outermost message");
    let e = DbError::from(IntOpError::DivisionByZero);
    assert_eq!(e.client_message(), "ERROR 22012: division by zero", "a plain error shows its own message");
}

#[test]
fn s3k_04_system_failures_show_nothing_of_their_inside() {
    let io = DbError::from(io::Error::new(io::ErrorKind::PermissionDenied, "/var/lib/bustub/secret.db: permission denied")).context("reading page 7");
    assert_eq!(io.client_message(), "ERROR 58030: internal error", "class 58: the path and the context stay inside");
    let internal = DbError::from(Exception::new(ExceptionType::Execution, "pointer 0xdeadbeef in executor")).context("running the plan");
    assert_eq!(internal.client_message(), "ERROR XX000: internal error", "class XX: nothing about the internals leaks");
    let corrupt = DbError::from(SealError::Mismatch);
    assert_eq!(corrupt.client_message(), "ERROR XX001: internal error", "corruption is the system's problem, not the client's");
}

// ---- 3k-05 · Boss: a statement fails in every layer ----------------------------------------------------------------------------

fn lex_layer() -> DbResult<()> {
    Err(LexError { pos: 3, kind: LexErrorKind::BadChar }).context("lexing the statement")
}

fn resolve_layer() -> DbResult<()> {
    Err(ResolveError::UnknownTable).context("resolving names")
}

fn storage_layer() -> DbResult<()> {
    Err(io::Error::new(io::ErrorKind::PermissionDenied, "/var/lib/bustub/secret.db: denied")).context("reading page 7")
}

fn execute_layer() -> DbResult<()> {
    Err(Exception::new(ExceptionType::DivideByZero, "1 / 0")).context("executing the projection")
}

#[test]
fn s3k_05_errors_from_every_layer_cross_threads_with_their_context() {
    let (tx, rx) = mpsc::channel::<(usize, DbError)>();
    let layers: [fn() -> DbResult<()>; 4] = [lex_layer, resolve_layer, storage_layer, execute_layer];
    let handles: Vec<_> = layers
        .into_iter()
        .enumerate()
        .map(|(i, layer)| {
            let tx = tx.clone();
            thread::spawn(move || {
                let e = layer().context(format!("statement {i}")).unwrap_err();
                tx.send((i, e)).unwrap();
            })
        })
        .collect();
    drop(tx);
    for h in handles {
        h.join().unwrap();
    }
    let mut got: Vec<(usize, DbError)> = rx.iter().collect();
    got.sort_by_key(|(i, _)| *i);
    let codes: Vec<&str> = got.iter().map(|(_, e)| e.code()).collect();
    assert_eq!(codes, ["42601", "42P01", "58030", "22012"], "one code per layer, whichever thread sent it first");
    assert_eq!(got[0].1.report(), "statement 0: lexing the statement: unexpected character at position 3", "the lexer error, with both contexts");
    assert!(got[3].1.report().starts_with("statement 3: executing the projection: "), "the executor error keeps its contexts in order");
}

fn base(i: usize) -> (DbError, &'static str, String) {
    let e = match i {
        0 => DbError::from(not_found()),
        1 => DbError::from(Exception::new(ExceptionType::DivideByZero, "1 / 0")),
        2 => DbError::from(IntError::OutOfRange),
        3 => DbError::from(NameError::Missing),
        4 => DbError::from(LexError { pos: 9, kind: LexErrorKind::UnterminatedString }),
        5 => DbError::from(ResolveError::UnknownColumn),
        6 => DbError::from(SealError::Mismatch),
        _ => DbError::from(FreeError::DoubleFree),
    };
    let code = ["58030", "22012", "22003", "42P01", "42601", "42703", "XX001", "XX000"][i.min(7)];
    let text = e.to_string();
    (e, code, text)
}

proptest! {
    #[test]
    fn s3k_05_random_context_stacks_keep_the_code_the_root_and_the_report(i in 0usize..8, ctx in proptest::collection::vec("[a-z]{1,8}", 0..8)) {
        let (mut e, code, text) = base(i);
        // numbered, so that no two layers say the same thing (a report shows a repeated message once)
        let layers: Vec<String> = ctx.iter().enumerate().map(|(n, c)| format!("ctx-{n}-{c}")).collect();
        for l in &layers {
            e = e.context(l.clone());
        }
        let mut parts: Vec<String> = layers.iter().rev().cloned().collect();
        parts.push(text.clone());
        prop_assert_eq!(e.report(), parts.join(": "), "the report is every layer, outermost first, the cause once");
        prop_assert_eq!(e.chain().count(), ctx.len() + 2, "each context adds one element to the chain");
        prop_assert_eq!(e.code(), code, "context never changes the code");
        prop_assert_eq!(e.root_cause().to_string(), text, "the root cause is the original error");
    }
}

#[test]
fn s3k_05_the_client_never_sees_what_lies_inside() {
    let internal = Err::<(), _>(Exception::new(ExceptionType::Execution, "pointer 0xdeadbeef in the join executor")).context("running the plan");
    for e in [storage_layer().unwrap_err(), internal.unwrap_err()] {
        let shown = e.client_message();
        assert!(!shown.contains("secret") && !shown.contains("0xdeadbeef") && !shown.contains("page 7") && !shown.contains("running the plan"), "no internal text in {shown:?}");
        assert!(shown.ends_with("internal error"), "a system failure only says so, got {shown:?}");
    }
    let shown = lex_layer().unwrap_err().client_message();
    assert_eq!(shown, "ERROR 42601: lexing the statement", "a syntax error is the client's, and says where it happened");
    let shown = execute_layer().unwrap_err().client_message();
    assert_eq!(shown, "ERROR 22012: executing the projection", "a division by zero is the client's too");
}

#[test]
fn s3k_05_a_very_long_chain_is_walked_and_reported() {
    let mut e = DbError::from(not_found());
    for i in 0..2_000 {
        e = e.context(format!("layer {i}"));
    }
    assert_eq!(e.chain().count(), 2_002, "every layer is in the chain");
    let report = e.report();
    assert!(report.starts_with("layer 1999: layer 1998: ") && report.ends_with(": layer 0: no such file"), "the report runs from the outermost layer to the cause");
    assert_eq!(e.code(), "58030", "and the code is still the cause's");
}

#[test]
fn s3k_05_a_function_can_return_a_boxed_error_and_the_dberror_comes_back_out() {
    fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
        storage_layer()?;
        Ok(())
    }
    let boxed = run().unwrap_err();
    let back = boxed.downcast::<DbError>().expect("the boxed error is the DbError");
    assert_eq!(back.code(), "58030", "and it is the same error, code included");
}
