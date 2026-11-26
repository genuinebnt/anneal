//! All four implementations accept the same input and describe each failure the same way.

use choosing_an_error_type::{app, boxed, derived, hand, report, status};
use std::io;
use std::num::ParseIntError;
use std::path::Path;

/// The one-line chain from each implementation.
fn four_ways(text: &str) -> [Result<u16, String>; 4] {
    [
        hand::parse_port(text).map_err(|e| report::one_line(&e)),
        derived::parse_port(text).map_err(|e| report::one_line(&e)),
        app::parse_port(text).map_err(|e| format!("{e:#}")),
        boxed::parse_port(text).map_err(|e| report::one_line(e.as_ref())),
    ]
}

#[test]
fn success_and_every_content_failure_agree() {
    let cases: [(&str, Result<u16, &str>); 6] = [
        ("host=a\nport=8080\n", Ok(8080)),
        ("port=65535", Ok(65535)),
        ("host=a", Err("no `port=` line")),
        (
            "port=eighty",
            Err("invalid port \"eighty\": invalid digit found in string"),
        ),
        (
            "port=65536",
            Err("invalid port \"65536\": number too large to fit in target type"),
        ),
        ("port=0", Err("port must not be 0")),
    ];
    for (text, expected) in cases {
        let expected = expected.map_err(str::to_owned);
        for (i, got) in four_ways(text).into_iter().enumerate() {
            assert_eq!(got, expected, "implementation {i}, input {text:?}");
        }
    }
}

#[test]
fn every_implementation_keeps_the_io_cause() {
    let path = Path::new("/definitely/missing.conf");
    let hand = hand::load_port(path).unwrap_err();
    let derived = derived::load_port(path).unwrap_err();
    let app = app::load_port(path).unwrap_err();
    let boxed = boxed::load_port(path).unwrap_err();
    let chains: [&(dyn std::error::Error + 'static); 4] =
        [&hand, &derived, app.as_ref(), boxed.as_ref()];
    for chain in chains {
        assert_eq!(chain.to_string(), "cannot read /definitely/missing.conf");
        let cause = report::find::<io::Error>(chain).expect("io::Error in the chain");
        assert_eq!(cause.kind(), io::ErrorKind::NotFound);
        assert_eq!(report::sources(chain).count(), 2);
    }
}

#[test]
fn callers_match_variants_and_read_their_data() {
    assert!(matches!(
        derived::parse_port("port=x"),
        Err(derived::PortError::NotANumber { ref text, .. }) if text == "x"
    ));
    assert!(matches!(
        hand::parse_port("port=0"),
        Err(hand::PortError::Zero)
    ));
}

#[test]
fn an_erased_error_can_still_be_asked_about_its_cause() {
    let app = app::parse_port("port=x").unwrap_err();
    assert!(app.downcast_ref::<ParseIntError>().is_some());

    // Box<dyn Error> downcasts only the outer value, so walk the chain.
    let boxed = boxed::parse_port("port=x").unwrap_err();
    assert!(boxed.downcast_ref::<ParseIntError>().is_none());
    assert!(report::find::<ParseIntError>(boxed.as_ref()).is_some());
}

#[test]
fn statuses_are_decided_from_variants() {
    let missing_file = derived::load_port(Path::new("/definitely/missing.conf")).unwrap_err();
    assert_eq!(
        (
            status::exit_code(&missing_file),
            status::http_status(&missing_file)
        ),
        (2, 404)
    );

    // Reading a directory fails with something other than NotFound.
    let directory = derived::load_port(Path::new("/")).unwrap_err();
    assert_eq!(
        (
            status::exit_code(&directory),
            status::http_status(&directory)
        ),
        (1, 500)
    );
    assert_eq!(status::public_message(&directory), "internal error");

    let bad = derived::parse_port("port=0").unwrap_err();
    assert_eq!(
        (status::exit_code(&bad), status::http_status(&bad)),
        (2, 422)
    );
    assert_eq!(status::public_message(&bad), "port must not be 0");
}
