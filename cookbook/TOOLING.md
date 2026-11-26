# Rust Tooling Field Guide for Systems Work

Every link below returned HTTP 200 when checked. Install commands are in the checklist at the end.

## 1. Install first (checklist)

1. `rustup` (https://rust-lang.github.io/rustup/): manages toolchains. Add `clippy`, `rustfmt` and `rust-src`, and install a nightly (`rustup toolchain install nightly`) for Miri and sanitizers.
2. `cargo install cargo-nextest cargo-deny cargo-audit cargo-expand cargo-show-asm cargo-bloat flamegraph samply cargo-semver-checks --locked` (install what you need, not all at once).
3. `cargo install cargo-binstall` (https://github.com/cargo-bins/cargo-binstall) to fetch prebuilt binaries instead of compiling each tool.
4. Platform profiler: `perf` on Linux, Instruments (ships with Xcode) on macOS, `samply` on both.
5. Copy CI from https://github.com/jonhoo/rust-ci-conf.

## 2. Everyday cargo

| Tool | When to use | Command |
|---|---|---|
| [cargo check](https://doc.rust-lang.org/cargo/commands/cargo-check.html) | Fast type-check loop without codegen. | `cargo check --all-targets` |
| [cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html) | Run unit, integration and doc tests. | `cargo test --workspace` |
| [clippy](https://doc.rust-lang.org/clippy/) | Catch common bugs and non-idiomatic code before review. | `cargo clippy --all-targets -- -D warnings` |
| [rustfmt](https://github.com/rust-lang/rustfmt) | Keep formatting uniform and diffs small. | `cargo fmt --check` |
| [cargo tree](https://doc.rust-lang.org/cargo/commands/cargo-tree.html) | Find why a dependency (or a duplicate version) is in the build. | `cargo tree -d` or `cargo tree -i <crate>` |
| [cargo-expand](https://github.com/dtolnay/cargo-expand) | See what a macro or derive generates. | `cargo expand path::to::module` |
| [cargo-show-asm](https://github.com/pacak/cargo-show-asm) | Check what the compiler did to one function (inlining, vectorisation). | `cargo asm --lib crate::module::func` |
| [cargo-bloat](https://github.com/RazrFalcon/cargo-bloat) | Find which functions and crates dominate binary size. | `cargo bloat --release --crates` |
| [cargo-udeps](https://github.com/est31/cargo-udeps) | Find unused dependencies (needs nightly). | `cargo +nightly udeps --all-targets` |
| [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | Enforce licence, ban and advisory policy in CI. | `cargo deny check` |
| [cargo-audit (RustSec)](https://github.com/rustsec/rustsec) | Scan `Cargo.lock` for known vulnerabilities. | `cargo audit` |
| [cargo-outdated](https://github.com/kbknapp/cargo-outdated) | See which dependencies have newer releases. | `cargo outdated -R` |

## 3. Workspaces and features

- [Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html): use when several crates share one lockfile and target directory. Run `cargo build -p <member>` for one crate, `--workspace` for all.
- [Features](https://doc.rust-lang.org/cargo/reference/features.html): use for optional dependencies and compile-time switches. Features must be additive. Test combinations with `cargo test --no-default-features` and `cargo test --all-features`.
- Put shared versions in `[workspace.dependencies]` and inherit with `dep = { workspace = true }`.

## 4. Build profiles

Reference: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) and [rustc codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html).

| Knob | When to use | Setting |
|---|---|---|
| LTO | Smaller and faster release binaries, at the cost of link time. | `[profile.release] lto = "fat"` (or `"thin"`) |
| codegen-units | Fewer units give more optimisation, slower builds. | `codegen-units = 1` |
| panic | Smaller binaries and no unwinding tables when you never catch panics. | `panic = "abort"` |
| Debug symbols for profiling | Readable flamegraphs from a release build. | `[profile.release] debug = "line-tables-only"` |
| Native CPU | Benchmarking on the machine you run on only; binaries will not be portable. | `RUSTFLAGS="-C target-cpu=native" cargo build --release` |
| PGO | Squeeze extra speed using real workload profiles ([cargo-pgo](https://github.com/Kobzol/cargo-pgo)). | `cargo pgo build`, run the workload, then `cargo pgo optimize` |

## 5. Testing

| Tool | When to use | Command |
|---|---|---|
| [cargo-nextest](https://nexte.st/) | Faster, isolated-per-test runner with better output. Does not run doc tests. | `cargo nextest run` |
| [proptest](https://github.com/proptest-rs/proptest) | Generate inputs and shrink failures for invariants and parsers. | add as dev-dependency |
| [quickcheck](https://github.com/BurntSushi/quickcheck) | Lighter property testing driven by type-based generators. | add as dev-dependency |
| [insta](https://insta.rs/) | Snapshot tests for large or structured outputs. | `cargo insta review` |
| [criterion](https://github.com/bheisler/criterion.rs) | Statistical benchmarks with regression detection. | `cargo bench` |
| [divan](https://github.com/nvzqz/divan) | Simpler attribute-based benchmarks. | `cargo bench` |
| [Doc tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html) | Keep examples in docs compiling and correct. | `cargo test --doc` |
| [Miri](https://github.com/rust-lang/miri) | Detect undefined behaviour in `unsafe` code (aliasing, uninitialised memory, leaks). | `rustup +nightly component add miri`, then `cargo +nightly miri test` |
| [loom](https://github.com/tokio-rs/loom) | Exhaustively explore thread interleavings of lock-free code. | `RUSTFLAGS="--cfg loom" cargo test --release` |
| [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) | Coverage-guided fuzzing with libFuzzer for parsers and decoders. | `cargo +nightly fuzz run <target>` |
| [Sanitizers](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html) | Catch memory errors (address) or data races (thread) in native code and FFI. | `RUSTFLAGS=-Zsanitizer=address cargo +nightly test --target <host-triple>` |

Notes: Miri is slow and cannot do most FFI or I/O. Sanitizers need nightly and an explicit `--target`. Loom only models code that uses `loom::sync` types.

## 6. Profiling and debugging

| Tool | When to use | Command |
|---|---|---|
| [flamegraph](https://github.com/flamegraph-rs/flamegraph) | First look at where CPU time goes ([background](https://www.brendangregg.com/flamegraphs.html)). | `cargo flamegraph --bin <name>` |
| [perf](https://perf.wiki.kernel.org/index.php/Main_Page) | Linux sampling and hardware counters (cache misses, branch misses). | `perf record -g ./target/release/<bin>` then `perf report` |
| [samply](https://github.com/mstange/samply) | Cross-platform sampling profiler with a Firefox Profiler UI; works on macOS. | `samply record ./target/release/<bin>` |
| [Instruments](https://developer.apple.com/xcode/features/) | macOS Time Profiler, Allocations and System Trace. | Open Instruments, or `xcrun xctrace record --template 'Time Profiler' --launch -- ./bin` |
| [valgrind](https://valgrind.org/) | Linux memory errors and call graphs (callgrind, massif). | `valgrind --tool=callgrind ./bin` |
| [heaptrack](https://github.com/KDE/heaptrack) | Linux allocation profiling over time. | `heaptrack ./bin` |
| [dhat-rs](https://github.com/nnethercote/dhat-rs) | In-process heap profiling and allocation-count assertions in tests. | add `dhat` crate with a profiler guard |
| [tokio-console](https://github.com/tokio-rs/console) | Inspect live async tasks, stalls and waker behaviour. | `tokio-console` (app built with `console-subscriber` and `--cfg tokio_unstable`) |
| [tracing](https://github.com/tokio-rs/tracing) | Structured spans and events for services; the standard for async diagnostics. | add `tracing` and `tracing-subscriber` |
| [lldb](https://lldb.llvm.org/) | Debugger on macOS (default). | `rust-lldb ./target/debug/<bin>` |
| [gdb](https://sourceware.org/gdb/) | Debugger on Linux. | `rust-gdb ./target/debug/<bin>` |
| [Backtraces](https://doc.rust-lang.org/std/backtrace/index.html) | See where a panic or error originated. | `RUST_BACKTRACE=1` (or `full`) |
| [env_logger](https://docs.rs/env_logger/latest/env_logger/) | Control log verbosity per module at runtime. | `RUST_LOG=my_crate=debug,info` |

Tip: profile release builds with line-table debug info (section 4); profiling a debug build misleads.

## 7. Lints

- [rustc lints](https://doc.rust-lang.org/rustc/lints/index.html): put `#![deny(missing_docs, unreachable_pub)]` or `#![warn(...)]` at the crate root; use `[lints]` tables in `Cargo.toml` for workspace-wide policy.
- `unsafe_op_in_unsafe_fn`: when you write `unsafe`, warn on it so each unsafe operation inside an `unsafe fn` sits in its own `unsafe {}` block with a `// SAFETY:` comment.
- [clippy lint list](https://rust-lang.github.io/rust-clippy/master/index.html): enable `clippy::pedantic` as a warning group, then allow the noisy lints you disagree with, instead of denying the whole group. Consider `clippy::undocumented_unsafe_blocks`.
- [`#[expect(lint)]`](https://doc.rust-lang.org/reference/attributes/diagnostics.html): use instead of `#[allow]` for deliberate exceptions; the compiler warns when the exception becomes unnecessary.

## 8. Continuous integration

Start from [jonhoo/rust-ci-conf](https://github.com/jonhoo/rust-ci-conf). A GitHub Actions workflow outline:

1. Trigger on `push` and `pull_request`.
2. Job `fmt`: `cargo fmt --check`.
3. Job `clippy`: `cargo clippy --all-targets --all-features -- -D warnings`.
4. Job `test`: matrix of `stable`, `beta` and the declared MSRV; run `cargo nextest run` plus `cargo test --doc`.
5. Job `miri` (nightly): `cargo +nightly miri test`, if you have `unsafe`.
6. Job `deny`/`audit`: `cargo deny check` and `cargo audit`.
7. Job `semver`: `cargo semver-checks` for libraries.
8. Cache `~/.cargo` and `target/` (for example with Swatinem/rust-cache).

## 9. Documentation

- [rustdoc](https://doc.rust-lang.org/rustdoc/): build and open docs with `cargo doc --no-deps --open`.
- [Doc tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html): every code block in `///` comments is compiled and run by `cargo test`.
- [Intra-doc links](https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html): link items by name with ``[`Vec`]`` or ``[`crate::module::Item`]``; broken links are caught with `RUSTDOCFLAGS="-D warnings" cargo doc`.

## 10. Release engineering

| Tool | When to use | Command |
|---|---|---|
| [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) | Before publishing a library: detect accidental breaking API changes. | `cargo semver-checks` |
| [cargo-release](https://github.com/crate-ci/cargo-release) | Automate version bump, changelog commit, tag and publish. | `cargo release minor` (dry run by default; add `--execute`) |
| [cargo-dist](https://github.com/axodotdev/cargo-dist) | Build and publish binaries and installers for several platforms from CI. | `cargo dist init` |

## 11. Second pass: tools that were missing

Added after a gap review. Every link below returned HTTP 200 when this section was written. Install what you need, not everything: most of these are
situational. Install-first picks: `bacon`, `cargo-llvm-cov`, `cargo-mutants`, `hyperfine`, `just`, `cargo-binstall`.

### Test quality and verification

| Tool | Command | Use it for |
|---|---|---|
| [cargo-mutants](https://github.com/sourcefrog/cargo-mutants) | `cargo install cargo-mutants && cargo mutants` | Mutation testing: it changes your code and checks that a test fails. Surviving mutants show tests that assert nothing. |
| [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) | `cargo llvm-cov --html` | Line/region coverage using LLVM instrumentation; works with doctests and workspaces. |
| [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) | `cargo tarpaulin` | Alternative coverage tool (Linux-centric). |
| [cargo-hack](https://github.com/taiki-e/cargo-hack) | `cargo hack check --feature-powerset` | Checks every feature combination compiles; catches feature-unification bugs. |
| [cargo-careful](https://github.com/RalfJung/cargo-careful) | `cargo +nightly careful test` | Runs tests with extra UB checks and a debug std; cheaper than Miri. |
| [Kani](https://github.com/model-checking/kani) | `cargo kani` | Bounded model checker: PROVES properties for all inputs up to a bound (overflow, panics, unsafe invariants). |
| [Verus](https://github.com/verus-lang/verus) | `verus file.rs` | Deductive verification of Rust code with SMT; for proving functional correctness. |
| [Creusot](https://github.com/creusot-rs/creusot) | `cargo creusot` | Deductive verifier translating Rust to Why3. |
| [Prusti](https://github.com/viperproject/prusti-dev) | `cargo prusti` | Contracts (pre/postconditions) checked statically. |
| [AFL.rs](https://github.com/rust-fuzz/afl.rs) | `cargo afl fuzz` | Coverage-guided fuzzing with AFL++ (alternative to cargo-fuzz). |
| [cargo-geiger](https://github.com/geiger-rs/cargo-geiger) | `cargo geiger` | Counts unsafe usage in your crate and dependencies. |
| [cargo-vet](https://github.com/mozilla/cargo-vet) | `cargo vet` | Supply-chain audits: record who reviewed which dependency version. |
| [cargo-machete](https://github.com/bnjbvr/cargo-machete) | `cargo machete` | Finds unused dependencies quickly (cargo-udeps is slower but more precise). |
| [cargo-public-api](https://github.com/cargo-public-api/cargo-public-api) | `cargo public-api diff` | Shows the public API diff between versions; pairs with cargo-semver-checks. |
| [typos](https://github.com/crate-ci/typos) | `typos` | Source-code spell checker for CI. |
| [taplo](https://github.com/tamasfe/taplo) | `taplo fmt` | Formats and validates TOML (Cargo.toml). |
| [lychee](https://github.com/lycheeverse/lychee) | `lychee **/*.md` | Checks links in docs and READMEs (we hand-verify cookbook links with curl). |

### Fast inner loop

| Tool | Command | Use it for |
|---|---|---|
| [bacon](https://github.com/Canop/bacon) | `bacon test` | Background watcher that re-runs check/clippy/test on save with a readable TUI. |
| [cargo-watch](https://github.com/watchexec/cargo-watch) | `cargo watch -x test` | Simple rerun-on-change loop (in maintenance mode; bacon is the modern pick). |
| [sccache](https://github.com/mozilla/sccache) | `RUSTC_WRAPPER=sccache cargo build` | Shared compile cache for CI and multiple checkouts. |
| [mold](https://github.com/rui314/mold) | `RUSTFLAGS="-C link-arg=-fuse-ld=mold" cargo build` | Much faster linker on Linux; linking dominates incremental builds of big binaries. |
| [lld](https://lld.llvm.org/) | `RUSTFLAGS="-C link-arg=-fuse-ld=lld"` | LLVM linker; rustc ships `rust-lld` and uses it by default on some targets. |
| [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) | `cargo binstall cargo-nextest` | Installs prebuilt binaries of cargo tools instead of compiling them. |
| [just](https://github.com/casey/just) | `just test` | Command runner (a Makefile without the pain) for project tasks. |
| [lefthook](https://github.com/evilmartians/lefthook) | `lefthook install` | Git hooks manager (run fmt/clippy before commit). |
| [pre-commit](https://pre-commit.com/) | `pre-commit install` | Popular hook framework with a large hook catalogue. |
| [cargo-llvm-lines](https://github.com/dtolnay/cargo-llvm-lines) | `cargo llvm-lines` | Shows which generic functions bloat code and compile time. |
| [twiggy](https://github.com/rustwasm/twiggy) | `twiggy top file.wasm` | Code-size profiler (wasm and native). |

### Debugging and systems tools

| Tool | Command | Use it for |
|---|---|---|
| [rr](https://rr-project.org/) | `rr record ./bin && rr replay` | Record-and-replay debugger (Linux): run backwards, reproduce flaky concurrency bugs deterministically. |
| [bpftrace](https://github.com/bpftrace/bpftrace) | `sudo bpftrace -e 'tracepoint:syscalls:sys_enter_openat { @[comm] = count(); }'` | eBPF one-liners for tracing syscalls, latency and scheduling on Linux. |
| [heaptrack](https://github.com/KDE/heaptrack) | `heaptrack ./bin` | Heap profiler: allocations by call stack over time (Linux). |
| [bytehound](https://github.com/koute/bytehound) | `LD_PRELOAD=libbytehound.so ./bin` | Memory profiler with a web UI; finds leaks and fragmentation. |
| [Valgrind](https://valgrind.org/) | `valgrind --tool=massif ./bin` | memcheck, massif, callgrind, helgrind (Linux; limited on Apple Silicon). |
| [hyperfine](https://github.com/sharkdp/hyperfine) | `hyperfine './a' './b'` | Benchmarks whole commands with warm-up and statistics (compare two binaries). |
| [tcpdump](https://www.tcpdump.org/) | `sudo tcpdump -i lo0 -nn port 8080` | Capture packets; confirm what actually goes on the wire (pairs with the networking recipes). |
| [Wireshark](https://www.wireshark.org/) | `tshark -r capture.pcap` | Packet analysis with protocol dissectors; open pcap files our recipes write. |
| [rust-analyzer manual](https://rust-analyzer.github.io/book/) |  | Editor integration: inlay hints, `cargo check` on save, proc-macro and build-script support. |
| [rustfmt configuration](https://rust-lang.github.io/rustfmt/) | `rustfmt.toml` | Formatting options. |
| [Clippy configuration](https://doc.rust-lang.org/clippy/configuration.html) | `clippy.toml` | Tune lints and thresholds per project. |

### Load testing and HTTP/gRPC clients

| Tool | Command | Use it for |
|---|---|---|
| [oha](https://github.com/hatoo/oha) | `oha -z 10s -c 50 http://127.0.0.1:8080/` | HTTP load generator in Rust with a live latency histogram. |
| [wrk](https://github.com/wg/wrk) | `wrk -t4 -c64 -d10s http://127.0.0.1:8080/` | Classic HTTP benchmarking tool (Lua scripting). |
| [k6](https://k6.io/) | `k6 run script.js` | Scriptable load tests with thresholds for CI. |
| [vegeta](https://github.com/tsenart/vegeta) | `echo 'GET http://127.0.0.1:8080/' / vegeta attack -rate=500 / vegeta report` | Constant-rate load (avoids coordinated omission better than closed-loop tools). |
| [xh](https://github.com/ducaale/xh) | `xh POST :8080/jobs name=a` | Friendly HTTP client (HTTPie-compatible) for poking APIs. |
| [grpcurl](https://github.com/fullstorydev/grpcurl) | `grpcurl -plaintext localhost:50051 list` | curl for gRPC services (needs reflection or proto files). |

### Databases

| Tool | Command | Use it for |
|---|---|---|
| [sqlx-cli](https://github.com/launchbadge/sqlx) | `cargo install sqlx-cli && sqlx migrate run` | Migrations and compile-time checked queries (`cargo sqlx prepare` for offline mode). |
| [Diesel CLI](https://diesel.rs/) | `diesel migration run` | ORM and migrations alternative. |

### Containers and deployment

| Tool | Command | Use it for |
|---|---|---|
| [Docker multi-stage builds](https://docs.docker.com/build/building/multi-stage/) | `docker build -t app .` | Build in a fat image, ship a tiny runtime image. |
| [cargo-chef](https://github.com/LukeMathWalker/cargo-chef) | `cargo chef prepare / cook` | Caches dependency compilation layers in Docker builds. |
| [distroless](https://github.com/GoogleContainerTools/distroless) | `FROM gcr.io/distroless/cc` | Minimal runtime images without a shell (smaller attack surface). |
| [Terraform](https://developer.hashicorp.com/terraform) | `terraform plan && terraform apply` | Infrastructure as code (Cloud Run, Pub/Sub, Cloud SQL for FlowForge). |
| [tflint](https://github.com/terraform-linters/tflint) | `tflint` | Terraform linter. |
| [Cloud Run docs](https://cloud.google.com/run/docs) | `gcloud run deploy` | Container platform: expects SIGTERM graceful shutdown, a PORT env var and health checks. |
| [act](https://github.com/nektos/act) | `act -j test` | Runs GitHub Actions workflows locally. |
| [cross](https://github.com/cross-rs/cross) | `cross build --target aarch64-unknown-linux-gnu` | Cross-compilation in containers. |
| [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) | `cargo zigbuild --target x86_64-unknown-linux-musl` | Cross-compile using Zig's linker (easy musl and glibc-version targeting). |
| [git-cliff](https://github.com/orhun/git-cliff) | `git cliff -o CHANGELOG.md` | Changelog generation from conventional commits. |

### WebAssembly

| Tool | Command | Use it for |
|---|---|---|
| [wasmtime](https://github.com/bytecodealliance/wasmtime) | `wasmtime run file.wasm` | Run and embed WebAssembly; pairs with the wasm_binary_interpreter recipe. |
| [wasm-tools](https://github.com/bytecodealliance/wasm-tools) | `wasm-tools print file.wasm` | Parse, validate, print and fuzz-generate wasm. |
| [wasm-pack](https://github.com/rustwasm/wasm-pack) | `wasm-pack build` | Build and publish Rust to wasm for the web. |

### Observability stack

| Tool | Command | Use it for |
|---|---|---|
| [OpenTelemetry](https://opentelemetry.io/docs/) |  | Vendor-neutral traces, metrics and logs; the `tracing-opentelemetry` bridge exports spans. |
| [Prometheus](https://prometheus.io/docs/) |  | Pull-based metrics (text exposition format; see the http server recipe's /metrics). |
| [Grafana](https://grafana.com/docs/) |  | Dashboards and alerts over Prometheus, Loki and Tempo. |
| [Jaeger](https://www.jaegertracing.io/docs/) |  | Distributed trace storage and UI (trace context recipe shows the W3C headers). |

