//! Feature-gated library. Doc-cfg badges only render on nightly docs.rs builds.
// On nightly with `--cfg docsrs`, enable the `doc(cfg(..))` attribute. On stable
// the whole line vanishes because `docsrs` is not set.
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Feature-gated module: only exists when `json` is on.
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub mod json {
    /// Serialise a list of numbers.
    ///
    /// ```
    /// assert_eq!(cb_features_workspace::json::to_json(&[1, 2]), "[1,2]");
    /// ```
    pub fn to_json(v: &[i32]) -> String {
        serde_json::to_string(v).unwrap()
    }
}

#[cfg(feature = "stats")]
#[cfg_attr(docsrs, doc(cfg(feature = "stats")))]
pub mod stats {
    /// Arithmetic mean.
    ///
    /// ```
    /// assert_eq!(cb_features_workspace::stats::mean(&[1.0, 3.0]), 2.0);
    /// ```
    pub fn mean(v: &[f64]) -> f64 {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

/// `cfg_attr`: conditionally apply an attribute. Here `Debug` is always derived,
/// and `PartialEq` only with `stats` (stand-in for `serde::Serialize` etc.).
#[derive(Debug)]
#[cfg_attr(feature = "stats", derive(PartialEq))]
pub struct Sample(pub u8);

/// Backend choice for features that may BOTH be enabled (additive rule):
/// resolve conflicts with a deterministic precedence, never `compile_error!`,
/// because `--all-features` or another crate in the graph can enable both.
pub fn backend() -> &'static str {
    if cfg!(feature = "backend-b") {
        "b"
    } else if cfg!(feature = "backend-a") {
        "a"
    } else {
        "none"
    }
}

/// `cfg!` evaluates at compile time to a literal bool, but both branches must
/// still type-check (unlike `#[cfg]`, which removes code before type checking).
pub fn enabled_features() -> Vec<&'static str> {
    let mut v = vec![];
    if cfg!(feature = "json") {
        v.push("json");
    }
    if cfg!(feature = "stats") {
        v.push("stats");
    }
    if cfg!(feature = "backend-a") {
        v.push("backend-a");
    }
    if cfg!(feature = "backend-b") {
        v.push("backend-b");
    }
    v
}
