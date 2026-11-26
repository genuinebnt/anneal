# Error Handling

Error enums, context chains, retryable errors, diagnostics with spans, Option/Result combinators.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

5 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`error_enum`](error_enum.rs) | one error enum, `?` everywhere, and a thin `main` | simple | every CLI/service: one error enum per crate, `?` everywhere, only `main` maps errors to exit codes or HTTP statuses. | serde-json, thiserror |
| [`anyhow_context`](anyhow_context.rs) | `anyhow`: context chains for application-level errors, and downcasting back to the cause | simple | application glue code and CLIs: add context at each layer, report the whole chain. | anyhow, serde-json |
| [`error_chain_retryable`](error_chain_retryable.rs) | library errors with `#[source]`, error chains, and classifying errors (retryable vs fatal) | intermediate | retry/dead-letter decisions in workers; error chains in logs. | thiserror |
| [`diagnostics_spans`](diagnostics_spans.rs) | compiler-style diagnostics: byte spans, line/column, and a caret under the problem | intermediate | compilers, linters, config validators, query parsers: errors that point at the exact bytes. | std only |
| [`option_result_toolbox`](option_result_toolbox.rs) | the Option/Result combinator toolbox (and when to just use `match` or `?`) | simple | all fallible code: the right combinator or `?` instead of unwrap. | std only |
