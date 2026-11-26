//! Behaviour that adapts to whichever feature set the test is compiled with.
//! Run directly it uses the defaults; `tests/driver.rs` reruns it with others.
use cb_features_workspace::*;

#[test]
fn modules_match_features() {
    let f = enabled_features();
    assert_eq!(f.contains(&"json"), cfg!(feature = "json"));
    #[cfg(feature = "json")]
    assert_eq!(json::to_json(&[3]), "[3]");
    #[cfg(feature = "stats")]
    assert_eq!(stats::mean(&[2.0, 4.0]), 3.0);
}

#[test]
fn cfg_attr_derive() {
    #[cfg(feature = "stats")]
    assert_eq!(Sample(1), Sample(1));
    assert_eq!(format!("{:?}", Sample(1)), "Sample(1)");
}

#[test]
fn exclusive_resolution() {
    let expect = if cfg!(feature = "backend-b") {
        "b"
    } else if cfg!(feature = "backend-a") {
        "a"
    } else {
        "none"
    };
    assert_eq!(backend(), expect);
}

/// The driver tells us which set it requested via an env var, so the test can
/// check that cargo really compiled that set.
#[test]
fn driver_expectation() {
    match std::env::var("CB_EXPECT").as_deref() {
        Ok("none") => assert!(enabled_features().is_empty()),
        Ok("default") => assert_eq!(enabled_features(), ["json"]),
        Ok("all") => assert_eq!(
            enabled_features(),
            ["json", "stats", "backend-a", "backend-b"]
        ),
        _ => {}
    }
}
