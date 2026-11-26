# cb-build-script: `build.rs`

Shows five things a build script does (see `build.rs`, each section numbered):
1. **Stamp the build**: `cargo::rustc-env=NAME=value` makes `env!("NAME")` work
   (git hash, `RUSTC --version`, `PROFILE`, `TARGET`, `HOST`).
2. **Probe for capabilities**: compile a snippet with `$RUSTC`; on success emit
   `cargo::rustc-cfg=has_x` and use `#[cfg(has_x)]`. This is what the `autocfg`
   crate does. Declare cfgs with `cargo::rustc-check-cfg` to keep lints quiet.
3. **Generate code** into `OUT_DIR` (a CRC-32 table) and `include!` it.
4. **Compile C** with the [`cc`](https://docs.rs/cc) crate and call it via `extern "C"`.
5. **`cargo::warning=`** surfaces a message in the build output (opt in with `CB_BUILD_WARN=1`).

## Lifecycle and caching
cargo compiles `build.rs`, runs it, parses its stdout directives, then compiles
the crate with the resulting cfgs/env. With no `rerun-if-*` lines cargo reruns
the script whenever any file in the package changes; once you emit one, only the
listed `rerun-if-changed` paths and `rerun-if-env-changed` variables trigger a
rerun. Try `CB_BUILD_LABEL=x cargo test ...` and watch the stamp change.

## Host vs target
The script is compiled for and runs on the **host**, even when cross-compiling.
So `cfg!(target_os = ..)` inside `build.rs` describes the host; read `TARGET`,
`CARGO_CFG_TARGET_OS`, etc. from the environment instead. Probes and `cc` must
use target-aware tools (`cc` does, via `TARGET`/`CC_<triple>`).

## Common uses
`prost`/`tonic` codegen, `bindgen`, linking system libs (`pkg-config`),
embedding version info, `-sys` crates.

Docs: <https://doc.rust-lang.org/cargo/reference/build-scripts.html>

## Run
`CARGO_TARGET_DIR=/tmp/cb_target_shared cargo test -q --release -p cb-build-script`
