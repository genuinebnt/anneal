use solution::*;


#[allow(dead_code)]
fn lim() -> Limits {
    Limits { max_bytes: 10, max_items: 3, max_depth: 3 }
}
#[allow(dead_code)]
fn read(input: &str) -> Result<Option<(Value, usize)>, Error> {
    decode(input.as_bytes(), &lim())
}
#[allow(dead_code)]
fn whole(input: &str) -> Result<Option<Value>, Error> {
    read(input).map(|r| r.map(|(v, used)| {
        assert_eq!(used, input.len(), "all of {input:?} belongs to the value");
        v
    }))
}
#[allow(dead_code)]
fn text(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}
/// `bad` must be refused with `error` exactly when its last byte arrives, and not a byte earlier.
#[allow(dead_code)]
fn refused_at_the_last_byte(bad: &str, error: Error) {
    for cut in 0..bad.len() {
        check!(format!("the first {cut} bytes of {bad:?} could still be valid"), decode(&bad.as_bytes()[..cut], &lim()), Ok(None));
    }
    check!(format!("{bad:?}"), read(bad), Err(error));
}

#[test]
fn the_three_kinds() {
    check!("an integer", whole("i-42e"), Ok(Some(Value::Int(-42))));
    check!("bytes", whole("5:he:ll"), Ok(Some(text("he:ll"))));
    check!("no bytes", whole("0:"), Ok(Some(text(""))));
    check!("a list", whole("li1el2:abee"), Ok(Some(Value::List(vec![Value::Int(1), Value::List(vec![text("ab")])]))));
    check!("an empty list", whole("le"), Ok(Some(Value::List(vec![]))));
}

#[test]
fn only_the_first_value_is_taken() {
    check!("two values", read("i7ei8e"), Ok(Some((Value::Int(7), 3))));
}

#[test]
fn an_unfinished_value_needs_more() {
    let input = "li-5e3:abcl0:ee";
    for cut in 0..input.len() {
        check!(format!("{:?}", &input[..cut]), read(&input[..cut]), Ok(None));
    }
}

#[test]
fn malformed_input() {
    refused_at_the_last_byte("x", Error::Malformed);
    refused_at_the_last_byte("ie", Error::Malformed);
    refused_at_the_last_byte("i03", Error::Malformed);
    refused_at_the_last_byte("i-0", Error::Malformed);
}

#[test]
fn a_limit_is_reported_before_the_input_is_complete() {
    refused_at_the_last_byte("11", Error::TooLong);
    refused_at_the_last_byte("llll", Error::TooDeep);
    refused_at_the_last_byte("li1ei2ei3ei", Error::TooMany);
}
