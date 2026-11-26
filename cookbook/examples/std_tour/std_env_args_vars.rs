//! RECIPE 1011 (std): std::env (args, environment variables, process paths), a hand-written CLI parser, layered config,
//! and the FFI string types OsStr/OsString/CStr/CString.
//! LEVEL: intermediate
//! Use in: CLI tools, 12-factor service config, container entrypoints, passing paths and names to the OS or C.   Run: cargo run --release --example std_env_args_vars
//! COMMON USES: DATABASE_URL / PORT / RUST_LOG, `--flag=value` parsing, `--` pass-through to a child command,
//! config precedence (defaults < file < env < flags), non-UTF-8 file names.
//! METHODS: (everyday first) std::env::args, std::env::var, std::env::VarError, std::env::args_os, std::env::var_os, std::env::vars, std::env::vars_os,
//! METHODS: std::env::current_dir, std::env::temp_dir, std::env::current_exe, std::env::set_var, std::env::remove_var, std::env::split_paths,
//! METHODS: std::env::join_paths, std::env::home_dir, std::env::consts::OS, std::env::consts::ARCH, std::env, std::option_env,
//! METHODS: std::process::Command::env, Command::env_remove, Command::env_clear, Command::args, OsStr::new, OsStr::to_str, OsStr::to_string_lossy,
//! METHODS: OsStr::as_encoded_bytes, OsString::from, OsString::into_string, OsString::push, std::os::unix::ffi::OsStrExt::from_bytes,
//! METHODS: std::os::unix::ffi::OsStringExt::from_vec, std::ffi::CString::new, CString::into_bytes, CString::as_bytes_with_nul, CString::as_c_str,
//! METHODS: NulError::nul_position, CStr::from_bytes_with_nul, CStr::from_bytes_until_nul, CStr::to_str, CStr::to_bytes, CStr::count_bytes
//! DOCS: https://doc.rust-lang.org/std/env/index.html
//! DEFINED IN: `std::env`, `std::process`, `OsStr`/`OsString` and the unix extension traits are std only. `CStr` is core
//! (`core::ffi`), `CString` is alloc (`alloc::ffi`); `env!`/`option_env!` are compiler built-in macros exported by core.
//! Also: https://doc.rust-lang.org/std/ffi/index.html
//! NOT COVERED (and why): set_current_dir (process-global, racy in tests: use Command::current_dir), env::chdir-like helpers,
//! unsafe raw-pointer functions (CStr::from_ptr, CString::into_raw/from_raw: see the FFI chapter), from_encoded_bytes_unchecked (unsafe),
//! Windows OsStringExt (cannot run here).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   arguments, may be non-UTF-8 (robust)       -> args_os().skip(1)
//!   arguments as Strings (panics on bad UTF-8) -> args().skip(1)
//!   optional text setting                      -> env::var("X").ok()   (but that hides NotUnicode)
//!   distinguish unset from garbage             -> match env::var("X") { NotPresent / NotUnicode }
//!   path-valued variable                       -> var_os("X").map(PathBuf::from)
//!   value fixed at COMPILE time                -> env!("CARGO_PKG_VERSION") / option_env!("X")
//!   give a variable to a child process         -> Command::env (never set_var for this)
//!   testable config code                       -> take an args iterator and a `Fn(&str) -> Option<String>` lookup
//!   split or build PATH                        -> split_paths / join_paths
//!   OS string -> &str                          -> to_str() (None if invalid) or to_string_lossy() (replacement chars)
//!   string for a C API                         -> CString::new(s)  (fails on an interior NUL)
//!
//! SAFETY NOTE (std docs for set_var): `set_var`/`remove_var` are `unsafe` in edition 2024. They are sound in a
//! single-threaded program and on Windows. In a multi-threaded program on other systems the only sound option is not to
//! call them, because C code (even DNS lookup) reads the environment without synchronisation. `cargo test` is multi-threaded.
//!
//! Read: https://doc.rust-lang.org/std/env/index.html , https://doc.rust-lang.org/std/ffi/index.html
//! TRY NEXT:
//!   - [S] Run with `PORT=abc` in `resolve`'s lookup closure and check that the error names the SOURCE of the bad value.
//!   - [S] Add a `-v` flag that may repeat (`-vvv`) and extend the table of tests.
//!   - [M] Run this binary with a non-UTF-8 argument from the shell (`$'\xff'`) after changing `child` to use `args()`; observe the panic.
//!   - [M] Add a `file` layer parsed with split_once (std_str_search_split_parse) and report which layer supplied each field.
//!   - [A] PROJECT: a mini process runner: parse `run [--env K=V]... -- CMD ARGS` with this parser, execute with
//!         std_panic_process_exit_catch's Command helpers, and exit with the child's code via ExitCode.
use std::collections::{HashMap, HashSet};
use std::env::{self, VarError};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

const COVERED: &[&str] = &[
    "args",
    "var",
    "VarError",
    "args_os",
    "var_os",
    "vars",
    "vars_os",
    "current_dir",
    "temp_dir",
    "current_exe",
    "set_var",
    "remove_var",
    "split_paths",
    "join_paths",
    "home_dir",
    "OS",
    "ARCH",
    "env",
    "option_env",
    "env_remove",
    "env_clear",
    "to_str",
    "to_string_lossy",
    "as_encoded_bytes",
    "into_string",
    "push",
    "from_bytes",
    "from_vec",
    "new",
    "into_bytes",
    "as_bytes_with_nul",
    "as_c_str",
    "nul_position",
    "from_bytes_with_nul",
    "from_bytes_until_nul",
    "to_bytes",
    "count_bytes",
];

const CHILD_MARKER: &str = "STD_ENV_COOKBOOK_CHILD";

// ---- 1. Everyday core: argv into a typed command ---------------------------------------------------------------
#[derive(Debug, PartialEq)]
enum Cmd {
    Serve { port: Option<u16>, verbose: bool },
    Exec { argv: Vec<String> },
    Help,
}
#[derive(Debug, PartialEq)]
enum CliError {
    NoCommand,
    UnknownCommand(String),
    UnknownFlag(String),
    MissingValue(&'static str),
    BadValue { flag: &'static str, value: String },
    Unexpected(String),
}

/// `args` excludes the program name. Accepts `--port 80` and `--port=80`; `--` ends option parsing.
fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Cmd, CliError> {
    let mut it = args.into_iter();
    let cmd = it.next().ok_or(CliError::NoCommand)?;
    match cmd.as_str() {
        "help" | "--help" | "-h" => Ok(Cmd::Help),
        "serve" => {
            let (mut port, mut verbose) = (None, false);
            while let Some(a) = it.next() {
                let (flag, inline) = match a.split_once('=') {
                    Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
                    _ => (a.clone(), None),
                };
                match flag.as_str() {
                    "--verbose" | "-v" if inline.is_none() => verbose = true,
                    "--port" | "-p" => {
                        let v = inline
                            .or_else(|| it.next())
                            .ok_or(CliError::MissingValue("--port"))?;
                        port = Some(v.parse().map_err(|_| CliError::BadValue {
                            flag: "--port",
                            value: v,
                        })?);
                    }
                    f if f.starts_with('-') => return Err(CliError::UnknownFlag(a)),
                    _ => return Err(CliError::Unexpected(a)),
                }
            }
            Ok(Cmd::Serve { port, verbose })
        }
        "exec" => match it.next().as_deref() {
            // Everything after `--` belongs to the child, even things that look like our flags.
            Some("--") => Ok(Cmd::Exec { argv: it.collect() }),
            Some(other) => Err(CliError::Unexpected(other.into())),
            None => Err(CliError::MissingValue("--")),
        },
        other => Err(CliError::UnknownCommand(other.into())),
    }
}

fn argv(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

fn section_cli() {
    assert_eq!(
        parse(argv("serve --port 80 -v")),
        Ok(Cmd::Serve {
            port: Some(80),
            verbose: true
        })
    );
    assert_eq!(
        parse(argv("serve --port=81")),
        Ok(Cmd::Serve {
            port: Some(81),
            verbose: false
        })
    );
    assert_eq!(
        parse(argv("serve -p 1 -p 2")),
        Ok(Cmd::Serve {
            port: Some(2),
            verbose: false
        })
    ); // last one wins
    assert_eq!(
        parse(argv("exec -- ls --port -- -v")),
        Ok(Cmd::Exec {
            argv: argv("ls --port -- -v")
        })
    );
    assert_eq!(parse(argv("--help")), Ok(Cmd::Help));
    assert_eq!(parse(argv("")), Err(CliError::NoCommand));
    assert_eq!(
        parse(argv("srve")),
        Err(CliError::UnknownCommand("srve".into()))
    );
    assert_eq!(
        parse(argv("serve --prot 1")),
        Err(CliError::UnknownFlag("--prot".into()))
    );
    assert_eq!(
        parse(argv("serve --verbose=yes")),
        Err(CliError::UnknownFlag("--verbose=yes".into()))
    );
    assert_eq!(
        parse(argv("serve --port")),
        Err(CliError::MissingValue("--port"))
    );
    assert_eq!(
        parse(argv("serve --port 70000")),
        Err(CliError::BadValue {
            flag: "--port",
            value: "70000".into()
        })
    );
    assert_eq!(
        parse(argv("serve extra")),
        Err(CliError::Unexpected("extra".into()))
    );
    assert_eq!(
        parse(argv("exec ls")),
        Err(CliError::Unexpected("ls".into()))
    );
    // The real process: element 0 is the program name as the CALLER chose it. Do not trust it for security decisions.
    assert!(env::args().next().is_some());
    assert_eq!(env::args_os().count(), env::args().count());
    println!("1. CLI parsing ok");
}

// ---- 2. Layered config: defaults < file < env < flags ----------------------------------------------------------
#[derive(Debug, PartialEq)]
struct Config {
    port: u16,
    host: String,
    log: String,
}
#[derive(Debug, PartialEq)]
struct ConfigError {
    source: &'static str,
    value: String,
}

/// Pure: the environment is a lookup function, so tests never touch the real process.
fn resolve(
    file: &HashMap<&str, &str>,
    env_lookup: impl Fn(&str) -> Option<String>,
    flag_port: Option<u16>,
) -> Result<Config, ConfigError> {
    // An empty variable counts as unset here. That is a policy choice: std reports it as Ok("").
    let env_nonempty = |k: &str| env_lookup(k).filter(|v| !v.is_empty());
    let text = |env_key: &str, file_key: &str, default: &str| {
        env_nonempty(env_key)
            .or_else(|| file.get(file_key).map(|s| s.to_string()))
            .unwrap_or_else(|| default.to_string())
    };
    let parse_port = |source: &'static str, v: String| {
        v.parse::<u16>()
            .map_err(|_| ConfigError { source, value: v })
    };
    let port = match (flag_port, env_nonempty("APP_PORT"), file.get("port")) {
        (Some(p), _, _) => p,
        (None, Some(v), _) => parse_port("env APP_PORT", v)?, // a bad value is an ERROR, not a silent fallback
        (None, None, Some(v)) => parse_port("file port", v.to_string())?,
        (None, None, None) => 8080,
    };
    Ok(Config {
        port,
        host: text("APP_HOST", "host", "127.0.0.1"),
        log: text("APP_LOG", "log", "info"),
    })
}

fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string())
}

fn section_config() {
    let file = HashMap::from([("port", "9000"), ("host", "db.local")]);
    let none = HashMap::new();
    let d = resolve(&none, lookup(&[]), None).unwrap();
    assert_eq!(
        d,
        Config {
            port: 8080,
            host: "127.0.0.1".into(),
            log: "info".into()
        }
    );
    assert_eq!(resolve(&file, lookup(&[]), None).unwrap().port, 9000);
    assert_eq!(
        resolve(&file, lookup(&[("APP_PORT", "9100")]), None)
            .unwrap()
            .port,
        9100
    );
    assert_eq!(
        resolve(&file, lookup(&[("APP_PORT", "9100")]), Some(1))
            .unwrap()
            .port,
        1
    );
    assert_eq!(
        resolve(&file, lookup(&[("APP_PORT", ""), ("APP_HOST", "h")]), None)
            .unwrap()
            .port,
        9000
    );
    assert_eq!(
        resolve(&file, lookup(&[("APP_PORT", "x")]), None),
        Err(ConfigError {
            source: "env APP_PORT",
            value: "x".into()
        })
    );
    // In production the lookup is one line. `.ok()` also turns NotUnicode into "unset": decide whether that is acceptable.
    let _real = |k: &str| env::var(k).ok();
    println!("2. layered config ok");
}

// ---- 3. Reading the real environment ---------------------------------------------------------------------------
fn section_env_read() {
    assert_eq!(
        env::var("STD_ENV_COOKBOOK_SURELY_UNSET"),
        Err(VarError::NotPresent)
    );
    assert_eq!(env::var_os("STD_ENV_COOKBOOK_SURELY_UNSET"), None);
    // Names containing '=' or NUL are reported as NotPresent by var.
    assert_eq!(env::var("A=B"), Err(VarError::NotPresent));
    // set_var / remove_var: unsafe since edition 2024.
    let key = "STD_ENV_COOKBOOK_LOCAL";
    // SAFETY: called from main before this program creates any thread (see the SAFETY NOTE in the header).
    unsafe { env::set_var(key, "1") };
    assert_eq!(env::var(key).as_deref(), Ok("1"));
    assert!(env::vars().any(|(k, v)| k == key && v == "1")); // vars() panics if ANY variable is not Unicode
    assert!(env::vars_os().any(|(k, _)| k == OsStr::new(key))); // vars_os() never does
    // SAFETY: same as above: still single-threaded.
    unsafe { env::remove_var(key) };
    assert!(env::var_os(key).is_none());
    // Compile-time environment: baked into the binary.
    assert_eq!(env!("CARGO_PKG_NAME"), "flowforge-cookbook");
    assert_eq!(option_env!("STD_ENV_COOKBOOK_NOT_AT_BUILD_TIME"), None);
    assert!(!env::consts::OS.is_empty() && !env::consts::ARCH.is_empty());
    println!("3. environment access ok");
}

// ---- 4. Process paths ------------------------------------------------------------------------------------------
fn section_paths() {
    // All can fail or surprise: cwd may be deleted; temp_dir is only a path (not guaranteed to exist or be private).
    let cwd: PathBuf = env::current_dir().expect("cwd");
    assert!(cwd.is_absolute());
    let tmp = env::temp_dir(); // on unix it honours TMPDIR
    assert!(tmp.is_absolute());
    let exe = env::current_exe().expect("exe path"); // do not use it as a security anchor (see std docs)
    assert!(
        exe.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("std_env_args_vars"))
    );
    // home_dir: may be None. Older releases marked it deprecated; std docs note its Windows behaviour changed in 1.85.
    let _home: Option<PathBuf> = env::home_dir();
    // PATH handling without hand-splitting on ':' (';' on Windows).
    let joined = env::join_paths(["/opt/a/bin", "/usr/bin"].map(PathBuf::from)).unwrap();
    let back: Vec<PathBuf> = env::split_paths(&joined).collect();
    assert_eq!(
        back,
        [PathBuf::from("/opt/a/bin"), PathBuf::from("/usr/bin")]
    );
    #[cfg(unix)]
    assert!(env::join_paths(["/has:colon"]).is_err()); // cannot be represented
    println!("4. process paths ok");
}

// ---- 5. OsStr / OsString ---------------------------------------------------------------------------------------
fn section_os_str() {
    let os: &OsStr = OsStr::new("report.txt"); // free view of a &str
    assert_eq!(os.to_str(), Some("report.txt"));
    let mut s: OsString = OsString::from("log-");
    s.push("1");
    assert_eq!(s, "log-1");
    assert_eq!(s.clone().into_string(), Ok("log-1".to_string()));
    assert_eq!(os.as_encoded_bytes(), b"report.txt"); // since 1.74; a UTF-8 superset, not portable between platforms
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        // Unix names are arbitrary non-NUL bytes.
        let bad = OsStr::from_bytes(b"caf\xe9.txt");
        assert_eq!(bad.to_str(), None);
        assert_eq!(bad.to_string_lossy(), "caf\u{FFFD}.txt"); // lossy: NEVER use the result to open the file
        assert_eq!(bad.as_bytes(), b"caf\xe9.txt");
        let owned = OsString::from_vec(vec![0xff, b'a']);
        let returned = owned.clone().into_string().unwrap_err(); // failure hands the original back
        assert_eq!(returned, owned);
        // Extension checks work without any UTF-8 conversion.
        assert_eq!(
            std::path::Path::new(bad).extension(),
            Some(OsStr::new("txt"))
        );
    }
    println!("5. OsStr ok");
}

// ---- 6. CStr / CString -----------------------------------------------------------------------------------------
fn section_c_strings() {
    let c = CString::new("hello").unwrap(); // appends the terminating NUL
    assert_eq!(c.as_bytes_with_nul(), b"hello\0");
    assert_eq!(c.count_bytes(), 5); // since 1.79
    let view: &CStr = c.as_c_str();
    assert_eq!(
        (view.to_str(), view.to_bytes()),
        (Ok("hello"), &b"hello"[..])
    );
    assert_eq!(c.clone().into_bytes(), b"hello");
    // An interior NUL would silently truncate in C, so CString::new rejects it and reports where.
    let err = CString::new("ab\0cd").unwrap_err();
    assert_eq!(err.nul_position(), 2);
    // C string literals (since 1.77): checked at compile time, no allocation.
    let lit: &CStr = c"hi";
    assert_eq!(lit.to_bytes_with_nul(), b"hi\0");
    // Parsing buffers that came from C or a binary format.
    assert!(CStr::from_bytes_with_nul(b"ab\0").is_ok());
    assert!(CStr::from_bytes_with_nul(b"ab").is_err()); // no terminator
    assert!(CStr::from_bytes_with_nul(b"a\0b\0").is_err()); // interior NUL
    let field = *b"name\0\0\0\0"; // fixed-width, NUL-padded record field
    assert_eq!(
        CStr::from_bytes_until_nul(&field).map(CStr::to_bytes),
        Ok(&b"name"[..])
    ); // since 1.69
    assert!(CStr::from_bytes_until_nul(b"full").is_err());
    assert!(c"\xff".to_str().is_err()); // CStr has no encoding: UTF-8 must be checked
    // PITFALL: `CString::new(x).unwrap().as_ptr()` leaves a dangling pointer (lint: dangling_pointers_from_temporaries).
    // Bind the CString to a variable that outlives every use of the pointer.
    println!("6. C strings ok");
}

// ---- 7. Child processes: the safe way to set environment and pass arguments ------------------------------------
/// Runs inside the spawned copy of this binary and reports what it received.
fn child() {
    let args: Vec<String> = env::args_os().skip(1).map(|a| format!("{a:?}")).collect();
    let mut vars: Vec<String> = ["APP_PORT", "HOME", "DROP_ME"]
        .iter()
        .map(|k| {
            format!(
                "{k}={:?}",
                env::var_os(k).map(|v| v.to_string_lossy().into_owned())
            )
        })
        .collect();
    vars.sort();
    println!("{}|{}", args.join(","), vars.join(";"));
}

fn section_child() {
    let exe = env::current_exe().expect("exe path");
    let out = Command::new(&exe)
        .args(["--flag", "two words", "*"]) // no shell: no splitting, no globbing, no injection through a shell
        .env_clear() // start from an empty environment
        .env(CHILD_MARKER, "1")
        .env("APP_PORT", "7")
        .env("DROP_ME", "x")
        .env_remove("DROP_ME")
        .output()
        .expect("spawn self");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text.trim(),
        "\"--flag\",\"two words\",\"*\"|APP_PORT=Some(\"7\");DROP_ME=None;HOME=None"
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let out = Command::new(&exe)
            .arg(OsStr::from_bytes(b"\xff"))
            .env(CHILD_MARKER, "1")
            .output()
            .expect("spawn self");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("\"\\xFF\"|"));
    }
    println!("7. child environment ok");
}

// ---- 8. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Read configuration ONCE at start-up into a typed struct and pass it down; no env::var calls deep in the code.
    // 2. Fail fast on a bad value and name its source (section 2). Silent defaults hide typos for months.
    let e = resolve(&HashMap::new(), lookup(&[("APP_PORT", "80a")]), None).unwrap_err();
    assert_eq!(e.source, "env APP_PORT");
    // 3. Never log the whole environment or a config struct with secrets: redact in Debug (std_fmt_display_debug_deep).
    // 4. Do not mutate the environment to pass data between your own functions (see SAFETY NOTE).
    // 5. Secrets on the command line are visible to other users through the process list: prefer files or env.
    // 6. Use args_os for anything that may be a path; convert with PathBuf::from without going through String.
    let p = PathBuf::from(OsString::from("a/b"));
    assert!(p.ends_with("b"));
    // 7. Presence flags: `var_os("DEBUG").is_some()` is true even for DEBUG=0 or DEBUG= . Define accepted values.
    let truthy = |v: Option<&str>| matches!(v, Some("1" | "true" | "yes"));
    assert!(truthy(Some("1")) && !truthy(Some("0")) && !truthy(None));
    // 8. For real CLIs with help text and completion, use a parser crate (clap); hand-parse only tiny tools.
    // 9. Forbid accidental process-global access in a codebase with clippy's disallowed_methods configuration.
    println!("8. production tips ok");
}

fn main() {
    if env::var_os(CHILD_MARKER).is_some() {
        child();
        return;
    }
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_env_read(); // first: it mutates the environment while still single-threaded
    section_cli();
    section_config();
    section_paths();
    section_os_str();
    section_c_strings();
    section_child();
    section_production_tips();
    println!(
        "covered {} env/ffi-string names; all asserts passed",
        COVERED.len()
    );
}
