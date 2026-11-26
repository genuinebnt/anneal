use cb_build_script::*;

#[test]
fn stamps() {
    assert_eq!(VERSION, "0.1.0");
    assert_eq!(PROFILE, "release"); // the check script runs with --release
    assert!(TARGET.contains("-"));
    assert_eq!(TARGET, HOST); // no cross compile here; they differ when cross compiling
    assert!(RUSTC_VERSION.starts_with("rustc "));
    assert!(!GIT_HASH.is_empty());
    assert!(!LABEL.is_empty());
}

#[test]
fn cfg_probes() {
    assert_eq!(let_chains_path(), "let-chains available");
    assert!(!nonexistent_path());
}

#[test]
fn generated_table() {
    assert_eq!(CRC32_TABLE.len(), 256);
    assert_eq!(CRC32_TABLE[1], 0x7707_3096);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926); // standard check value
}

#[test]
fn c_ffi() {
    assert_eq!(c_add(2, 3), 5);
    assert_eq!(c_add(-1, i32::MIN + 1), i32::MIN);
}
