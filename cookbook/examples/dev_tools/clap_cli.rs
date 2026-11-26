//! RECIPE 54 (tooling): a CLI with clap derive: subcommands, flags, enums, validation, and testing arg parsing.
//! LEVEL: intermediate
//! Use in: the CLI-client stage, any tool with more than one command.   Run: cargo run --release --example clap_cli
//! COMMON USES: every command-line tool: subcommands, flags, validation, completion, testable arg parsing.
#![allow(dead_code)]
//!
//! Derive a struct/enum and clap generates parsing, `--help`, usage errors, and shell completions. Keep `main` thin:
//! parse -> call a function -> map Result to an exit code (error_enum). Use `try_parse_from(args)` in tests: it returns
//! an error instead of exiting the process.
//! Docs: https://docs.rs/clap/latest/clap/_derive/index.html
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "recipebook", version, about = "Manage recipes")]
struct Cli {
    /// Print extra detail (global: works before or after the subcommand)
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Add a recipe from a file
    Add {
        path: PathBuf,
        /// Cooking time in minutes
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=600))]
        minutes: u32,
        #[arg(long, value_enum, default_value_t = Course::Main)]
        course: Course,
        /// Repeatable: --tag a --tag b
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// List recipes
    List {
        #[arg(long)]
        json: bool,
        /// Only this many (optional value)
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },
}

#[derive(ValueEnum, Clone, Debug, PartialEq)]
enum Course {
    Starter,
    Main,
    Dessert,
}

fn main() {
    let cases: [&[&str]; 6] = [
        &[
            "recipebook",
            "add",
            "soup.md",
            "--minutes",
            "45",
            "--tag",
            "quick",
            "--tag",
            "veg",
            "-v",
        ],
        &["recipebook", "list", "--json", "-n", "5"],
        &["recipebook", "add", "x.md", "--course", "dessert"],
        &["recipebook", "add", "x.md", "--minutes", "0"], // out of range
        &["recipebook", "add"],                           // missing path
        &["recipebook", "frobnicate"],                    // unknown command
    ];
    for args in cases {
        match Cli::try_parse_from(args) {
            Ok(cli) => println!("OK  {cli:?}"),
            Err(e) => println!("ERR {}", e.to_string().lines().next().unwrap_or("")),
        }
    }
    // In a real main: `let cli = Cli::parse();` exits with a nice message on bad input.
}
