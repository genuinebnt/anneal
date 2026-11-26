//! Each test pins down one surprise. They pass because the surprise is real.

use anyhow::{Context, anyhow};
use choosing_an_error_type::boxed::BoxError;
use choosing_an_error_type::derived::PortError;
use choosing_an_error_type::{report, status};
use std::error::Error;
use std::io;

fn failed() -> anyhow::Result<()> {
    Err(PortError::Zero.into())
}

#[test]
fn anyhow_context_keeps_downcasting_on_the_anyhow_error() {
    let error = failed().context("starting the server").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<PortError>(),
        Some(PortError::Zero)
    ));
    assert_eq!(status::http_status_of(&error), 422);
}

#[test]
fn but_not_after_viewing_it_as_a_std_error() {
    let error = failed().context("starting the server").unwrap_err();
    // The outer std error is anyhow's private context wrapper.
    let as_std: &(dyn Error + Send + Sync + 'static) = error.as_ref();
    assert!(as_std.downcast_ref::<PortError>().is_none());
    // The original is one source() step down.
    assert!(report::find::<PortError>(as_std).is_some());
}

#[derive(Debug, thiserror::Error)]
#[error("while loading the configuration")]
struct Loading;

#[test]
fn a_context_type_is_found_only_by_anyhow_itself() {
    let error = failed()
        .context(Loading)
        .context("starting the server")
        .unwrap_err();
    assert!(error.downcast_ref::<Loading>().is_some());
    assert!(error.chain().all(|e| e.downcast_ref::<Loading>().is_none()));
}

#[test]
fn converting_to_box_dyn_error_loses_the_downcast() {
    fn through_question_mark() -> Result<(), BoxError> {
        failed()?; // no context at all
        Ok(())
    }
    let boxed = through_question_mark().unwrap_err();
    assert_eq!(boxed.to_string(), "port must not be 0"); // looks unchanged
    assert!(boxed.downcast_ref::<PortError>().is_none()); // but is not a PortError

    // This conversion keeps the concrete type, and gives up the backtrace.
    let kept = failed()
        .unwrap_err()
        .reallocate_into_boxed_dyn_error_without_backtrace();
    assert!(kept.downcast_ref::<PortError>().is_some());
}

#[test]
fn from_boxed_hides_the_concrete_type_from_downcast() {
    let boxed: BoxError = Box::new(PortError::Zero);
    let error = anyhow::Error::from_boxed(boxed);
    assert!(error.downcast_ref::<PortError>().is_none());
    assert!(
        error
            .chain()
            .all(|e| e.downcast_ref::<PortError>().is_none())
    );
    // What anyhow holds is the box, so ask for the box and then look inside.
    let inner = error.downcast_ref::<BoxError>().expect("the box");
    assert!(inner.downcast_ref::<PortError>().is_some());
}

#[test]
fn formatting_into_a_new_message_throws_the_cause_away() {
    let kept = anyhow!(PortError::Zero);
    assert!(kept.is::<PortError>());

    let flattened = anyhow!("startup failed: {}", PortError::Zero);
    assert!(!flattened.is::<PortError>());
    assert_eq!(flattened.chain().count(), 1);
    assert_eq!(status::http_status_of(&flattened), 500);
}

/// The same inner error, wrapped two ways.
#[derive(Debug, thiserror::Error)]
enum Wrapper {
    #[error(transparent)]
    Transparent(#[from] io::Error),
    #[error("storage failed")]
    Layer(#[source] io::Error),
}

#[test]
fn transparent_removes_the_inner_error_from_the_chain() {
    let transparent = Wrapper::from(io::Error::other("disk gone"));
    assert_eq!(transparent.to_string(), "disk gone");
    assert!(report::find::<io::Error>(&transparent).is_none());
    assert!(matches!(transparent, Wrapper::Transparent(_))); // matching still works

    let layer = Wrapper::Layer(io::Error::other("disk gone"));
    assert!(report::find::<io::Error>(&layer).is_some());
}

/// Wrong on purpose: the cause is in the message AND returned by `source()`.
#[derive(Debug, thiserror::Error)]
#[error("storage failed: {source}")]
struct Doubled {
    source: io::Error,
}

#[test]
fn naming_the_cause_in_display_prints_it_twice() {
    let error = Doubled {
        source: io::Error::other("disk gone"),
    };
    assert_eq!(
        report::one_line(&error),
        "storage failed: disk gone: disk gone"
    );
}
