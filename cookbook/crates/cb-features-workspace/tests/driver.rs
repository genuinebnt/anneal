//! Test driver: run `cargo test` for several feature sets. CI does the same
//! with a matrix, because a green default build says nothing about
//! `--no-default-features`.
use std::process::Command;

fn run(extra: &[&str], expect: &str) {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    // Separate target dir: the outer cargo may hold the shared one's lock.
    let target = std::env::temp_dir().join("cb_features_driver_target");
    let out = Command::new(env!("CARGO"))
        .args([
            "test",
            "-q",
            "--release",
            "--manifest-path",
            manifest,
            "--test",
            "feature_tests",
        ])
        .args(extra)
        .env("CARGO_TARGET_DIR", target)
        .env("CB_EXPECT", expect)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{extra:?} failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn no_default_features() {
    run(&["--no-default-features"], "none");
}
#[test]
fn default_features() {
    run(&[], "default");
}
#[test]
fn all_features() {
    run(&["--all-features"], "all");
}
#[test]
fn explicit_set() {
    run(
        &["--no-default-features", "--features", "stats,backend-a"],
        "x",
    );
}
