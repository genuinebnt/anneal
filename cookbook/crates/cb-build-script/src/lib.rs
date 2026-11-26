//! Values produced by `build.rs`. See README.

// Generated code: `include!` pastes the file at this point in the module.
include!(concat!(env!("OUT_DIR"), "/table.rs"));

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HASH: &str = env!("CB_GIT_HASH");
pub const PROFILE: &str = env!("CB_PROFILE");
pub const TARGET: &str = env!("CB_TARGET");
pub const HOST: &str = env!("CB_HOST");
pub const RUSTC_VERSION: &str = env!("CB_RUSTC_VERSION");
/// Read with `option_env!` because it is a user-provided input, not guaranteed.
pub const LABEL: &str = env!("CB_LABEL");

/// Two code paths chosen at compile time by a cfg the build script detected.
#[cfg(has_let_chains)]
pub fn let_chains_path() -> &'static str {
    "let-chains available"
}
#[cfg(not(has_let_chains))]
pub fn let_chains_path() -> &'static str {
    "fallback"
}

#[cfg(has_nonexistent_api)]
pub fn nonexistent_path() -> bool {
    true
}
#[cfg(not(has_nonexistent_api))]
pub fn nonexistent_path() -> bool {
    false
}

/// CRC-32 (IEEE 802.3) using the generated table.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c = CRC32_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

unsafe extern "C" {
    fn cb_add(a: i32, b: i32) -> i32;
}

/// Safe wrapper over the C function compiled by build.rs.
pub fn c_add(a: i32, b: i32) -> i32 {
    // SAFETY: pure C function with no pointers or global state.
    unsafe { cb_add(a, b) }
}
