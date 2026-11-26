# Development Tools

Testing patterns, property testing, CLI parsing, structured logging.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

4 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`shrinking`](shrinking.rs) | property testing with SHRINKING (what quickcheck/proptest do for you) | advanced | finding minimal failing inputs in any property test (proptest/quickcheck do this automatically). | std only |
| [`testing_patterns`](testing_patterns.rs) | unit-test patterns: table-driven, Result-returning, should_panic, fakes, builders, seeded randomness | intermediate | every codebase: table-driven tests, injected fakes, seeded property tests. | std only |
| [`clap_cli`](clap_cli.rs) | a CLI with clap derive: subcommands, flags, enums, validation, and testing arg parsing | intermediate | every command-line tool: subcommands, flags, validation, completion, testable arg parsing. | clap |
| [`tracing_logs`](tracing_logs.rs) | structured logging with `tracing`: events, spans, fields, levels | intermediate | production services: structured logs with request/job ids, spans across await points, later OpenTelemetry. | tokio, tracing, tracing-subscriber |
