# cb-macros + cb-macros-demo: procedural macros

Two crates: `cb-macros` (the `proc-macro = true` crate) and `cb-macros-demo`
(a consumer whose tests exercise it). Demonstrates every kind of proc macro:

| kind | here |
|---|---|
| derive | `#[derive(Describe)]`, `#[derive(Builder)]` |
| attribute | `#[timed]`, `#[memoize]` |
| function-like | `hashmap!{ k => v }`, `const_assert!(cond, "msg")` |

## Concepts
- **Token pipeline**: rustc hands the macro a `proc_macro::TokenStream`;
  `syn` parses it into a typed AST (`DeriveInput`, `ItemFn`, `Expr`);
  you build output with `quote!` (using `proc_macro2` types, which also work in
  unit tests); rustc re-parses the returned stream as ordinary code.
- **Errors**: never `panic!`. Return `syn::Error::new_spanned(tokens, msg)
  .to_compile_error()` so the error points at the user's code. `quote_spanned!`
  makes generated code report at a chosen span.
- **Hygiene**: `macro_rules!` is hygienic for locals; proc macros are not by
  default (`Span::call_site`). Use fully qualified paths (`::std::...`) and
  unlikely local names (`__start`) in generated code.
- **Generics**: always `let (impl_g, ty_g, where_c) = generics.split_for_impl()`.
- **Why a separate crate?** A proc macro is compiled to a dylib that rustc
  loads and runs at compile time on the *host*; a crate with `proc-macro = true`
  can export nothing but macros, so shared types go in a third crate.
- **Debugging**: [cargo-expand](https://github.com/dtolnay/cargo-expand) shows
  the generated code (`cargo expand -p cb-macros-demo --test behaviour`).
- **Compile-fail tests**: [trybuild](https://github.com/dtolnay/trybuild)
  snapshots rustc's stderr in `tests/ui/*.stderr`. Snapshots can drift between
  compiler versions; regenerate with `TRYBUILD=overwrite cargo test`.

## Run
`CARGO_TARGET_DIR=/tmp/cb_target_shared cargo test --release -p cb-macros-demo`

## Common uses
serde/clap/thiserror derives, `#[tokio::main]`, `#[tracing::instrument]`,
`sqlx::query!`.
