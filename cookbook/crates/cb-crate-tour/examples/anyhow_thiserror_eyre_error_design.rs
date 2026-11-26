//! RECIPE 1111 (crates): error design with thiserror, anyhow, eyre and miette: sources, context, downcasting, codes, reports and testing
//! LEVEL: intermediate
//! Use in: any crate with a public API (typed errors) and any binary that reports failures to people (anyhow).   Run: cargo run --release -p cb-crate-tour --example anyhow_thiserror_eyre_error_design
//! COMMON USES: library error enums, adding "what was I doing" context in applications, mapping errors to HTTP statuses.
//! METHODS: thiserror::Error, #[error("..")], #[error(transparent)], #[from], #[source], anyhow::Error, anyhow::Result, anyhow::Context::context, anyhow::Context::with_context, anyhow::anyhow!, anyhow::bail!, anyhow::ensure!, Error::new, Error::msg, Error::context, Error::chain, Error::root_cause, Error::downcast_ref, Error::downcast, Error::is, std::error::Error::source, anyhow::Ok, Error::from_boxed, Error::downcast_mut, Error::into_boxed_dyn_error, Error::reallocate_into_boxed_dyn_error_without_backtrace, eyre::Report, eyre::eyre!, eyre::bail!, eyre::ensure!, eyre::WrapErr::{wrap_err, wrap_err_with}, eyre::OptionExt::ok_or_eyre, eyre::EyreHandler, eyre::set_hook, Report::{handler, handler_mut, chain, downcast_ref}, miette::Diagnostic, #[diagnostic(code, help, severity, url)], #[label], #[source_code], #[related], miette::SourceSpan, miette::NamedSource, miette::Report::with_source_code, miette::NarratableReportHandler::render_report
//! DOCS: https://docs.rs/thiserror/2/thiserror/ https://docs.rs/anyhow/1/anyhow/ https://docs.rs/eyre/0.6/eyre/ https://docs.rs/miette/7/miette/
//! CRATES: thiserror = "2", anyhow = "1", eyre = "0.6", miette = "7" (default features: derive, no "fancy")
//! NOT COVERED: color-eyre. It is a ready-made `EyreHandler` plus a panic hook with coloured, environment-dependent output;
//! section `eyre_layer` shows the handler mechanism it plugs into. miette's graphical renderer needs the "fancy" feature,
//! which is not enabled here: reports are rendered with the plain-text NarratableReportHandler so they can be asserted.
//!
//! THE DECISION
//!   Will a CALLER branch on the failure?  yes -> a concrete enum (thiserror)    no, it is only reported -> anyhow::Error
//!   Libraries usually expose the first; the top of an application uses the second.
//!
//! CHEAT SHEET
//!   wrap a cause and keep it                -> #[source] field (or a field named `source`)
//!   let `?` convert automatically           -> #[from] (one variant per source TYPE)
//!   the variant IS the inner error          -> #[error(transparent)]
//!   add "while doing X"                     -> .with_context(|| format!(..))?   (lazy: only runs on error)
//!   print the whole chain on one line       -> format!("{err:#}")
//!   print chain + backtrace                 -> format!("{err:?}")  (what `fn main() -> anyhow::Result<()>` prints)
//!   recover a typed error from anyhow       -> err.downcast_ref::<MyError>()
//!   need your OWN report format/extra data  -> eyre (pluggable EyreHandler); the API is otherwise close to anyhow
//!   point at a span of the user's INPUT     -> miette: Diagnostic + #[source_code] + #[label]  (parsers, config, DSLs)
//!
//! RULE: an error EITHER displays its cause's text OR returns it from source(). Doing both prints it twice in any
//! chain-aware reporter.
//! ANYHOW vs EYRE: the same idea (one boxed dynamic error with context). eyre adds a global hook that builds a handler
//! for every Report; the hook can be installed ONCE per process, before the first Report exists. That makes it an
//! application decision: a library should not call set_hook.
//! BACKTRACES: anyhow captures one when RUST_BACKTRACE or RUST_LIB_BACKTRACE is set. This example does not assert on it
//! because that depends on the environment.
//!
//! Read: https://doc.rust-lang.org/std/error/trait.Error.html ; https://github.com/dtolnay/thiserror ; https://github.com/dtolnay/anyhow ; https://rust-lang.github.io/api-guidelines/interoperability.html#c-good-err
//!
//! TRY NEXT:
//!   - [S] Add `{source}` to the `Io` variant's message and read the `{:#}` output. Why is it wrong?
//!   - [M] Replace the public enum with an opaque struct plus `kind()` so you can add variants without breaking callers.
//!   - [A] PROJECT: give `RepoError` in the sqlx recipe this design and test every code and status.
#![allow(dead_code)]
use anyhow::{Context, anyhow, bail, ensure};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::num::ParseIntError;

// ---------------- the library layer ----------------
#[derive(Debug, thiserror::Error)]
#[non_exhaustive] // downstream matches need a wildcard, so adding a variant is not a breaking change
pub enum ConfigError {
    #[error("cannot read {path}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("line {line}: expected key=value, found {text:?}")]
    Syntax { line: usize, text: String },
    #[error("key `{key}` is not a number")]
    BadNumber {
        key: &'static str,
        source: ParseIntError, // a field NAMED source is the source without any attribute
    },
    #[error("missing key `{0}`")]
    Missing(&'static str),
    #[error("{key} must be in {min}..={max}, got {got}")]
    OutOfRange {
        key: &'static str,
        got: u64,
        min: u64,
        max: u64,
    },
}
impl ConfigError {
    /// A stable machine-readable code. Messages may be reworded; codes are a contract.
    pub fn code(&self) -> &'static str {
        match self {
            ConfigError::Io { .. } => "config.io",
            ConfigError::Syntax { .. } => "config.syntax",
            ConfigError::BadNumber { .. } => "config.number",
            ConfigError::Missing(_) => "config.missing",
            ConfigError::OutOfRange { .. } => "config.range",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Config {
    pub workers: u64,
}
pub fn parse_config(text: &str) -> Result<Config, ConfigError> {
    let mut map = HashMap::new();
    for (i, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let (k, v) = line.split_once('=').ok_or_else(|| ConfigError::Syntax {
            line: i + 1,
            text: line.to_string(),
        })?;
        map.insert(k.trim(), v.trim());
    }
    let raw = map.get("workers").ok_or(ConfigError::Missing("workers"))?;
    let workers: u64 = raw.parse().map_err(|source| ConfigError::BadNumber {
        key: "workers",
        source,
    })?;
    if !(1..=64).contains(&workers) {
        return Err(ConfigError::OutOfRange {
            key: "workers",
            got: workers,
            min: 1,
            max: 64,
        });
    }
    Ok(Config { workers })
}
pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    // No blanket From<io::Error>: it would lose the path, the one thing the user needs.
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.into(),
        source,
    })?;
    parse_config(&text)
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("configuration is invalid")]
    Config(#[from] ConfigError),
    #[error("user {0} not found")]
    NotFound(u64),
    /// transparent: Display and source() are forwarded to the inner error, so this layer adds nothing to the chain.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
impl ServiceError {
    fn status(&self) -> u16 {
        match self {
            ServiceError::NotFound(_) => 404,
            ServiceError::Config(_) | ServiceError::Other(_) => 500,
        }
    }
    /// What an API client sees. Internal causes (paths, SQL, hosts) stay in the logs.
    fn public_message(&self) -> String {
        if self.status() == 500 {
            "internal error".into()
        } else {
            self.to_string()
        }
    }
}

/// Walk source() by hand: what every reporter does.
fn chain_of(e: &(dyn StdError + 'static)) -> Vec<String> {
    let mut out = vec![];
    let mut next = Some(e);
    while let Some(e) = next {
        out.push(e.to_string());
        next = e.source();
    }
    out
}

fn thiserror_layer() {
    let cases = [
        (
            "workers",
            "config.syntax",
            "line 1: expected key=value, found \"workers\"",
        ),
        ("", "config.missing", "missing key `workers`"),
        (
            "workers=many",
            "config.number",
            "key `workers` is not a number",
        ),
        (
            "workers=0",
            "config.range",
            "workers must be in 1..=64, got 0",
        ),
    ];
    for (input, code, message) in cases {
        let e = parse_config(input).unwrap_err();
        assert_eq!(
            (e.code(), e.to_string().as_str()),
            (code, message),
            "{input:?}"
        );
    }
    assert_eq!(
        parse_config("\nworkers = 8\n").unwrap(),
        Config { workers: 8 }
    );
    // Tests should match the VARIANT and its data, not only the text.
    assert!(matches!(
        parse_config("workers=65"),
        Err(ConfigError::OutOfRange { got: 65, .. })
    ));

    let e = parse_config("workers=x").unwrap_err();
    assert_eq!(
        chain_of(&e),
        [
            "key `workers` is not a number",
            "invalid digit found in string"
        ]
    );
    assert!(
        e.source()
            .unwrap()
            .downcast_ref::<ParseIntError>()
            .is_some()
    );

    let e = load_config("/definitely/not/here.conf").unwrap_err();
    let ConfigError::Io { path, source } = &e else {
        panic!("{e:?}")
    };
    assert_eq!(
        (path.as_str(), source.kind()),
        ("/definitely/not/here.conf", std::io::ErrorKind::NotFound)
    );

    // #[from] lets `?` convert.
    fn start(text: &str) -> Result<Config, ServiceError> {
        Ok(parse_config(text)?)
    }
    let s = start("").unwrap_err();
    assert!(matches!(
        s,
        ServiceError::Config(ConfigError::Missing("workers"))
    ));
    assert_eq!(
        chain_of(&s),
        ["configuration is invalid", "missing key `workers`"]
    );
    assert_eq!(
        (s.status(), s.public_message().as_str()),
        (500, "internal error")
    );
    assert_eq!(
        ServiceError::NotFound(7).public_message(),
        "user 7 not found"
    );
    // transparent adds no extra link.
    let inner = anyhow!("disk full").context("writing report");
    let t = ServiceError::from(inner);
    assert_eq!(chain_of(&t), ["writing report", "disk full"]);

    // A public error should be Send + Sync + 'static so it crosses threads and fits in anyhow. Check at compile time.
    fn good_citizen<E: StdError + Send + Sync + 'static>() {}
    good_citizen::<ConfigError>();
    good_citizen::<ServiceError>();
}

// ---------------- the application layer ----------------
fn run(path: &str) -> anyhow::Result<u64> {
    let config = load_config(path).with_context(|| format!("starting worker pool from {path}"))?;
    ensure!(
        config.workers % 2 == 0,
        "workers must be even, got {}",
        config.workers
    );
    if config.workers > 32 {
        bail!(
            "refusing to start {} workers on this machine",
            config.workers
        );
    }
    Ok(config.workers)
}

fn anyhow_layer() {
    let err = run("/no/such/file").unwrap_err();
    // {}  : only the outermost message.   {:#} : the whole chain on one line.
    assert_eq!(err.to_string(), "starting worker pool from /no/such/file");
    let one_line = format!("{err:#}");
    assert!(
        one_line
            .starts_with("starting worker pool from /no/such/file: cannot read /no/such/file: "),
        "{one_line}"
    );
    // {:?} : the multi-line report with "Caused by:".
    assert!(format!("{err:?}").contains("Caused by:"));
    assert_eq!(err.chain().count(), 3);
    assert_eq!(
        err.root_cause()
            .downcast_ref::<std::io::Error>()
            .map(|e| e.kind()),
        Some(std::io::ErrorKind::NotFound)
    );
    // Downcasting looks through context layers.
    assert!(err.is::<ConfigError>());
    assert_eq!(
        err.downcast_ref::<ConfigError>().map(ConfigError::code),
        Some("config.io")
    );
    let back: ConfigError = err.downcast().expect("takes ownership of the typed error");
    assert_eq!(back.code(), "config.io");

    let dir = tempfile::tempdir().unwrap();
    let write = |body: &str| {
        let p = dir.path().join("c.conf");
        std::fs::write(&p, body).unwrap();
        p.to_str().unwrap().to_owned()
    };
    assert_eq!(run(&write("workers=8")).unwrap(), 8);
    assert_eq!(
        run(&write("workers=7")).unwrap_err().to_string(),
        "workers must be even, got 7"
    );
    let e = run(&write("workers=40")).unwrap_err();
    assert!(e.to_string().contains("refusing to start 40"));
    assert!(
        e.downcast_ref::<ConfigError>().is_none(),
        "bail! creates a message-only error: callers cannot branch on it"
    );

    // Error::new keeps a typed error; Error::msg makes a text-only one. A context VALUE can itself be downcast to.
    #[derive(Debug, thiserror::Error)]
    #[error("while loading tenant {0}")]
    struct Tenant(u32);
    let e = anyhow::Error::new(ConfigError::Missing("x")).context(Tenant(5));
    assert_eq!(e.downcast_ref::<Tenant>().map(|t| t.0), Some(5));
    assert!(e.downcast_ref::<ConfigError>().is_some());
    assert_eq!(format!("{e:#}"), "while loading tenant 5: missing key `x`");
    assert_eq!(anyhow::Error::msg("plain").chain().count(), 1);

    // Option works with context too.
    let missing: anyhow::Result<u8> = None::<u8>.context("no leader elected");
    assert_eq!(missing.unwrap_err().to_string(), "no leader elected");

    // with_context is LAZY. context(format!(..)) would build the string even on success.
    let mut built = 0;
    let ok: anyhow::Result<u8> = Ok::<u8, std::io::Error>(1).with_context(|| {
        built += 1;
        "unused"
    });
    assert!(ok.is_ok() && built == 0);

    // The bug the header rule describes: Display includes the source AND source() returns it.
    #[derive(Debug, thiserror::Error)]
    #[error("parse failed: {source}")]
    struct Doubled {
        source: ParseIntError,
    }
    let e = anyhow::Error::new(Doubled {
        source: "x".parse::<u8>().unwrap_err(),
    });
    assert_eq!(
        format!("{e:#}").matches("invalid digit").count(),
        2,
        "{e:#}"
    );
}

/// Collect ALL problems instead of stopping at the first: errors are values.
fn validate_all(inputs: &[&str]) -> Result<Vec<Config>, Vec<(usize, ConfigError)>> {
    let mut ok = vec![];
    let mut bad = vec![];
    for (i, s) in inputs.iter().enumerate() {
        match parse_config(s) {
            Ok(c) => ok.push(c),
            Err(e) => bad.push((i, e)),
        }
    }
    if bad.is_empty() { Ok(ok) } else { Err(bad) }
}
fn batch_and_exit_codes() {
    let errs = validate_all(&["workers=1", "nope", "workers=99"]).unwrap_err();
    assert_eq!(
        errs.iter().map(|(i, e)| (*i, e.code())).collect::<Vec<_>>(),
        [(1, "config.syntax"), (2, "config.range")]
    );
    assert_eq!(validate_all(&["workers=2"]).unwrap().len(), 1);
    // A binary's boundary: turn the error into an exit code in ONE place.
    fn exit_code(e: &anyhow::Error) -> u8 {
        match e.downcast_ref::<ConfigError>() {
            Some(ConfigError::Io { .. }) => 74, // EX_IOERR
            Some(_) => 78,                      // EX_CONFIG
            None => 1,
        }
    }
    assert_eq!(exit_code(&run("/nope").unwrap_err()), 74);
    assert_eq!(
        exit_code(&anyhow::Error::new(ConfigError::Missing("w"))),
        78
    );
    assert_eq!(exit_code(&anyhow!("other")), 1);
}

// ---------------- anyhow: the edges ----------------
fn anyhow_edges() {
    // `anyhow::Ok` pins the error type when inference cannot, e.g. a closure that only uses `?`.
    let double = |s: &str| {
        let n: u32 = s.parse()?;
        anyhow::Ok(n * 2)
    };
    assert_eq!(double("21").unwrap(), 42);

    // An already boxed error cannot go through Error::new or `?` (the box type itself does not implement Error), so
    // anyhow has a named constructor. PITFALL: the type anyhow now remembers is the BOX, not what is inside it.
    type Boxed = Box<dyn StdError + Send + Sync>;
    let boxed: Boxed = Box::new(ConfigError::Missing("w"));
    let mut e = anyhow::Error::from_boxed(boxed);
    assert_eq!(e.to_string(), "missing key `w`");
    assert!(
        e.downcast_ref::<ConfigError>().is_none(),
        "one downcast is not enough"
    );
    // Downcast to the box first, then ask the trait object. downcast_mut edits the typed error in place.
    if let Some(ConfigError::Missing(key)) = e
        .downcast_mut::<Boxed>()
        .and_then(|b| b.downcast_mut::<ConfigError>())
    {
        *key = "workers";
    }
    assert_eq!(e.to_string(), "missing key `workers`");
    assert!(
        e.downcast_ref::<Boxed>()
            .is_some_and(|b| b.is::<ConfigError>())
    );

    // Leaving anyhow for an API that wants Box<dyn Error>. PITFALL: into_boxed_dyn_error (also what `?` and `.into()`
    // use) keeps anyhow's wrapper, so a later downcast to YOUR type fails.
    let make = || anyhow::Error::new(ConfigError::Missing("w"));
    let wrapped: Box<dyn StdError + Send + Sync> = make().into_boxed_dyn_error();
    assert_eq!(wrapped.to_string(), "missing key `w`");
    assert!(wrapped.downcast_ref::<ConfigError>().is_none());
    let via_into: Box<dyn StdError + Send + Sync> = make().into();
    assert!(via_into.downcast_ref::<ConfigError>().is_none());
    // If the receiver must downcast, re-box the original (any captured backtrace is dropped).
    let real = make().reallocate_into_boxed_dyn_error_without_backtrace();
    assert!(real.downcast_ref::<ConfigError>().is_some());
}

// ---------------- eyre: the same model with a pluggable report handler ----------------
/// What one report remembers besides the error: where it was created and notes added on the way up.
/// color-eyre's "sections" and suggestions are built on this same extension point.
struct NoteHandler {
    created_at: Option<&'static std::panic::Location<'static>>,
    notes: Vec<String>,
}
impl eyre::EyreHandler for NoteHandler {
    /// Used by `{:?}`, which is what `fn main() -> eyre::Result<()>` prints.
    fn debug(
        &self,
        error: &(dyn StdError + 'static),
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        for (i, cause) in chain_of(error).iter().enumerate() {
            writeln!(f, "{i}: {cause}")?;
        }
        for note in &self.notes {
            writeln!(f, "note: {note}")?;
        }
        Ok(())
    }
    fn track_caller(&mut self, location: &'static std::panic::Location<'static>) {
        self.created_at = Some(location);
    }
}
/// Install once, before the first Report is built. A second call is an error, so guard it with a Once.
fn install_eyre_handler() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        eyre::set_hook(Box::new(|_| {
            Box::new(NoteHandler {
                created_at: None,
                notes: vec![],
            })
        }))
        .expect("must run before any eyre::Report is created in this process");
    });
}
fn add_note(report: &mut eyre::Report, note: &str) {
    // The handler is a trait object: downcast it to reach your own fields. It is silently absent if someone else's
    // handler is installed, so treat notes as optional decoration.
    if let Some(h) = report.handler_mut().downcast_mut::<NoteHandler>() {
        h.notes.push(note.to_owned());
    }
}
fn eyre_run(path: &str) -> eyre::Result<u64> {
    use eyre::WrapErr;
    let config = load_config(path).wrap_err_with(|| format!("starting worker pool from {path}"))?;
    eyre::ensure!(
        config.workers % 2 == 0,
        "workers must be even, got {}",
        config.workers
    );
    if config.workers > 32 {
        eyre::bail!(
            "refusing to start {} workers on this machine",
            config.workers
        );
    }
    Ok(config.workers)
}
fn eyre_layer() {
    use eyre::{OptionExt, WrapErr};
    install_eyre_handler();
    assert!(
        eyre::set_hook(Box::new(eyre::DefaultHandler::default_with)).is_err(),
        "the hook cannot be replaced"
    );

    let mut report = eyre_run("/no/such/file").unwrap_err();
    // Display and {:#} follow the same convention as anyhow.
    assert_eq!(
        report.to_string(),
        "starting worker pool from /no/such/file"
    );
    assert!(
        format!("{report:#}")
            .starts_with("starting worker pool from /no/such/file: cannot read /no/such/file: ")
    );
    assert_eq!(report.chain().count(), 3);
    assert_eq!(
        report
            .root_cause()
            .downcast_ref::<std::io::Error>()
            .map(|e| e.kind()),
        Some(std::io::ErrorKind::NotFound)
    );
    assert_eq!(
        report.downcast_ref::<ConfigError>().map(ConfigError::code),
        Some("config.io")
    );
    // {:?} now goes through OUR handler.
    add_note(&mut report, "check the --config flag");
    let shown = format!("{report:?}");
    assert!(
        shown.starts_with(
            "0: starting worker pool from /no/such/file\n1: cannot read /no/such/file\n2: "
        ),
        "{shown}"
    );
    assert!(
        shown.ends_with("note: check the --config flag\n"),
        "{shown}"
    );
    // Wrapping keeps the SAME handler, so notes survive layers added later.
    let report = report.wrap_err("deploy failed");
    assert!(format!("{report:?}").contains("note: check the --config flag"));

    // track_caller records where the Report was created.
    let (r, line) = (eyre::eyre!("boom"), line!());
    let handler = r
        .handler()
        .downcast_ref::<NoteHandler>()
        .expect("our handler");
    let at = handler
        .created_at
        .expect("the default `track-caller` feature is on");
    assert_eq!((at.file(), at.line()), (file!(), line));

    // eyre! with a typed VALUE keeps the type; with a format string it makes a message-only report.
    assert!(
        eyre::eyre!(ConfigError::Missing("w"))
            .downcast_ref::<ConfigError>()
            .is_some()
    );
    assert!(
        eyre::eyre!("missing {}", "w")
            .downcast_ref::<ConfigError>()
            .is_none()
    );
    // Option: eyre prefers ok_or_eyre (a new error) over "context on nothing".
    assert_eq!(
        None::<u8>
            .ok_or_eyre("no leader elected")
            .unwrap_err()
            .to_string(),
        "no leader elected"
    );
    assert_eq!(
        Err::<u8, _>(std::fmt::Error)
            .wrap_err("rendering")
            .unwrap_err()
            .chain()
            .count(),
        2
    );
}

// ---------------- miette: diagnostics that point into the user's input ----------------
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("unknown dependency `{name}`")]
#[diagnostic(
    code(flow::unknown_dep),
    help("declare a task named `{name}` or remove the reference"),
    url("https://example.invalid/errors/unknown_dep")
)]
struct UnknownRef {
    name: String,
    #[source_code]
    src: miette::NamedSource<String>,
    /// BYTE offset and length into `src`, not characters and not line/column.
    #[label("not declared anywhere")]
    at: miette::SourceSpan,
}
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{} unknown reference(s)", problems.len())]
#[diagnostic(code(flow::invalid), severity(Error))]
struct Invalid {
    /// Report every problem at once, each with its own snippet.
    #[related]
    problems: Vec<UnknownRef>,
}
/// A tiny `name: dep dep` format. The point is that the checker keeps byte offsets while it parses.
fn check_refs(file: &str, text: &str) -> Result<(), Invalid> {
    let names: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(n, _)| n.trim())
        .collect();
    let mut problems = vec![];
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        if let Some((_, deps)) = line.split_once(':') {
            for dep in deps.split_whitespace().filter(|d| !names.contains(d)) {
                // `dep` is a subslice of `line`, so the pointer difference is its byte offset.
                let offset = line_start + (dep.as_ptr() as usize - line.as_ptr() as usize);
                problems.push(UnknownRef {
                    name: dep.into(),
                    src: miette::NamedSource::new(file, text.to_owned()),
                    at: (offset, dep.len()).into(),
                });
            }
        }
        line_start += line.len();
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(Invalid { problems })
    }
}
fn render(d: &dyn miette::Diagnostic) -> String {
    let mut out = String::new();
    // Rendering through an explicit handler avoids the process-wide miette hook and terminal detection.
    miette::NarratableReportHandler::new()
        .render_report(&mut out, d)
        .unwrap();
    out
}
fn miette_diagnostics() {
    use miette::Diagnostic as _;
    let text = "fetch:\nbuild: fetch\ntest: biuld fetch lint\n";
    assert!(check_refs("ok.flow", "a:\nb: a\n").is_ok());
    let err = check_refs("ci.flow", text).unwrap_err();
    assert_eq!(err.to_string(), "2 unknown reference(s)");
    let spans: Vec<&str> = err
        .problems
        .iter()
        .map(|p| &text[p.at.offset()..p.at.offset() + p.at.len()])
        .collect();
    assert_eq!(
        spans,
        ["biuld", "lint"],
        "the spans point at exactly the offending words"
    );
    // The trait gives machine-readable parts, which tests should prefer over rendered text.
    let first = &err.problems[0];
    assert_eq!(
        first.code().map(|c| c.to_string()).as_deref(),
        Some("flow::unknown_dep")
    );
    assert_eq!(
        first.help().map(|h| h.to_string()).as_deref(),
        Some("declare a task named `biuld` or remove the reference")
    );
    assert_eq!(
        first
            .labels()
            .unwrap()
            .map(|l| l.label().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["not declared anywhere"]
    );
    assert_eq!(err.related().unwrap().count(), 2);
    assert_eq!(err.severity(), Some(miette::Severity::Error));

    let shown = render(&err);
    for needle in [
        "Begin snippet for ci.flow",
        "snippet line 3: test: biuld fetch lint",
        "label at line 3, columns 7 to 11: not declared anywhere",
        "diagnostic code: flow::unknown_dep",
        "For more details, see:\nhttps://example.invalid/errors/unknown_dep",
    ] {
        assert!(shown.contains(needle), "missing {needle:?} in:\n{shown}");
    }
    // Plain `{}` shows only the message: the snippet exists only if a miette handler renders the diagnostic.
    assert!(!first.to_string().contains("snippet"));
    // A library can return spans WITHOUT owning the text; the caller attaches it at the boundary.
    #[derive(Debug, thiserror::Error, miette::Diagnostic)]
    #[error("bad token")]
    struct Bare {
        #[label("here")]
        at: miette::SourceSpan,
    }
    let report =
        miette::Report::new(Bare { at: (4, 3).into() }).with_source_code("abc def ghi".to_string());
    assert!(render(report.as_ref()).contains("columns 5 to 7: here"));
}

pub fn main() {
    // First, so that no eyre::Report can be created before the handler is installed.
    install_eyre_handler();
    thiserror_layer();
    anyhow_layer();
    anyhow_edges();
    batch_and_exit_codes();
    eyre_layer();
    miette_diagnostics();
    println!("error design: all sections passed");
}
