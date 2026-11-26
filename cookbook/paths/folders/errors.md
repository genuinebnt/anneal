# Chapter `errors`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`anyhow_context`](../../examples/errors/anyhow_context.rs) | simple | `anyhow`: context chains for application-level errors, and downcasting back to the cause. |
| 2 | [`error_enum`](../../examples/errors/error_enum.rs) | simple | one error enum, `?` everywhere, and a thin `main`. |
| 3 | [`option_result_toolbox`](../../examples/errors/option_result_toolbox.rs) | simple | the Option/Result combinator toolbox (and when to just use `match` or `?`). |
| 4 | [`diagnostics_spans`](../../examples/errors/diagnostics_spans.rs) | intermediate | compiler-style diagnostics: byte spans, line/column, and a caret under the problem. |
| 5 | [`error_chain_retryable`](../../examples/errors/error_chain_retryable.rs) | intermediate | library errors with `#[source]`, error chains, and classifying errors (retryable vs fatal). |
