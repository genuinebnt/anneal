//! Runs on the HOST, before the crate is compiled. Talks to cargo only via
//! stdout lines (`cargo::...`) and env vars; cargo reruns it only when the
//! declared inputs change.
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    // Rerun rules. Without ANY rerun-if line cargo reruns on every source
    // change in the package; with them, only when these inputs change.
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=csrc/add.c");
    println!("cargo::rerun-if-env-changed=CB_BUILD_LABEL");
    println!("cargo::rerun-if-env-changed=CB_BUILD_WARN");
    // HEAD changes on commit/checkout; ignore silently if there is no git dir.
    println!("cargo::rerun-if-changed=../../.git/HEAD");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    // ---- 1. build stamping -------------------------------------------------
    // CARGO_PKG_VERSION is already available via env!; we stamp what cargo
    // does NOT give to the crate itself.
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let rustc_v = Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let git = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo::rustc-env=CB_RUSTC_VERSION={rustc_v}");
    println!("cargo::rustc-env=CB_GIT_HASH={git}");
    // PROFILE is "release"/"debug"; TARGET is the *target* triple, HOST the build machine.
    println!(
        "cargo::rustc-env=CB_PROFILE={}",
        env::var("PROFILE").unwrap()
    );
    println!("cargo::rustc-env=CB_TARGET={}", env::var("TARGET").unwrap());
    println!("cargo::rustc-env=CB_HOST={}", env::var("HOST").unwrap());
    let label = env::var("CB_BUILD_LABEL").unwrap_or_else(|_| "none".into());
    println!("cargo::rustc-env=CB_LABEL={label}");

    // ---- 2. cfg probing (a hand-written autocfg) ---------------------------
    // Declare custom cfgs so `unexpected_cfgs` lint stays quiet.
    println!("cargo::rustc-check-cfg=cfg(has_let_chains)");
    println!("cargo::rustc-check-cfg=cfg(has_nonexistent_api)");
    // Probe: does this compiler accept the snippet? Compile it to metadata only.
    let probe = |name: &str, src: &str| -> bool {
        let f = out.join(format!("probe_{name}.rs"));
        fs::write(&f, src).unwrap();
        Command::new(&rustc)
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--out-dir",
            ])
            .arg(&out)
            .arg(&f)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if probe(
        "let_chains",
        "pub fn f(a: Option<u8>) -> bool { if let Some(x) = a && x > 1 { true } else { false } }",
    ) {
        println!("cargo::rustc-cfg=has_let_chains");
    }
    if probe(
        "nonexistent",
        "pub fn f() { core::this_api_does_not_exist(); }",
    ) {
        println!("cargo::rustc-cfg=has_nonexistent_api");
    }

    // ---- 3. code generation into OUT_DIR -----------------------------------
    // CRC-32 (IEEE) table computed at build time, not hand-pasted.
    let mut s = String::from("pub static CRC32_TABLE: [u32; 256] = [\n");
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        s.push_str(&format!("    0x{c:08X},\n"));
    }
    s.push_str("];\n");
    fs::write(out.join("table.rs"), s).unwrap();

    // ---- 4. compile C with the cc crate ------------------------------------
    // cc picks the right compiler for TARGET (honouring CC_<target>), emits
    // cargo::rustc-link-lib/-search itself, and its own rerun lines.
    cc::Build::new().file("csrc/add.c").compile("cbadd");

    // ---- 5. warnings -------------------------------------------------------
    // Shown for path/workspace crates always; for registry deps only on failure.
    // Opt-in so the normal build stays warning-free: CB_BUILD_WARN=1 cargo build
    if env::var_os("CB_BUILD_WARN").is_some() {
        println!("cargo::warning=demo warning from build.rs (git hash: {git})");
    }
}
