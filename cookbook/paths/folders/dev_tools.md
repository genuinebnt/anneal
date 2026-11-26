# Chapter `dev_tools`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`clap_cli`](../../examples/dev_tools/clap_cli.rs) | intermediate | a CLI with clap derive: subcommands, flags, enums, validation, and testing arg parsing. |
| 2 | [`testing_patterns`](../../examples/dev_tools/testing_patterns.rs) | intermediate | unit-test patterns: table-driven, Result-returning, should_panic, fakes, builders, seeded randomness. |
| 3 | [`tracing_logs`](../../examples/dev_tools/tracing_logs.rs) | intermediate | structured logging with `tracing`: events, spans, fields, levels. |
| 4 | [`shrinking`](../../examples/dev_tools/shrinking.rs) | advanced | property testing with SHRINKING (what quickcheck/proptest do for you). |
