use solution::*;
use std::collections::BTreeMap;

#[allow(dead_code)]
fn unit(list: &[(&str, i32)]) -> Result<BTreeMap<String, i32>, UnitError> {
    Ok(list.iter().map(|&(n, p)| (n.to_string(), p)).collect())
}

#[test]
fn side_by_side_and_dots_multiply() {
    check!("N m", parse("N m"), unit(&[("N", 1), ("m", 1)]));
    check!("N.m", parse("N.m"), unit(&[("N", 1), ("m", 1)]));
    check!("kg(m)m", parse("kg(m)m"), unit(&[("kg", 1), ("m", 2)]));
}

#[test]
fn a_product_binds_tighter_than_a_ratio() {
    check!("J / kg K", parse("J / kg K"), unit(&[("J", 1), ("K", -1), ("kg", -1)]));
    check!("J / (K.kg)", parse("J / (K.kg)"), unit(&[("J", 1), ("K", -1), ("kg", -1)]));
    check!("a b / c d", parse("a b / c d"), unit(&[("a", 1), ("b", 1), ("c", -1), ("d", -1)]));
}

#[test]
fn ratios_read_from_the_left() {
    check!("a / b / c", parse("a / b / c"), unit(&[("a", 1), ("b", -1), ("c", -1)]));
    check!("a / (b / c)", parse("a / (b / c)"), unit(&[("a", 1), ("b", -1), ("c", 1)]));
}

#[test]
fn powers_bind_to_one_factor() {
    check!("kg m s^-2", parse("kg m s^-2"), unit(&[("kg", 1), ("m", 1), ("s", -2)]));
    check!("(m / s)^ 2", parse("(m / s)^ 2"), unit(&[("m", 2), ("s", -2)]));
}

#[test]
fn what_cancels_disappears() {
    check!("m / m", parse("m / m"), unit(&[]));
    check!("x^0 y", parse("x^0 y"), unit(&[("y", 1)]));
    check!("1 / s", parse("1 / s"), unit(&[("s", -1)]));
}

#[test]
fn errors() {
    check!("empty", parse(""), Err(UnitError::UnexpectedEnd));
    check!("m /", parse("m /"), Err(UnitError::UnexpectedEnd));
    check!("m + s", parse("m + s"), Err(UnitError::BadChar(2)));
    check!("m) s", parse("m) s"), Err(UnitError::Unexpected(1)));
    check!("m^s", parse("m^s"), Err(UnitError::Unexpected(2)));
    check!("m 2", parse("m 2"), Err(UnitError::Unexpected(2)));
}
