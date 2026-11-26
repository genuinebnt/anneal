use solution::*;
use std::collections::HashMap;

#[test]
fn max_of_values() {
    check!("max_of!(3, 9, 2)", max_of!(3, 9, 2), 9);
    check!("max_of!(1.5, 0.5,)", max_of!(1.5, 0.5,), 1.5);
    check!("max_of!(7)", max_of!(7), 7);
}

#[test]
fn max_of_evaluates_once() {
    let mut numbers = [10, 20, 30].into_iter();
    let got = max_of!(numbers.next().unwrap(), numbers.next().unwrap(), numbers.next().unwrap());
    check!("three calls to next()", (got, numbers.next()), (30, None));
}

#[test]
fn map_with_trailing_comma() {
    let got: HashMap<&str, u8> = map! {
        "a" => 1,
        "b" => 2,
    };
    check!(r#"map!{"a" => 1, "b" => 2,}"#, got, HashMap::from([("a", 1), ("b", 2)]));
}

fn half(n: u32) -> Result<u32, Failure> {
    ensure!(n & 1 == 0, "{} is odd", n);
    Ok(n / 2)
}

#[test]
fn ensure_passes_and_fails() {
    check!("half(4), half(5)", (half(4), half(5)), (Ok(2), Err(Failure("5 is odd".to_string()))));
}

#[test]
fn ensure_formats_only_on_failure() {
    fn go(ok: bool, evaluated: &mut u32) -> Result<(), Failure> {
        ensure!(ok, "failed after {} tries", {
            *evaluated += 1;
            *evaluated
        });
        Ok(())
    }
    let mut n = 0;
    let passed = go(true, &mut n);
    check!("condition true: times the argument ran", (passed, n), (Ok(()), 0));
}
