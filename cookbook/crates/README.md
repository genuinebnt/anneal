# Multi-crate recipes

Some Rust concepts cannot live in a single example file: procedural macros (`proc-macro = true`), `build.rs` scripts, cargo features and
workspaces, `#![no_std]`, and C-ABI libraries (`cdylib`). Each directory here is a workspace member with its own `Cargo.toml`, a
`description`, a `README.md` explaining the concept, and tests that verify it (`cargo test -p <crate>`; `./check.sh` runs them all).
