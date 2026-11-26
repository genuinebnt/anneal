//! `pebble-tools`: formatter, highlighter, editor-file generator and language server.
//!
//! Exit codes: 0 success; 1 the input has errors (fmt on a file with syntax errors,
//! `fmt --check` on an unformatted file); 2 usage or I/O error.

use pebble_tools::{fmt, highlight, lsp, textmate};
use std::io::{BufReader, Write};
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: pebble-tools <command> [options]

commands:
  fmt FILE [--check] [--best-effort]  format FILE in place (--check: only report). Strict by default:
                                      a file with syntax errors is refused (exit 1). --best-effort formats
                                      the error-free items and keeps error regions verbatim
  highlight FILE [--format=ansi|html|json] [--semantic]
                                      print FILE with syntax highlighting (default: ansi)
  lsp [--sync=full|incremental]       run the language server on stdin/stdout
  gen-editors [DIR]                   (re)generate editors/ (TextMate grammar, VS Code extension)";

fn main() -> ExitCode {
    // The checker recurses on nested programs: give every command a big stack.
    match std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(real_main)
        .map(|h| h.join())
    {
        Ok(Ok(code)) => ExitCode::from(code),
        _ => {
            eprintln!("pebble-tools: internal failure");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> u8 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("{USAGE}");
        return 2;
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "fmt" => cmd_fmt(rest),
        "highlight" => cmd_highlight(rest),
        "lsp" => cmd_lsp(rest),
        "gen-editors" => cmd_gen_editors(rest),
        "-h" | "--help" => {
            println!("{USAGE}");
            0
        }
        other => {
            eprintln!("pebble-tools: unknown command `{other}`\n{USAGE}");
            2
        }
    }
}

fn split_args(rest: &[String]) -> (Vec<&str>, Vec<&str>) {
    let (flags, pos): (Vec<&String>, Vec<&String>) = rest.iter().partition(|a| a.starts_with("--"));
    (
        flags.into_iter().map(|s| s.as_str()).collect(),
        pos.into_iter().map(|s| s.as_str()).collect(),
    )
}

fn read(path: &str) -> Result<String, u8> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("pebble-tools: cannot read `{path}`: {e}");
        2
    })
}

fn cmd_fmt(rest: &[String]) -> u8 {
    let (flags, pos) = split_args(rest);
    let [path] = pos.as_slice() else {
        eprintln!("{USAGE}");
        return 2;
    };
    let check = flags.contains(&"--check");
    let Ok(src) = read(path) else { return 2 };
    let best_effort = flags.contains(&"--best-effort");
    let out = match fmt::format(&src) {
        Ok(o) => o,
        Err(fmt::FormatError::Syntax(_)) if best_effort => fmt::format_best_effort(&src).text,
        Err(e) => {
            eprintln!("pebble-tools: {path}: {e}");
            return 1;
        }
    };
    if check {
        if out != src {
            eprintln!("pebble-tools: {path} is not formatted");
            return 1;
        }
        return 0;
    }
    if out != src
        && let Err(e) = std::fs::write(path, out)
    {
        eprintln!("pebble-tools: cannot write `{path}`: {e}");
        return 2;
    }
    0
}

fn cmd_highlight(rest: &[String]) -> u8 {
    let (flags, pos) = split_args(rest);
    let [path] = pos.as_slice() else {
        eprintln!("{USAGE}");
        return 2;
    };
    let Ok(src) = read(path) else { return 2 };
    let toks = if flags.contains(&"--semantic") {
        let an = pebble_tools::analysis::analyze(&src);
        highlight::semantic(&src, &an)
    } else {
        highlight::lexical(&src)
    };
    let out = match flags
        .iter()
        .find_map(|f| f.strip_prefix("--format="))
        .unwrap_or("ansi")
    {
        "ansi" => highlight::render_ansi(&src, &toks),
        "html" => highlight::render_html(&src, &toks, path),
        "json" => highlight::render_json(&src, &toks),
        other => {
            eprintln!("pebble-tools: unknown format `{other}`");
            return 2;
        }
    };
    let _ = std::io::stdout().write_all(out.as_bytes());
    0
}

fn cmd_lsp(rest: &[String]) -> u8 {
    let (flags, _) = split_args(rest);
    let sync = if flags.contains(&"--sync=full") {
        lsp::SyncKind::Full
    } else {
        lsp::SyncKind::Incremental
    };
    let mut server = lsp::Server::new(sync);
    let stdin = BufReader::new(std::io::stdin());
    let mut stdout = std::io::stdout().lock();
    lsp::serve(&mut server, stdin, &mut stdout, Duration::from_millis(15)) as u8
}

fn cmd_gen_editors(rest: &[String]) -> u8 {
    let dir = rest
        .first()
        .map(String::as_str)
        .unwrap_or(env!("CARGO_MANIFEST_DIR"));
    for (rel, content) in textmate::editor_files() {
        let path = std::path::Path::new(dir).join(rel);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("pebble-tools: cannot write {}: {e}", path.display());
            return 2;
        }
        println!("wrote {}", path.display());
    }
    0
}
