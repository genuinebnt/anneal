//! RECIPE 1112 (crates): clap derive: subcommands, value enums, constraints, env fallback, custom parsers, completions, testing
//! LEVEL: intermediate
//! Use in: any command-line tool that needs help text, validation and shell completion.   Run: cargo run --release -p cb-crate-tour --example clap_derive_cli_tool
//! COMMON USES: cargo-style subcommand tools, ops utilities that read flags with environment fallbacks.
//! METHODS: clap::Parser::try_parse_from, clap::Parser::parse, clap::Subcommand, clap::Args, clap::ValueEnum, clap::ArgAction::{Count, Append}, #[arg(short, long)], #[arg(env)], #[arg(default_value_t)], #[arg(value_enum)], #[arg(value_parser)], clap::value_parser!, #[arg(value_delimiter)], #[arg(num_args)], #[arg(conflicts_with)], #[arg(requires)], #[arg(global)], #[arg(last)], #[command(flatten)], #[command(subcommand)], #[group(required, multiple)], clap::CommandFactory::command, Command::render_help, Command::render_long_help, Command::debug_assert, clap::Error::kind, clap::Error::exit_code, clap::error::ErrorKind, ArgMatches::value_source, clap::FromArgMatches::from_arg_matches, clap_complete::generate, clap_complete::Shell, #[arg(hide_env_values)], #[arg(required_unless_present)], #[arg(default_value_if)], #[arg(overrides_with)], #[arg(allow_negative_numbers)], #[arg(allow_hyphen_values, trailing_var_arg)], #[command(external_subcommand)], clap::builder::PossibleValuesParser, clap::builder::FalseyValueParser, clap::Parser::{parse_from, try_update_from}, clap::parser::ValueSource
//! DOCS: https://docs.rs/clap/4/clap/ https://docs.rs/clap/4/clap/_derive/index.html https://docs.rs/clap_complete/4/
//! CRATES: clap = "4" (features derive, env), clap_complete = "4"
//!
//! CHEAT SHEET
//!   flag                    -> bool field              repeatable -vvv   -> u8 with action = ArgAction::Count
//!   required value          -> T                       optional          -> Option<T>       list -> Vec<T>
//!   closed set of words     -> #[derive(ValueEnum)]    number in a range -> value_parser!(u16).range(1..)
//!   own syntax (10s, 5m)    -> value_parser = my_fn  (fn(&str) -> Result<T, String>)
//!   exactly one of A or B   -> an ArgGroup with required = true
//!   test the CLI            -> Cli::try_parse_from (never parse(): it EXITS the process on error)
//!
//! EXIT CODES: clap uses 2 for usage errors and 0 for --help and --version. Your own failures should use other codes.
//! ENVIRONMENT: `env = "NAME"` gives the precedence flag > environment > default. Setting a variable in a running process is
//! `unsafe` in edition 2024 (other threads may be reading the environment), so section `environment_fallback` instead runs
//! THIS executable again with `Command::env`: the child gets its own environment before it starts. The same trick shows the
//! real process exit codes of `Cli::parse`. The in-process asserts assume CB_FERRY_RECIPE_CONFIG is unset in the parent
//! (the name is unique to this recipe).
//!
//! Read: https://docs.rs/clap/4/clap/_derive/_tutorial/index.html ; https://clig.dev/
//!
//! TRY NEXT:
//!   - [S] Add a conflicting pair and predict the ErrorKind before running.
//!   - [M] Add a `--output <FILE>` that requires `--format json`; test all four combinations.
//!   - [A] PROJECT: with `tempfile_walkdir_glob_ignore`, build a `du`-like tool and test it with assert_cmd.
#![allow(dead_code)]
use clap::error::ErrorKind;
use clap::{
    ArgAction, ArgGroup, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum,
};
use clap_complete::Shell;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug, PartialEq)]
#[command(
    name = "ferry",
    version = "1.2.3",
    about = "Copy and serve artifacts",
    propagate_version = true
)]
struct Cli {
    /// Increase log verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = ArgAction::Count, global = true)]
    verbose: u8,
    /// Configuration file
    #[arg(
        long,
        global = true,
        env = "CB_FERRY_RECIPE_CONFIG",
        default_value = "ferry.toml"
    )]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Command {
    /// Copy files to a destination
    Copy(CopyArgs),
    /// Run the HTTP server
    Serve {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: SocketAddr,
        /// Ports 1-65535
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        metrics_port: Option<u16>,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value = "30s", value_parser = parse_duration)]
        timeout: Duration,
        #[arg(long, requires = "tls_key")]
        tls_cert: Option<PathBuf>,
        #[arg(long)]
        tls_key: Option<PathBuf>,
    },
    /// Run another program
    Exec {
        program: String,
        /// Everything after `--` is passed through untouched
        #[arg(last = true)]
        rest: Vec<String>,
    },
}

#[derive(Args, Debug, PartialEq)]
struct CopyArgs {
    /// Source files
    #[arg(required = true, num_args = 1..)]
    sources: Vec<PathBuf>,
    #[arg(long)]
    to: PathBuf,
    #[arg(long, value_enum, default_value_t = Mode::SkipExisting)]
    mode: Mode,
    #[arg(long, conflicts_with = "mode")]
    force: bool,
    /// Tags, repeatable or comma separated
    #[arg(long = "tag", value_delimiter = ',', action = ArgAction::Append, value_parser = parse_tag)]
    tags: Vec<(String, String)>,
    #[command(flatten)]
    auth: Auth,
}

/// Exactly one authentication method.
#[derive(Args, Debug, PartialEq)]
#[command(group(ArgGroup::new("auth").required(true).args(["token", "anonymous"])))]
struct Auth {
    #[arg(long)]
    token: Option<String>,
    #[arg(long)]
    anonymous: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
enum Mode {
    SkipExisting,
    Overwrite,
    #[value(name = "fail", alias = "strict")]
    FailOnExisting,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let split = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or("missing unit (ms, s or m)")?;
    let n: u64 = s[..split]
        .parse()
        .map_err(|_| format!("bad number in {s:?}"))?;
    match &s[split..] {
        "ms" => Ok(Duration::from_millis(n)),
        "s" => Ok(Duration::from_secs(n)),
        "m" => n
            .checked_mul(60)
            .map(Duration::from_secs)
            .ok_or_else(|| "too large".into()),
        unit => Err(format!("unknown unit {unit:?}")),
    }
}
fn parse_tag(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((k, v)) if !k.is_empty() => Ok((k.into(), v.into())),
        _ => Err("expected key=value".into()),
    }
}

fn parse(line: &str) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(line.split_whitespace()) // the first item is the program name
}
fn fails(line: &str, kind: ErrorKind) -> String {
    let e = parse(line).expect_err(line);
    assert_eq!(e.kind(), kind, "{line}: {e}");
    assert_eq!(e.exit_code(), 2, "usage errors exit with 2");
    assert!(e.use_stderr());
    e.to_string()
}

fn definition_is_consistent() {
    // Catches duplicate flags, bad `requires` names and similar. Put this in a unit test of every CLI.
    Cli::command().debug_assert();
}

fn parsing_subcommands_and_defaults() {
    let c = parse("ferry copy a b --to /d --anonymous").unwrap();
    assert_eq!((c.verbose, c.config.to_str()), (0, Some("ferry.toml")));
    let Command::Copy(copy) = c.command else {
        panic!()
    };
    assert_eq!(copy.sources, [PathBuf::from("a"), PathBuf::from("b")]);
    assert_eq!(
        (copy.mode, copy.force, copy.tags.len()),
        (Mode::SkipExisting, false, 0)
    );
    // global args may come before or after the subcommand.
    for line in ["ferry -vv --config x serve", "ferry serve -v --config x -v"] {
        let c = parse(line).unwrap();
        assert_eq!((c.verbose, c.config.to_str()), (2, Some("x")), "{line}");
    }
    let Command::Serve {
        listen,
        workers,
        timeout,
        metrics_port,
        ..
    } = parse("ferry serve").unwrap().command
    else {
        panic!()
    };
    assert_eq!(
        (listen, workers, timeout, metrics_port),
        (
            "127.0.0.1:8080".parse().unwrap(),
            4,
            Duration::from_secs(30),
            None
        )
    );
    // --k=v and -- passthrough.
    let Command::Exec { program, rest } = parse("ferry exec ls -- -la --verbose").unwrap().command
    else {
        panic!()
    };
    assert_eq!(
        (program.as_str(), rest),
        ("ls", vec!["-la".to_string(), "--verbose".to_string()])
    );
    let c = parse("ferry copy a --to=/d --token=t --tag k=1,m=2 --tag n=3 --mode strict").unwrap();
    let Command::Copy(copy) = c.command else {
        panic!()
    };
    assert_eq!(copy.mode, Mode::FailOnExisting);
    assert_eq!(
        copy.tags
            .iter()
            .map(|(k, v)| format!("{k}{v}"))
            .collect::<String>(),
        "k1m2n3"
    );
}

fn constraints_and_errors() {
    use ErrorKind::*;
    let msg = fails("ferry copy --to /d --anonymous", MissingRequiredArgument);
    assert!(msg.contains("<SOURCES>..."), "{msg}");
    fails("ferry copy a --to /d", MissingRequiredArgument); // group: need one
    fails(
        "ferry copy a --to /d --anonymous --token t",
        ArgumentConflict,
    ); // group: only one
    fails(
        "ferry copy a --to /d --anonymous --force --mode overwrite",
        ArgumentConflict,
    );
    // A DEFAULT value does not count as "present", so --force alone is fine.
    assert!(parse("ferry copy a --to /d --anonymous --force").is_ok());
    fails("ferry serve --tls-cert c.pem", MissingRequiredArgument);
    assert!(parse("ferry serve --tls-cert c --tls-key k").is_ok());
    assert!(
        parse("ferry serve --tls-key k").is_ok(),
        "requires is one-directional"
    );
    let msg = fails("ferry copy a --to /d --anonymous --mode nuke", InvalidValue);
    assert!(
        msg.contains("skip-existing, overwrite, fail"),
        "the possible values are listed: {msg}"
    );
    fails("ferry serve --metrics-port 0", ValueValidation);
    fails("ferry serve --metrics-port 70000", ValueValidation);
    fails("ferry serve --workers -1", UnknownArgument);
    let msg = fails("ferry serve --timeout 5h", ValueValidation);
    assert!(
        msg.contains("unknown unit \"h\""),
        "your parser's message is shown: {msg}"
    );
    fails("ferry serve --listen localhost", ValueValidation);
    let msg = fails("ferry sreve", InvalidSubcommand);
    assert!(msg.contains("serve"), "suggests the close match: {msg}");
    fails("ferry serve --wrokers 2", UnknownArgument);
    fails("ferry --config a --config b serve", ArgumentConflict); // a single-value flag given twice
}

fn help_version_and_value_sources() {
    let help = parse("ferry --help").unwrap_err();
    assert_eq!(
        (help.kind(), help.exit_code(), help.use_stderr()),
        (ErrorKind::DisplayHelp, 0, false)
    );
    let version = parse("ferry serve --version").unwrap_err(); // propagate_version
    assert_eq!(
        (version.kind(), version.exit_code()),
        (ErrorKind::DisplayVersion, 0)
    );
    assert!(version.to_string().contains("1.2.3"));
    let text = Cli::command().render_help().to_string();
    for needle in [
        "Copy and serve artifacts",
        "Usage: ferry [OPTIONS] <COMMAND>",
        "Copy files to a destination",
        "-v, --verbose",
        "Increase log verbosity",
        "CB_FERRY_RECIPE_CONFIG",
        "[default: ferry.toml]",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
    let mut cmd = Cli::command();
    let serve = cmd
        .find_subcommand_mut("serve")
        .unwrap()
        .render_long_help()
        .to_string();
    assert!(
        serve.contains("Ports 1-65535") && serve.contains("[default: 4]"),
        "{serve}"
    );
    // The declared env fallback, and where a value came from.
    let command = Cli::command();
    let arg = command
        .get_arguments()
        .find(|a| a.get_id() == "config")
        .unwrap();
    assert_eq!(
        arg.get_env().and_then(|e| e.to_str()),
        Some("CB_FERRY_RECIPE_CONFIG")
    );
    let source = |argv: &[&str]| {
        Cli::command()
            .try_get_matches_from(argv)
            .unwrap()
            .value_source("config")
    };
    assert_eq!(
        source(&["ferry", "--config", "x", "serve"]),
        Some(clap::parser::ValueSource::CommandLine)
    );
    assert_eq!(
        source(&["ferry", "serve"]),
        Some(clap::parser::ValueSource::DefaultValue)
    );
    // Drop to ArgMatches and back when you need both.
    let m = Cli::command()
        .try_get_matches_from(["ferry", "serve"])
        .unwrap();
    assert!(Cli::from_arg_matches(&m).is_ok());
}

fn completions() {
    for (shell, marker) in [
        (Shell::Bash, "complete -F"),
        (Shell::Zsh, "#compdef ferry"),
        (Shell::Fish, "complete -c ferry"),
        (Shell::PowerShell, "Register-ArgumentCompleter"),
    ] {
        let mut out = Vec::new();
        clap_complete::generate(shell, &mut Cli::command(), "ferry", &mut out);
        let script = String::from_utf8(out).unwrap();
        assert!(
            script.contains(marker) && script.contains("serve") && script.contains("metrics-port"),
            "{shell}"
        );
    }
}

fn duration_parser_properties() {
    for n in [0u64, 1, 59, 1000, 86_400] {
        assert_eq!(
            parse_duration(&format!("{n}ms")),
            Ok(Duration::from_millis(n))
        );
        assert_eq!(parse_duration(&format!("{n}s")), Ok(Duration::from_secs(n)));
        assert_eq!(
            parse_duration(&format!("{n}m")),
            Ok(Duration::from_secs(n * 60))
        );
    }
    for bad in [
        "",
        "10",
        "s",
        "1.5s",
        "-1s",
        "10 s",
        "99999999999999999999s",
        "18446744073709551615m",
    ] {
        assert!(parse_duration(bad).is_err(), "{bad:?}");
    }
}

// ---------------- more constraint attributes ----------------
#[derive(Parser, Debug, PartialEq)]
#[command(name = "tune")]
struct Tune {
    /// Needed unless --list is given
    #[arg(long, required_unless_present = "list")]
    name: Option<String>,
    #[arg(long)]
    list: bool,
    #[arg(long)]
    json: bool,
    /// A default that depends on another flag. Compare the VALUE "true" for a bool flag.
    #[arg(
        long,
        default_value = "table",
        default_value_if("json", "true", "json")
    )]
    format: String,
    /// The trap: a bool flag has an implicit default "false", so it can count as "present" when nobody typed it.
    #[arg(
        long,
        default_value = "table",
        default_value_if("json", clap::builder::ArgPredicate::IsPresent, "json")
    )]
    trap: String,
    /// The last one wins, which is what shell aliases such as `alias t='tune --color'` need
    #[arg(long, overrides_with = "no_color")]
    color: bool,
    #[arg(long = "no-color", overrides_with = "color")]
    no_color: bool,
    #[arg(long, allow_negative_numbers = true)]
    offset: Option<i64>,
    /// Allowed words without declaring an enum (the value stays a String)
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(["low", "high"]))]
    level: Option<String>,
}

/// A wrapper command: everything after the program name belongs to the wrapped program, including its flags.
#[derive(Parser, Debug, PartialEq)]
#[command(name = "wrap")]
struct Wrap {
    #[arg(short)]
    quiet: bool,
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    argv: Vec<String>,
}

/// git-style plugins: unknown subcommand names are captured instead of rejected.
#[derive(Parser, Debug, PartialEq)]
#[command(name = "host")]
struct Host {
    #[command(subcommand)]
    command: HostCommand,
}
#[derive(Subcommand, Debug, PartialEq)]
enum HostCommand {
    Status,
    #[command(external_subcommand)]
    Plugin(Vec<String>),
}

fn more_constraints() {
    let tune = |line: &str| Tune::try_parse_from(line.split_whitespace());
    assert_eq!(
        tune("tune").unwrap_err().kind(),
        ErrorKind::MissingRequiredArgument
    );
    assert!(tune("tune --list").is_ok());
    assert_eq!(tune("tune --list").unwrap().format, "table");
    assert_eq!(tune("tune --list --json").unwrap().format, "json");
    assert_eq!(
        tune("tune --list").unwrap().trap,
        "json",
        "PITFALL: IsPresent fired although --json was not typed"
    );
    assert_eq!(
        tune("tune --list --json --format csv").unwrap().format,
        "csv",
        "an explicit value beats the conditional default"
    );
    let t = tune("tune --list --color --no-color").unwrap();
    assert_eq!((t.color, t.no_color), (false, true));
    let t = tune("tune --list --no-color --color").unwrap();
    assert_eq!((t.color, t.no_color), (true, false));
    // Compare with `fails("ferry serve --workers -1", UnknownArgument)` above: a leading '-' looks like a flag.
    assert_eq!(tune("tune --list --offset -5").unwrap().offset, Some(-5));
    // `--flag=-5` also works without the attribute.
    let Command::Serve { .. } = parse("ferry serve --workers=3").unwrap().command else {
        panic!()
    };
    assert_eq!(
        parse("ferry serve --workers=-1").unwrap_err().kind(),
        ErrorKind::ValueValidation,
        "now it reaches the usize parser"
    );
    assert_eq!(
        tune("tune --list --level mid").unwrap_err().kind(),
        ErrorKind::InvalidValue
    );

    let w = Wrap::try_parse_from(["wrap", "-q", "ls", "-q", "--help"]).unwrap();
    assert!(w.quiet);
    assert_eq!(
        w.argv,
        ["ls", "-q", "--help"],
        "after the first positional, even --help belongs to the child"
    );

    let h = Host::try_parse_from(["host", "lint", "--fix", "a"]).unwrap();
    assert_eq!(
        h.command,
        HostCommand::Plugin(vec!["lint".into(), "--fix".into(), "a".into()])
    );
    // The price: a typo of a real subcommand is no longer an error.
    assert!(matches!(
        Host::try_parse_from(["host", "statsu"]).unwrap().command,
        HostCommand::Plugin(_)
    ));

    // try_update_from re-parses INTO an existing value. It is not a safe way to layer configuration:
    let mut t = tune("tune --name a --level low --format csv --json").unwrap();
    // 1. cross-argument rules are checked against the NEW line only.
    let e = t.try_update_from(["tune", "--level", "high"]).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::MissingRequiredArgument);
    // 2. arguments with a DEFAULT (including bool flags) are overwritten by that default even though nobody typed them.
    t.try_update_from(["tune", "--list", "--level", "high"])
        .unwrap();
    assert_eq!(
        (t.name.as_deref(), t.level.as_deref()),
        (Some("a"), Some("high")),
        "a plain Option survives"
    );
    assert_eq!(
        (t.format.as_str(), t.json),
        ("table", false),
        "csv and --json were lost"
    );
}

// ---------------- environment fallback, exercised in a child process ----------------
#[derive(Parser, Debug)]
#[command(name = "envdemo", version = "9.9.9")]
struct EnvCli {
    #[arg(long, env = "CB_FERRY_RECIPE_PORT", default_value_t = 80, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
    /// A secret: hide_env_values keeps the CURRENT value out of --help
    #[arg(
        long,
        env = "CB_FERRY_RECIPE_TOKEN",
        hide_env_values = true,
        conflicts_with = "anonymous"
    )]
    token: Option<String>,
    #[arg(long, env = "CB_FERRY_RECIPE_REGION")]
    region: Option<String>,
    #[arg(long)]
    anonymous: bool,
    #[arg(long, env = "CB_FERRY_RECIPE_STRICT_BOOL")]
    strict: bool,
    #[arg(long, env = "CB_FERRY_RECIPE_LAX_BOOL", value_parser = clap::builder::FalseyValueParser::new())]
    lax: bool,
    #[arg(long, env = "CB_FERRY_RECIPE_HOSTS", value_delimiter = ',')]
    hosts: Vec<String>,
}

const CHILD_MODE: &str = "CB_FERRY_RECIPE_CHILD";
const CHILD_ARGS: &str = "CB_FERRY_RECIPE_ARGV";
const CHILD_RESULT: &str = "CHILD_RESULT ";
const OWN_VARS: [&str; 8] = [
    "CONFIG",
    "PORT",
    "TOKEN",
    "REGION",
    "STRICT_BOOL",
    "LAX_BOOL",
    "HOSTS",
    "CHILD",
];

/// The child half. Does nothing unless the parent set CHILD_MODE. Uses `parse_from`, the exiting API a real `main` uses:
/// on a usage error or --help, clap prints and exits the process itself.
fn run_if_child() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };
    let argv = std::env::var(CHILD_ARGS).expect("parent always sets the argument line");
    let report = match mode.as_str() {
        "ferry" => {
            let matches = Cli::command().get_matches_from(argv.split_whitespace());
            let cli = Cli::from_arg_matches(&matches).expect("matches came from this command");
            format!(
                "config={} source={:?}",
                cli.config.display(),
                matches.value_source("config").expect("has a default")
            )
        }
        "envdemo" => {
            let c = EnvCli::parse_from(argv.split_whitespace());
            format!(
                "port={} token={:?} region={:?} strict={} lax={} hosts={:?}",
                c.port, c.token, c.region, c.strict, c.lax, c.hosts
            )
        }
        other => panic!("unknown child mode {other}"),
    };
    // The leading newline separates our line from libtest's "test ... " prefix when the child is the test harness.
    println!("\n{CHILD_RESULT}{report}");
    std::process::exit(0);
}

/// In the smoke-test binary `main` of this file is not the process entry point, so the child is started on this test.
/// In a normal run (variable unset) it returns immediately.
#[cfg(test)]
#[test]
fn child_entry() {
    run_if_child();
}

struct ChildOutput {
    code: i32,
    stdout: String,
    stderr: String,
}
impl ChildOutput {
    fn result(&self) -> &str {
        self.stdout
            .lines()
            .find_map(|l| l.strip_prefix(CHILD_RESULT))
            .unwrap_or_else(|| {
                panic!(
                    "no result line (exit {}):\n{}\n{}",
                    self.code, self.stdout, self.stderr
                )
            })
    }
}
/// Re-runs the current executable. `Command::env` changes only the CHILD's environment, so nothing here is unsafe and
/// parallel tests cannot observe each other's variables.
fn child(mode: &str, line: &str, vars: &[(&str, &str)]) -> ChildOutput {
    let mut cmd = std::process::Command::new(std::env::current_exe().expect("own path"));
    if cfg!(test) {
        // libtest names tests by module path without the crate name.
        let module = module_path!()
            .split_once("::")
            .map_or(String::new(), |(_, m)| format!("{m}::"));
        cmd.args([
            format!("{module}child_entry").as_str(),
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ]);
    }
    // Start from a known state: remove anything the user's shell may have set, then add this case's variables.
    for name in OWN_VARS {
        cmd.env_remove(format!("CB_FERRY_RECIPE_{name}"));
    }
    cmd.env(CHILD_MODE, mode).env(CHILD_ARGS, line);
    for (k, v) in vars {
        cmd.env(format!("CB_FERRY_RECIPE_{k}"), v);
    }
    let out = cmd.output().expect("spawn self");
    ChildOutput {
        code: out.status.code().expect("not killed by a signal"),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn environment_fallback() {
    // default < environment < command line, and value_source tells which one won.
    assert_eq!(
        child("ferry", "ferry serve", &[]).result(),
        "config=ferry.toml source=DefaultValue"
    );
    let from_env = [("CONFIG", "from-env.toml")];
    assert_eq!(
        child("ferry", "ferry serve", &from_env).result(),
        "config=from-env.toml source=EnvVariable"
    );
    for line in [
        "ferry --config flag.toml serve",
        "ferry serve --config flag.toml",
    ] {
        assert_eq!(
            child("ferry", line, &from_env).result(),
            "config=flag.toml source=CommandLine",
            "{line}"
        );
    }

    let plain = "port=80 token=None region=None strict=false lax=false hosts=[]";
    assert_eq!(child("envdemo", "envdemo", &[]).result(), plain);
    // Environment text goes through the same value_parser and value_delimiter as the flag.
    let ok = child("envdemo", "envdemo", &[("PORT", "8080"), ("HOSTS", "a,b")]);
    assert_eq!(
        ok.result(),
        r#"port=8080 token=None region=None strict=false lax=false hosts=["a", "b"]"#
    );
    let bad = child("envdemo", "envdemo", &[("PORT", "0")]);
    assert_eq!(
        (bad.code, bad.stdout.contains(CHILD_RESULT)),
        (2, false),
        "{}",
        bad.stderr
    );
    assert!(
        bad.stderr.contains("invalid value '0'") && bad.stderr.contains("--port"),
        "{}",
        bad.stderr
    );
    // A flag on the command line means the variable is not even parsed, so a broken value cannot hurt.
    assert!(
        child("envdemo", "envdemo --port 9", &[("PORT", "junk")])
            .result()
            .starts_with("port=9 ")
    );
    // PITFALL: set-but-empty is NOT the same as unset. `export REGION=` supplies the empty string.
    assert!(
        child("envdemo", "envdemo", &[("REGION", "")])
            .result()
            .contains(r#"region=Some("")"#)
    );
    let empty_port = child("envdemo", "envdemo", &[("PORT", "")]);
    assert_eq!(
        empty_port.code, 2,
        "and it does not fall back to the default: {}",
        empty_port.stderr
    );

    // PITFALL: a value from the environment counts as PRESENT for conflicts and requirements. A token exported in a
    // shell profile makes `--anonymous` fail even though the user typed no --token.
    let clash = child("envdemo", "envdemo --anonymous", &[("TOKEN", "hunter2")]);
    assert_eq!(clash.code, 2);
    assert!(
        clash.stderr.contains("cannot be used with"),
        "{}",
        clash.stderr
    );
    assert!(
        !clash.stderr.contains("hunter2"),
        "the error does not echo the secret"
    );

    // bool flags: the default parser for an env-backed flag accepts only "true"/"false" ...
    assert!(
        child("envdemo", "envdemo", &[("STRICT_BOOL", "true")])
            .result()
            .contains("strict=true")
    );
    assert!(
        child("envdemo", "envdemo", &[("STRICT_BOOL", "false")])
            .result()
            .contains("strict=false")
    );
    assert_eq!(child("envdemo", "envdemo", &[("STRICT_BOOL", "1")]).code, 2);
    // ... FalseyValueParser treats n, no, f, false, off, 0 and "" as false and anything else as true.
    for (text, want) in [
        ("1", true),
        ("yes", true),
        ("anything", true),
        ("0", false),
        ("off", false),
        ("NO", false),
        ("", false),
    ] {
        let out = child("envdemo", "envdemo", &[("LAX_BOOL", text)]);
        assert!(
            out.result().contains(&format!("lax={want}")),
            "{text:?}: {}",
            out.result()
        );
    }

    // --help shows CURRENT values of env variables unless told not to: a classic way to leak a secret into CI logs.
    let help = child(
        "envdemo",
        "envdemo --help",
        &[("TOKEN", "hunter2"), ("REGION", "eu-1")],
    );
    assert_eq!(help.code, 0, "--help is success");
    assert!(
        help.stdout.contains("[env: CB_FERRY_RECIPE_REGION=eu-1]"),
        "{}",
        help.stdout
    );
    assert!(
        help.stdout.contains("[env: CB_FERRY_RECIPE_TOKEN]") && !help.stdout.contains("hunter2"),
        "{}",
        help.stdout
    );

    // Real exit codes and streams of the exiting API.
    let version = child("envdemo", "envdemo --version", &[]);
    assert_eq!(version.code, 0);
    assert!(version.stdout.contains("envdemo 9.9.9"));
    let usage = child("envdemo", "envdemo --nope", &[]);
    assert_eq!(usage.code, 2);
    assert!(
        usage.stderr.contains("unexpected argument '--nope'")
            && !usage.stdout.contains("unexpected"),
        "usage errors go to stderr"
    );
}

pub fn main() {
    // Must come first: when this process is a child, it does the child's job and exits.
    run_if_child();
    definition_is_consistent();
    Tune::command().debug_assert();
    Wrap::command().debug_assert();
    Host::command().debug_assert();
    EnvCli::command().debug_assert();
    parsing_subcommands_and_defaults();
    constraints_and_errors();
    help_version_and_value_sources();
    completions();
    duration_parser_properties();
    more_constraints();
    environment_fallback();
    println!("clap: all sections passed");
}
