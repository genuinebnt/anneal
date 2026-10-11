//! Tests for module 3a, SQL values and types. A test name starts with its stage: `s3a_03_…` belongs to stage 3a-03, and `anneal course
//! test` runs just those.
//!
//! The data model is given (`TypeId` and the `Value` enum, BusTub's own); you write the operations on it. The tests are properties
//! checked against an oracle that is plain Rust: integer arithmetic and comparison against `i128`, decimals against `f64`, strings against
//! `str`. The few things that are only BusTub's convention (the coercion table, the text a value prints as, the bytes a value is stored
//! as) are written down once as tables and examples. Any value that is NULL is checked everywhere it matters: SQL's three-valued logic
//! is the part of this module that surprises people.

use std::cmp::Ordering;

use bustub::common::exception::{Exception, ExceptionType};
use bustub::types::limits::*;
use bustub::types::type_id::TypeId;
use bustub::types::value::{CmpBool, Value};
use proptest::prelude::*;
use TypeId::*;

const ALL: [TypeId; 9] = [Invalid, Boolean, TinyInt, SmallInt, Integer, BigInt, Decimal, Varchar, Timestamp];
const INT_TYPES: [TypeId; 4] = [TinyInt, SmallInt, Integer, BigInt];

fn kind<T: std::fmt::Debug>(r: Result<T, Exception>) -> ExceptionType {
    r.expect_err("expected an exception").kind
}

fn config() -> ProptestConfig {
    ProptestConfig { cases: 200, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

/// The usable range of an integer type: the smallest number is reserved for NULL.
fn range(t: TypeId) -> (i128, i128) {
    match t {
        TinyInt => (BUSTUB_INT8_MIN as i128, BUSTUB_INT8_MAX as i128),
        SmallInt => (BUSTUB_INT16_MIN as i128, BUSTUB_INT16_MAX as i128),
        Integer => (BUSTUB_INT32_MIN as i128, BUSTUB_INT32_MAX as i128),
        BigInt => (BUSTUB_INT64_MIN as i128, BUSTUB_INT64_MAX as i128),
        _ => unreachable!(),
    }
}

/// A value of integer type `t` holding `x` (which must be in range).
fn int_of(t: TypeId, x: i128) -> Value {
    match t {
        TinyInt => Value::tinyint(x as i8),
        SmallInt => Value::smallint(x as i16),
        Integer => Value::integer(x as i32),
        BigInt => Value::bigint(x as i64),
        _ => unreachable!(),
    }
}

/// A non-NULL integer value of a random integer type, with the number it holds.
fn any_int() -> impl Strategy<Value = (Value, i128)> {
    (0usize..4, any::<u64>(), prop_oneof![Just(0u8), Just(1), Just(2), Just(3)]).prop_map(|(t, bits, flavour)| {
        let t = INT_TYPES[t];
        let (lo, hi) = range(t);
        let span = (hi - lo + 1) as u128;
        let x = match flavour {
            0 => lo,                                   // an extreme
            1 => hi,                                   // the other extreme
            2 => (bits as i128 % 7) - 3,               // near zero
            _ => lo + (bits as u128 % span) as i128,   // anywhere
        };
        (int_of(t, x), x)
    })
}

fn nulls() -> impl Strategy<Value = Value> {
    prop_oneof![Just(Value::null(TinyInt)), Just(Value::null(SmallInt)), Just(Value::null(Integer)), Just(Value::null(BigInt)), Just(Value::null(Decimal))]
}

// ---- 3a-01 · Types and values ---------------------------------------------------------------------------------------------

#[test]
fn s3a_01_every_type_has_a_size_in_bytes_and_a_name() {
    let sizes = [(Boolean, 1), (TinyInt, 1), (SmallInt, 2), (Integer, 4), (BigInt, 8), (Decimal, 8), (Timestamp, 8), (Varchar, 0)];
    for (t, size) in sizes {
        assert_eq!(t.type_size().unwrap(), size, "{t:?}");
    }
    assert_eq!(kind(Invalid.type_size()), ExceptionType::UnknownType, "the invalid type has no size");
    let names = [(Invalid, "INVALID"), (Boolean, "BOOLEAN"), (TinyInt, "TINYINT"), (SmallInt, "SMALLINT"), (Integer, "INTEGER"), (BigInt, "BIGINT"), (Decimal, "DECIMAL"), (Varchar, "VARCHAR"), (Timestamp, "TIMESTAMP")];
    for (t, name) in names {
        assert_eq!(t.type_id_to_string(), name);
    }
}

#[test]
fn s3a_01_the_coercion_table_is_exactly_bustubs() {
    let numeric = [TinyInt, SmallInt, Integer, BigInt, Decimal];
    for target in ALL {
        for from in ALL {
            let expected = match target {
                Invalid => false,
                Boolean => true,
                t if numeric.contains(&t) => numeric.contains(&from) || from == Varchar,
                Timestamp => matches!(from, Varchar | Timestamp),
                Varchar => from != Invalid,
                _ => unreachable!(),
            };
            assert_eq!(target.is_coercable_from(from), expected, "{target:?} from {from:?}");
        }
    }
    for t in ALL.into_iter().filter(|&t| t != Invalid) {
        assert!(t.is_coercable_from(t), "{t:?} is coercable from itself");
    }
    assert!(!Integer.is_coercable_from(Boolean), "a number cannot be made from a boolean");
}

proptest! {
    #![proptest_config(config())]

    /// The smallest number of each integer type is reserved as NULL: constructing with it gives a NULL of that type, anything else keeps its number.
    #[test]
    fn s3a_01_the_reserved_number_makes_a_null_and_every_other_number_survives(a in any::<i8>(), b in any::<i16>(), c in any::<i32>(), d in any::<i64>()) {
        for (v, t, x, reserved) in [(Value::tinyint(a), TinyInt, a as i128, a == i8::MIN), (Value::smallint(b), SmallInt, b as i128, b == i16::MIN), (Value::integer(c), Integer, c as i128, c == i32::MIN), (Value::bigint(d), BigInt, d as i128, d == i64::MIN)] {
            prop_assert_eq!(v.type_id(), t);
            prop_assert_eq!(v.is_null(), reserved, "{:?} holding {}", t, x);
            if !reserved {
                prop_assert_eq!(v.as_i64(), Some(x as i64));
                prop_assert_eq!(v.as_f64(), Some(x as f64));
            } else {
                prop_assert_eq!(v.as_i64(), None);
            }
        }
    }

    #[test]
    fn s3a_01_a_null_keeps_its_type_and_has_no_number(t in prop::sample::select(vec![Boolean, TinyInt, SmallInt, Integer, BigInt, Decimal, Varchar, Timestamp])) {
        let v = Value::null(t);
        prop_assert!(v.is_null());
        prop_assert_eq!(v.type_id(), t);
        prop_assert!(v.as_i64().is_none() && v.as_f64().is_none());
    }
}

#[test]
fn s3a_01_minimum_and_maximum_values_are_values_in_order() {
    for t in [TinyInt, SmallInt, Integer, BigInt, Decimal, Timestamp, Boolean] {
        let (lo, hi) = (t.min_value().unwrap(), t.max_value().unwrap());
        assert!(!lo.is_null() && !hi.is_null(), "{t:?}: the smallest and largest values are real values");
        assert_eq!((lo.type_id(), hi.type_id()), (t, t));
        let num = |v: &Value| match v {
            Value::Timestamp(x) => *x as f64,
            Value::Boolean(b) => *b as u8 as f64,
            other => other.as_f64().unwrap(),
        };
        assert!(num(&lo) <= num(&hi), "{t:?}: min <= max");
    }
    assert_eq!(Integer.min_value().unwrap(), Value::Integer(i32::MIN + 1), "one above the reserved NULL number");
    assert_eq!(Varchar.min_value().unwrap(), Value::varchar(""));
    assert!(Varchar.max_value().unwrap().is_null(), "there is no largest string");
    assert_eq!(kind(Invalid.min_value()), ExceptionType::MismatchType);
    assert_eq!(kind(Invalid.max_value()), ExceptionType::MismatchType);
}

#[test]
fn s3a_01_values_print_like_bustub() {
    assert_eq!(Value::boolean(true).to_string(), "true");
    assert_eq!(Value::integer(-5).to_string(), "-5");
    assert_eq!(Value::decimal(3.14).to_string(), "3.140000", "six digits after the point, like std::to_string");
    assert_eq!(Value::varchar("hi there").to_string(), "hi there");
    assert_eq!(Value::timestamp(77).to_string(), "77");
    for (t, text) in [(Integer, "integer_null"), (Varchar, "varlen_null"), (Decimal, "decimal_null"), (Boolean, "boolean_null"), (TinyInt, "tinyint_null"), (SmallInt, "smallint_null"), (BigInt, "bigint_null"), (Timestamp, "timestamp_null")] {
        assert_eq!(Value::null(t).to_string(), text);
    }
}

#[test]
fn s3a_01_which_types_can_be_compared() {
    let (i, d, s, b, t) = (Value::integer(1), Value::decimal(1.0), Value::varchar("1"), Value::boolean(true), Value::timestamp(5));
    assert!(i.check_comparable(&d) && d.check_comparable(&i) && i.check_comparable(&s));
    assert!(s.check_comparable(&b) && s.check_comparable(&t) && s.check_comparable(&i), "anything can be compared with a string");
    assert!(b.check_comparable(&b) && b.check_comparable(&s));
    assert!(!b.check_comparable(&i) && !i.check_comparable(&b), "a boolean is not a number");
    assert!(t.check_comparable(&t) && !t.check_comparable(&i));
}

// ---- 3a-02 · Casting -------------------------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// Widening an integer never changes its number; narrowing succeeds exactly when the target type can hold it, else OutOfRange.
    #[test]
    fn s3a_02_an_integer_cast_to_another_integer_type_keeps_its_number_or_is_out_of_range((v, x) in any_int(), to in 0usize..4) {
        let to = INT_TYPES[to];
        let (lo, hi) = range(to);
        match v.cast_as(to) {
            Ok(cast) => {
                prop_assert!(lo <= x && x <= hi, "{} must not fit in {:?}", x, to);
                prop_assert_eq!(cast.type_id(), to);
                prop_assert_eq!(cast.as_i64(), Some(x as i64));
            }
            Err(e) => {
                prop_assert!(x < lo || x > hi, "{} fits in {:?} but the cast failed: {:?}", x, to, e);
                prop_assert_eq!(e.kind, ExceptionType::OutOfRange);
            }
        }
    }

    #[test]
    fn s3a_02_an_integer_cast_to_decimal_keeps_its_number((v, x) in any_int()) {
        let d = v.cast_as(Decimal).unwrap();
        prop_assert_eq!(d.type_id(), Decimal);
        prop_assert_eq!(d.as_f64(), Some(x as f64));
    }

    /// A decimal cast to an integer type truncates toward zero when the number fits, and is OutOfRange when it does not.
    #[test]
    fn s3a_02_a_decimal_truncates_toward_zero_and_checks_the_range(x in -40_000.0f64..40_000.0, to in 0usize..3) {
        let to = [TinyInt, SmallInt, Integer][to];
        let (lo, hi) = range(to);
        match Value::decimal(x).cast_as(to) {
            Ok(v) => {
                prop_assert_eq!(v.as_i64(), Some(x.trunc() as i64), "{} as {:?}", x, to);
                prop_assert!((lo as f64) <= x && x <= hi as f64);
            }
            Err(e) => {
                prop_assert_eq!(e.kind, ExceptionType::OutOfRange);
                prop_assert!(x < lo as f64 || x > hi as f64, "{} fits in {:?} but was refused", x, to);
            }
        }
    }

    /// A number written as text comes back as the same number.
    #[test]
    fn s3a_02_a_number_survives_a_trip_through_text((v, x) in any_int()) {
        let text = v.cast_as(Varchar).unwrap();
        prop_assert_eq!(text.as_str().map(str::to_owned), Some(x.to_string()));
        let back = text.cast_as(v.type_id()).unwrap();
        prop_assert_eq!(back.as_i64(), Some(x as i64));
    }

    /// The number at the start of a text is what counts; anything after it is ignored (like C++'s `stoi`).
    #[test]
    fn s3a_02_text_converts_by_its_leading_number(x in -9_999i32..9_999, spaces in 0usize..3, junk in "[a-z ]{0,4}") {
        let text = format!("{}{x}{junk}", " ".repeat(spaces));
        prop_assert_eq!(Value::varchar(&text).cast_as(Integer).unwrap().as_i64(), Some(x as i64), "{:?}", text);
    }

    /// Text with no digits is a Conversion error, not a zero.
    #[test]
    fn s3a_02_text_without_digits_is_a_conversion_error(text in "[a-z ]{0,6}") {
        prop_assert_eq!(Value::varchar(&text).cast_as(Integer).unwrap_err().kind, ExceptionType::Conversion);
    }

    /// A boolean is written `true`, `1` or `t` (any case) or `false`, `0` or `f`; nothing else is a boolean.
    #[test]
    fn s3a_02_only_the_known_spellings_are_booleans(word in prop::sample::select(vec!["true", "TRUE", "True", "t", "T", "1", "false", "FALSE", "False", "f", "F", "0", "yes", "no", "", "2", "tru"])) {
        let r = Value::varchar(word).cast_as(Boolean);
        match word {
            "true" | "TRUE" | "True" | "t" | "T" | "1" => prop_assert_eq!(r.unwrap().as_bool(), Some(true)),
            "false" | "FALSE" | "False" | "f" | "F" | "0" => prop_assert_eq!(r.unwrap().as_bool(), Some(false)),
            _ => prop_assert!(r.is_err(), "{:?} is not a boolean", word),
        }
    }
}

#[test]
fn s3a_02_a_null_becomes_the_null_of_the_target_and_impossible_casts_are_errors() {
    for from in [TinyInt, SmallInt, Integer, BigInt, Decimal] {
        for to in ALL {
            let r = Value::null(from).cast_as(to);
            if Value::integer(1).cast_as(to).is_ok() {
                let v = r.unwrap();
                assert!(v.is_null() && v.type_id() == to, "NULL {from:?} as {to:?} is the NULL of {to:?}");
            } else {
                assert!(r.is_err(), "NULL {from:?} as {to:?} is an error");
            }
        }
    }
    assert!(Value::boolean(true).cast_as(Integer).is_err(), "a boolean is not a number");
    assert!(Value::integer(1).cast_as(Boolean).is_err(), "and a number is not a boolean");
    assert!(Value::timestamp(5).cast_as(Integer).is_err());
    assert_eq!(Value::boolean(true).cast_as(Varchar).unwrap().as_str(), Some("true"));
    assert!(Value::integer(5).cast_as(Invalid).is_err());
}

/// A string to a decimal reads the leading number with its fraction and its exponent (the contract of the stage page).
#[test]
fn s3a_02_text_converts_to_a_decimal_with_its_fraction_and_exponent() {
    let cases: [(&str, f64); 6] = [("12.5", 12.5), ("-0.25", -0.25), ("3", 3.0), ("  7.5xyz", 7.5), ("1e3", 1000.0), ("2.5E-1", 0.25)];
    for (text, want) in cases {
        let got = Value::varchar(text).cast_as(Decimal).unwrap_or_else(|e| panic!("{text:?} as a decimal should be {want}, got the error {e}"));
        assert_eq!(got.as_f64(), Some(want), "{text:?} as a decimal reads its fraction and exponent, not only its integer part");
    }
}

/// No digits is a Conversion error for a decimal too: a lone point or sign is not a number, and neither is an exponent alone.
#[test]
fn s3a_02_text_with_no_digits_is_not_a_decimal() {
    for text in [".", "-.", "+", "e5", ""] {
        assert_eq!(kind(Value::varchar(text).cast_as(Decimal)), ExceptionType::Conversion, "{text:?} has no digits, so it is a Conversion error, not a zero");
    }
}

/// A number beyond what a decimal can hold is OutOfRange, not an infinity.
#[test]
fn s3a_02_text_beyond_the_decimal_range_is_out_of_range() {
    for text in ["1e400", "-1e400"] {
        assert_eq!(kind(Value::varchar(text).cast_as(Decimal)), ExceptionType::OutOfRange, "{text:?} does not fit a decimal");
    }
    assert!(Value::varchar("1e300").cast_as(Decimal).is_ok(), "1e300 does fit a decimal");
}

/// The end points of a type's range are values: the smallest and the largest cast, the number just beyond each does not. (The smallest
/// is one above the bit pattern kept for NULL.)
#[test]
fn s3a_02_a_decimal_at_the_end_of_a_types_range_fits_and_one_beyond_does_not() {
    for to in [TinyInt, SmallInt, Integer] {
        let (lo, hi) = range(to);
        let (lo, hi) = (lo as f64, hi as f64);
        assert_eq!(Value::decimal(lo).cast_as(to).unwrap().as_i64(), Some(lo as i64), "{lo} is the smallest {to:?} and fits");
        assert_eq!(Value::decimal(hi).cast_as(to).unwrap().as_i64(), Some(hi as i64), "{hi} is the largest {to:?} and fits");
        assert_eq!(kind(Value::decimal(lo - 1.0).cast_as(to)), ExceptionType::OutOfRange, "{} is below the smallest {to:?}", lo - 1.0);
        assert_eq!(kind(Value::decimal(hi + 1.0).cast_as(to)), ExceptionType::OutOfRange, "{} is above the largest {to:?}", hi + 1.0);
    }
}

// ---- 3a-03 · Comparing -----------------------------------------------------------------------------------------------------

fn all_six(a: &Value, b: &Value) -> [CmpBool; 6] {
    [a.compare_equals(b), a.compare_not_equals(b), a.compare_less_than(b), a.compare_less_than_equals(b), a.compare_greater_than(b), a.compare_greater_than_equals(b)].map(|r| r.unwrap())
}

fn truth(ord: Ordering) -> [CmpBool; 6] {
    [ord.is_eq(), ord.is_ne(), ord.is_lt(), ord.is_le(), ord.is_gt(), ord.is_ge()].map(CmpBool::from_bool)
}

proptest! {
    #![proptest_config(config())]

    /// Numbers compare by value, across integer types, exactly as `i128` does.
    #[test]
    fn s3a_03_integers_compare_across_types_exactly((a, x) in any_int(), (b, y) in any_int()) {
        prop_assert_eq!(all_six(&a, &b), truth(x.cmp(&y)), "{} against {}", x, y);
    }

    /// With a NULL on either side every comparison is NULL, neither true nor false.
    #[test]
    fn s3a_03_a_comparison_with_null_is_null(n in nulls(), (v, _) in any_int()) {
        prop_assert_eq!(all_six(&n, &v), [CmpBool::Null; 6]);
        prop_assert_eq!(all_six(&v, &n), [CmpBool::Null; 6]);
        prop_assert_eq!(all_six(&n, &n), [CmpBool::Null; 6], "even NULL = NULL is not true");
    }

    /// The six answers are consistent with each other: `=` and `<>` are opposites, `<` and `>=` are, `a < b` is `b > a`, and `<=` is `<` or `=`.
    #[test]
    fn s3a_03_the_six_comparisons_agree_with_each_other((a, _) in any_int(), (b, _) in any_int()) {
        let [eq, ne, lt, le, gt, ge] = all_six(&a, &b);
        let not = |c: CmpBool| match c { CmpBool::True => CmpBool::False, CmpBool::False => CmpBool::True, CmpBool::Null => CmpBool::Null };
        prop_assert_eq!(ne, not(eq));
        prop_assert_eq!(ge, not(lt));
        prop_assert_eq!(le, not(gt));
        let [_, _, rev_lt, _, rev_gt, _] = all_six(&b, &a);
        prop_assert_eq!(lt, rev_gt);
        prop_assert_eq!(gt, rev_lt);
    }

    /// `<` is transitive on integers.
    #[test]
    fn s3a_03_less_than_is_transitive((a, _) in any_int(), (b, _) in any_int(), (c, _) in any_int()) {
        if a.compare_less_than(&b).unwrap() == CmpBool::True && b.compare_less_than(&c).unwrap() == CmpBool::True {
            prop_assert_eq!(a.compare_less_than(&c).unwrap(), CmpBool::True);
        }
    }

    /// Decimals compare as `f64`; a decimal against an integer compares as a decimal.
    #[test]
    fn s3a_03_decimals_compare_as_floats(x in -1e6f64..1e6, (v, y) in any_int()) {
        let d = Value::decimal(x);
        prop_assert_eq!(all_six(&d, &Value::decimal(x + 1.0)), truth(Ordering::Less));
        prop_assert_eq!(all_six(&d, &v), truth(x.partial_cmp(&(y as f64)).unwrap()));
    }

    /// Strings compare byte by byte.
    #[test]
    fn s3a_03_strings_compare_bytewise(a in "[a-zA-Z0-9 ]{0,6}", b in "[a-zA-Z0-9 ]{0,6}") {
        prop_assert_eq!(all_six(&Value::varchar(&a), &Value::varchar(&b)), truth(a.as_bytes().cmp(b.as_bytes())));
    }

    /// A number compared with a string converts the string to the number's type first.
    #[test]
    fn s3a_03_a_number_and_a_string_compare_as_numbers((v, x) in any_int(), y in -1000i64..1000) {
        let s = Value::varchar(&y.to_string());
        if range(v.type_id()).0 <= y as i128 && y as i128 <= range(v.type_id()).1 {
            prop_assert_eq!(all_six(&v, &s), truth(x.cmp(&(y as i128))));
        }
    }
}

#[test]
fn s3a_03_incomparable_types_are_errors_and_exact_equality_is_for_grouping() {
    assert!(Value::boolean(true).compare_equals(&Value::integer(1)).is_err(), "a boolean and a number cannot be compared");
    assert!(Value::timestamp(1).compare_less_than(&Value::integer(1)).is_err());
    assert!(Value::null(Integer).compare_exactly_equals(&Value::null(Integer)), "two NULLs are the same group");
    assert!(Value::null(Integer).compare_exactly_equals(&Value::null(Varchar)));
    assert!(!Value::null(Integer).compare_exactly_equals(&Value::integer(1)));
    assert!(Value::integer(7).compare_exactly_equals(&Value::bigint(7)), "7 is 7 whatever the width");
    assert!(!Value::integer(7).compare_exactly_equals(&Value::integer(8)));
    assert!(!Value::boolean(true).compare_exactly_equals(&Value::integer(1)), "an error counts as not equal");
}

// ---- 3a-04 · Arithmetic ----------------------------------------------------------------------------------------------------

fn wider(a: TypeId, b: TypeId) -> TypeId {
    INT_TYPES[INT_TYPES.iter().position(|t| *t == a).unwrap().max(INT_TYPES.iter().position(|t| *t == b).unwrap())]
}

proptest! {
    #![proptest_config(config())]

    /// `+`, `-`, `*` are exact integer arithmetic in the wider of the two types; the result must fit that type, else OutOfRange.
    #[test]
    fn s3a_04_integer_arithmetic_is_exact_or_out_of_range((a, x) in any_int(), (b, y) in any_int()) {
        let t = wider(a.type_id(), b.type_id());
        let (lo, hi) = range(t);
        for (name, got, want) in [("add", a.add(&b), x + y), ("subtract", a.subtract(&b), x - y), ("multiply", a.multiply(&b), x * y)] {
            if lo <= want && want <= hi {
                let v = got.unwrap_or_else(|e| panic!("{name}({x}, {y}) = {want} fits in {t:?} but gave {e:?}"));
                prop_assert_eq!(v.type_id(), t, "{} of {:?} and {:?}", name, a.type_id(), b.type_id());
                prop_assert_eq!(v.as_i64().map(i128::from), Some(want), "{}({}, {})", name, x, y);
            } else {
                prop_assert_eq!(got.unwrap_err().kind, ExceptionType::OutOfRange, "{}({}, {}) = {} does not fit {:?}", name, x, y, want, t);
            }
        }
    }

    /// Division truncates toward zero and the remainder takes the sign of the dividend (like Rust and C++); a zero divisor is DivideByZero.
    #[test]
    fn s3a_04_division_and_modulo_follow_the_dividend((a, x) in any_int(), (b, y) in any_int()) {
        let t = wider(a.type_id(), b.type_id());
        if y == 0 {
            prop_assert_eq!(a.divide(&b).unwrap_err().kind, ExceptionType::DivideByZero);
            prop_assert_eq!(a.modulo(&b).unwrap_err().kind, ExceptionType::DivideByZero);
        } else {
            let (q, r) = (a.divide(&b).unwrap(), a.modulo(&b).unwrap());
            prop_assert_eq!((q.type_id(), r.type_id()), (t, t));
            prop_assert_eq!(q.as_i64().map(i128::from), Some(x / y));
            prop_assert_eq!(r.as_i64().map(i128::from), Some(x % y));
        }
    }

    /// Addition and multiplication commute; subtraction undoes addition when nothing overflowed.
    #[test]
    fn s3a_04_arithmetic_laws((a, _) in any_int(), (b, _) in any_int()) {
        if let (Ok(ab), Ok(ba)) = (a.add(&b), b.add(&a)) {
            prop_assert!(ab.compare_exactly_equals(&ba));
            if let Ok(back) = ab.subtract(&b) {
                prop_assert!(back.compare_exactly_equals(&a), "(a + b) - b is a");
            }
        }
        if let (Ok(ab), Ok(ba)) = (a.multiply(&b), b.multiply(&a)) {
            prop_assert!(ab.compare_exactly_equals(&ba));
        }
    }

    /// Any arithmetic with a NULL is the NULL of the result type: the wider integer type, or DECIMAL if either side is a decimal.
    #[test]
    fn s3a_04_arithmetic_with_a_null_is_a_null_of_the_result_type((v, _) in any_int(), n in nulls()) {
        let result_type = if n.type_id() == Decimal { Decimal } else { wider(v.type_id(), n.type_id()) };
        for r in [v.add(&n), n.add(&v), v.subtract(&n), v.multiply(&n), v.divide(&n), v.modulo(&n)] {
            let r = r.unwrap();
            prop_assert!(r.is_null());
            prop_assert_eq!(r.type_id(), result_type);
        }
    }

    /// With a decimal on either side the arithmetic is floating point and the result is a DECIMAL.
    #[test]
    fn s3a_04_a_decimal_makes_the_result_a_decimal(x in -1e5f64..1e5, y in 1.0f64..1e5, (v, k) in any_int()) {
        let (a, b) = (Value::decimal(x), Value::decimal(y));
        prop_assert_eq!(a.add(&b).unwrap().as_f64(), Some(x + y));
        prop_assert_eq!(a.subtract(&b).unwrap().as_f64(), Some(x - y));
        prop_assert_eq!(a.multiply(&b).unwrap().as_f64(), Some(x * y));
        prop_assert_eq!(a.divide(&b).unwrap().as_f64(), Some(x / y));
        let mixed = v.add(&a).unwrap();
        prop_assert_eq!(mixed.type_id(), Decimal);
        prop_assert_eq!(mixed.as_f64(), Some(k as f64 + x));
    }

    /// `min` and `max` are the smaller and the larger by comparison, NULL if either side is NULL.
    #[test]
    fn s3a_04_min_and_max_pick_by_comparison((a, x) in any_int(), (b, y) in any_int(), n in nulls()) {
        prop_assert_eq!(a.min(&b).unwrap().as_i64().map(i128::from), Some(x.min(y)));
        prop_assert_eq!(a.max(&b).unwrap().as_i64().map(i128::from), Some(x.max(y)));
        prop_assert!(a.min(&n).unwrap().is_null() && n.max(&a).unwrap().is_null());
    }
}

#[test]
fn s3a_04_strings_on_the_right_are_converted_and_other_types_are_refused() {
    assert_eq!(Value::integer(5).add(&Value::varchar("7")).unwrap().as_i64(), Some(12));
    assert_eq!(Value::integer(5).add(&Value::varchar("7")).unwrap().type_id(), Integer, "the result keeps the left type");
    assert_eq!(kind(Value::integer(5).divide(&Value::varchar("0"))), ExceptionType::DivideByZero);
    assert_eq!(kind(Value::boolean(true).add(&Value::boolean(true))), ExceptionType::NotImplemented);
    assert_eq!(kind(Value::varchar("1").add(&Value::integer(1))), ExceptionType::NotImplemented, "a string is only ever on the right");
}

#[test]
fn s3a_04_sqrt_and_is_zero() {
    assert_eq!(Value::integer(9).sqrt().unwrap(), Value::decimal(3.0));
    assert_eq!(Value::decimal(2.0).sqrt().unwrap().as_f64(), Some(2.0f64.sqrt()));
    assert!(Value::null(Integer).sqrt().unwrap().is_null());
    assert_eq!(kind(Value::integer(-1).sqrt()), ExceptionType::Decimal);
    assert!(Value::integer(0).is_zero().unwrap() && Value::decimal(0.0).is_zero().unwrap() && !Value::bigint(5).is_zero().unwrap());
    assert!(Value::varchar("0").is_zero().is_err());
}

// ---- 3a-05 · Storing values as bytes ---------------------------------------------------------------------------------------

fn any_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::boolean),
        any::<i8>().prop_map(Value::tinyint),
        any::<i16>().prop_map(Value::smallint),
        any::<i32>().prop_map(Value::integer),
        any::<i64>().prop_map(Value::bigint),
        (-1e12f64..1e12).prop_map(Value::decimal),
        (0u64..1_000_000_000_000).prop_map(Value::timestamp),
        "[a-z0-9 ]{0,12}".prop_map(|s| Value::varchar(&s)),
        prop::sample::select(vec![Boolean, TinyInt, SmallInt, Integer, BigInt, Decimal, Varchar, Timestamp]).prop_map(Value::null),
    ]
}

proptest! {
    #![proptest_config(config())]

    /// Any value, written at any offset of a buffer and read back as its type, comes back equal; nothing outside its `storage_size()` bytes is touched.
    #[test]
    fn s3a_05_every_value_survives_its_bytes_at_any_offset(v in any_value(), offset in 0usize..24) {
        let mut buf = vec![0xAAu8; 64];
        let size = v.storage_size();
        v.serialize_to(&mut buf[offset..]);
        prop_assert!(buf[..offset].iter().all(|&b| b == 0xAA), "wrote before its slot");
        prop_assert!(buf[offset + size..].iter().all(|&b| b == 0xAA), "wrote past its {} bytes", size);
        let back = Value::deserialize_from(&buf[offset..offset + size], v.type_id()).unwrap();
        prop_assert!(back.is_null() == v.is_null() && back.type_id() == v.type_id(), "{:?} came back as {:?}", v, back);
        prop_assert!(v.is_null() || back.compare_exactly_equals(&v), "{:?} came back as {:?}", v, back);
    }

    /// A fixed-size type takes exactly its type size.
    #[test]
    fn s3a_05_fixed_size_types_take_their_type_size(v in any_value()) {
        if v.type_id() != Varchar {
            prop_assert_eq!(v.storage_size() as u64, v.type_id().type_size().unwrap());
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// Values laid one after another, each at the offset the previous ones' sizes give (what a tuple will do), read back in order.
    #[test]
    fn s3a_05_values_laid_end_to_end_read_back_in_order(values in prop::collection::vec(any_value(), 1..12)) {
        let total: usize = values.iter().map(|v| v.storage_size()).sum();
        let mut buf = vec![0xAAu8; total + 8];
        let mut at = 0;
        for v in &values {
            v.serialize_to(&mut buf[at..]);
            at += v.storage_size();
        }
        prop_assert!(buf[total..].iter().all(|&b| b == 0xAA), "nothing written past the last value");
        let mut at = 0;
        for v in &values {
            let back = Value::deserialize_from(&buf[at..], v.type_id()).unwrap();
            prop_assert!(back.is_null() == v.is_null() && (v.is_null() || back.compare_exactly_equals(v)), "{:?} came back as {:?}", v, back);
            at += v.storage_size();
        }
    }

    /// A string takes its length plus five bytes (the length, the text, the terminator); a NULL string takes four, whatever its type is.
    #[test]
    fn s3a_05_a_string_takes_its_length_plus_five(text in "[a-z\u{e9}\u{1d11e}]{0,30}") {
        prop_assert_eq!(Value::varchar(&text).storage_size(), 4 + text.len() + 1);
        prop_assert_eq!(Value::null(Varchar).storage_size(), 4);
    }
}

#[test]
fn s3a_05_the_byte_formats_are_bustubs() {
    let bytes = |v: &Value| {
        let mut b = vec![0u8; v.storage_size()];
        v.serialize_to(&mut b);
        b
    };
    assert_eq!(bytes(&Value::integer(0x0102_0304)), vec![4, 3, 2, 1], "little-endian");
    assert_eq!(bytes(&Value::smallint(-2)), vec![0xFE, 0xFF]);
    assert_eq!(bytes(&Value::boolean(true)), vec![1]);
    assert_eq!(bytes(&Value::bigint(1)), vec![1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(bytes(&Value::varchar("ab")), vec![3, 0, 0, 0, b'a', b'b', 0], "a length that counts the zero byte, the text, a zero byte");
    assert_eq!(bytes(&Value::null(Varchar)), vec![0xFF, 0xFF, 0xFF, 0xFF], "a NULL string is the length u32::MAX and nothing else");
    assert_eq!(bytes(&Value::null(Integer)), i32::MIN.to_le_bytes().to_vec(), "a NULL is its reserved number");
    assert_eq!(bytes(&Value::null(TinyInt)), vec![0x80]);
    assert_eq!(Value::varchar("ab").storage_size(), 7);
    assert_eq!(Value::null(Varchar).storage_size(), 4);
}

#[test]
fn s3a_05_reading_a_reserved_encoding_gives_a_null_and_the_invalid_type_is_an_error() {
    for t in [TinyInt, SmallInt, Integer, BigInt, Decimal, Timestamp, Boolean] {
        let mut buf = vec![0u8; 8];
        Value::null(t).serialize_to(&mut buf);
        let v = Value::deserialize_from(&buf, t).unwrap();
        assert!(v.is_null() && v.type_id() == t, "{t:?}");
    }
    assert_eq!(kind(Value::deserialize_from(&[0u8; 8], Invalid)), ExceptionType::UnknownType);
    // only the bytes the type needs are read: a longer buffer is fine
    assert_eq!(Value::deserialize_from(&[7, 0, 0, 0, 99, 99, 99, 99], Integer).unwrap().as_i64(), Some(7));
}

// ---- 3a-06 · Boss: values end to end -----------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// The three halves of the module together: do arithmetic, store the answer in bytes, read it back, compare it with what the arithmetic gave.
    #[test]
    fn s3a_06_a_computed_value_survives_storage_and_compares_equal((a, _) in any_int(), (b, _) in any_int(), op in 0usize..5) {
        let r = match op { 0 => a.add(&b), 1 => a.subtract(&b), 2 => a.multiply(&b), 3 => a.divide(&b), _ => a.modulo(&b) };
        if let Ok(v) = r {
            let mut buf = vec![0u8; 16];
            v.serialize_to(&mut buf);
            let back = Value::deserialize_from(&buf, v.type_id()).unwrap();
            prop_assert_eq!(back.compare_equals(&v).unwrap(), CmpBool::True);
        }
    }

    /// Cast to text and back through every comparison: the number is the same number.
    #[test]
    fn s3a_06_a_value_equals_itself_after_a_cast_to_text_and_back((v, _) in any_int()) {
        let back = v.cast_as(Varchar).unwrap().cast_as(v.type_id()).unwrap();
        prop_assert_eq!(v.compare_equals(&back).unwrap(), CmpBool::True);
    }
}

// @@ challenge 3a-c1 begin
mod ch_3a_c1 {
    use proptest::prelude::*;

    use bustub::types::decimal::Decimal;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap_or_else(|| panic!("{s:?} should parse"))
    }

    #[test]
    fn s3a_c1_parsing_and_printing() {
        assert_eq!(d("12.5").to_string(), "12.50");
        assert_eq!(d("-0.07").to_string(), "-0.07");
        assert_eq!(d("+3").to_string(), "3.00");
        assert_eq!(d("-0.00").to_string(), "0.00");
        assert_eq!(d("-0.00"), d("0"));
    }

    #[test]
    fn s3a_c1_what_is_not_a_decimal() {
        for s in ["", "-", "1.234", "1.", ".5", "1e3", " 1", "1 ", "1.2.3", "--1", "0x10"] {
            assert_eq!(Decimal::parse(s), None, "{s:?}");
        }
    }

    #[test]
    fn s3a_c1_addition_is_exact_where_floats_are_not() {
        assert_eq!(d("0.1").add(d("0.2")).unwrap(), d("0.30"));
        assert_eq!(d("10").sub(d("0.01")).unwrap().to_string(), "9.99");
        assert_eq!(Decimal::from_cents(i128::MAX).add(d("0.01")), None);
        assert_eq!(Decimal::from_cents(i128::MIN).sub(d("0.01")), None);
    }

    #[test]
    fn s3a_c1_multiplication_rounds_half_away_from_zero() {
        assert_eq!(d("1.25").mul(d("1.25")).unwrap().to_string(), "1.56");
        assert_eq!(d("-1.25").mul(d("1.25")).unwrap().to_string(), "-1.56");
        assert_eq!(d("0.05").mul(d("0.10")).unwrap().to_string(), "0.01", "0.005 rounds to 0.01");
        assert_eq!(d("0.04").mul(d("0.10")).unwrap().to_string(), "0.00");
    }

    #[test]
    fn s3a_c1_division_rounds_and_refuses_zero() {
        assert_eq!(d("1").div(d("3")).unwrap().to_string(), "0.33");
        assert_eq!(d("2").div(d("3")).unwrap().to_string(), "0.67");
        assert_eq!(d("-2").div(d("3")).unwrap().to_string(), "-0.67");
        assert_eq!(d("1").div(d("0")), None);
        assert_eq!(d("5").div(d("2")).unwrap().to_string(), "2.50");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against `i128` arithmetic on hundredths, and printing then parsing is the identity.
        #[test]
        fn s3a_c1_property_decimals_match_integer_arithmetic(a in -100_000i128..100_000, b in -100_000i128..100_000) {
            let (x, y) = (Decimal::from_cents(a), Decimal::from_cents(b));
            prop_assert_eq!(Decimal::parse(&x.to_string()), Some(x));
            prop_assert_eq!(x.add(y).unwrap().cents(), a + b);
            prop_assert_eq!(x.sub(y).unwrap().cents(), a - b);
            let rnd = |n: i128, dd: i128| { let q = n / dd; let r = n % dd; if r.abs() * 2 >= dd.abs() { q + if (n < 0) != (dd < 0) { -1 } else { 1 } } else { q } };
            prop_assert_eq!(x.mul(y).unwrap().cents(), rnd(a * b, 100));
            if b != 0 { prop_assert_eq!(x.div(y).unwrap().cents(), rnd(a * 100, b)); } else { prop_assert_eq!(x.div(y), None); }
            prop_assert_eq!(x.add(y), y.add(x));
            prop_assert_eq!(x.mul(y), y.mul(x));
            prop_assert_eq!(x.mul(Decimal::from_cents(100)), Some(x));
        }
    }
}
// @@ challenge 3a-c1 end

// @@ challenge 3a-c2 begin
mod ch_3a_c2 {
    use proptest::prelude::*;

    use bustub::types::null_order::{cmp_nullable, Dir, Nulls};

    fn sorted(v: &[Option<i64>], dir: Dir, nulls: Nulls) -> Vec<Option<i64>> {
        let mut v = v.to_vec();
        v.sort_by(|a, b| cmp_nullable(*a, *b, dir, nulls));
        v
    }

    const ROWS: [Option<i64>; 3] = [Some(3), None, Some(1)];

    #[test]
    fn s3a_c2_ascending_nulls_last() {
        assert_eq!(sorted(&ROWS, Dir::Asc, Nulls::Last), vec![Some(1), Some(3), None]);
    }

    #[test]
    fn s3a_c2_descending_nulls_first_puts_nulls_first_and_values_downwards() {
        assert_eq!(sorted(&ROWS, Dir::Desc, Nulls::First), vec![None, Some(3), Some(1)]);
    }

    #[test]
    fn s3a_c2_the_placement_is_independent_of_the_direction() {
        assert_eq!(sorted(&ROWS, Dir::Asc, Nulls::First), vec![None, Some(1), Some(3)]);
        assert_eq!(sorted(&ROWS, Dir::Desc, Nulls::Last), vec![Some(3), Some(1), None]);
    }

    #[test]
    fn s3a_c2_two_nulls_are_equal_and_sorting_is_stable() {
        use std::cmp::Ordering::Equal;
        assert_eq!(cmp_nullable(None, None, Dir::Desc, Nulls::First), Equal);
        let mut tagged = vec![(None, 'a'), (Some(1), 'b'), (None, 'c'), (Some(1), 'd')];
        tagged.sort_by(|x, y| cmp_nullable(x.0, y.0, Dir::Asc, Nulls::Last));
        assert_eq!(tagged.iter().map(|t| t.1).collect::<String>(), "bdac");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a total preorder whose NULL placement ignores the direction and whose value order flips with it.
        #[test]
        fn s3a_c2_property_a_total_preorder_with_independent_options(a in proptest::option::of(-3i64..3), b in proptest::option::of(-3i64..3), c in proptest::option::of(-3i64..3)) {
            for dir in [Dir::Asc, Dir::Desc] {
                for nulls in [Nulls::First, Nulls::Last] {
                    let f = |x, y| cmp_nullable(x, y, dir, nulls);
                    prop_assert_eq!(f(a, b), f(b, a).reverse());
                    prop_assert_eq!(f(a, a), std::cmp::Ordering::Equal);
                    if f(a, b).is_le() && f(b, c).is_le() { prop_assert!(f(a, c).is_le()); }
                }
            }
            for nulls in [Nulls::First, Nulls::Last] {
                let asc = cmp_nullable(a, b, Dir::Asc, nulls);
                let desc = cmp_nullable(a, b, Dir::Desc, nulls);
                if a.is_some() && b.is_some() { prop_assert_eq!(desc, asc.reverse()); } else { prop_assert_eq!(desc, asc); }
            }
        }
    }
}
// @@ challenge 3a-c2 end

// @@ challenge 3a-c3 begin
mod ch_3a_c3 {
    use proptest::prelude::*;

    use bustub::types::int_parse::{parse_sql_int, IntError};

    #[test]
    fn s3a_c3_numbers_with_surrounding_whitespace_and_signs() {
        assert_eq!(parse_sql_int("  42 "), Ok(42));
        assert_eq!(parse_sql_int("+5"), Ok(5));
        assert_eq!(parse_sql_int("-0"), Ok(0));
        assert_eq!(parse_sql_int("\t-17\n"), Ok(-17));
        assert_eq!(parse_sql_int("007"), Ok(7));
    }

    #[test]
    fn s3a_c3_the_edges_of_i64() {
        assert_eq!(parse_sql_int("9223372036854775807"), Ok(i64::MAX));
        assert_eq!(parse_sql_int("-9223372036854775808"), Ok(i64::MIN));
        assert_eq!(parse_sql_int("9223372036854775808"), Err(IntError::OutOfRange));
        assert_eq!(parse_sql_int("-9223372036854775809"), Err(IntError::OutOfRange));
        assert_eq!(parse_sql_int("99999999999999999999999999"), Err(IntError::OutOfRange));
    }

    #[test]
    fn s3a_c3_empty_and_invalid_are_different_errors() {
        assert_eq!(parse_sql_int(""), Err(IntError::Empty));
        assert_eq!(parse_sql_int("   "), Err(IntError::Empty));
        for s in ["+", "-", "4 2", "1_0", "0x1", "1.0", "1e3", "--1", "+-1", "12a", "\u{0663}"] {
            assert_eq!(parse_sql_int(s), Err(IntError::Invalid), "{s:?}");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every printed `i64` parses back, whitespace changes nothing, and everything else agrees with 128-bit parsing.
        #[test]
        fn s3a_c3_property_round_trip_and_range(v in any::<i64>(), w in 0usize..3, digits in "[0-9]{1,24}", neg in any::<bool>()) {
            prop_assert_eq!(parse_sql_int(&v.to_string()), Ok(v));
            let padded = format!("{}{}{}", " ".repeat(w), v, "\t".repeat(w));
            prop_assert_eq!(parse_sql_int(&padded), Ok(v));
            let text = format!("{}{}", if neg { "-" } else { "" }, digits);
            let wide: i128 = text.parse().unwrap_or(i128::MAX);
            let want = if wide >= i64::MIN as i128 && wide <= i64::MAX as i128 && digits.len() < 38 { Ok(wide as i64) } else { Err(IntError::OutOfRange) };
            prop_assert_eq!(parse_sql_int(&text), want);
        }

        /// Property: never panics on any string.
        #[test]
        fn s3a_c3_property_never_panics(s in ".{0,12}") {
            let _ = parse_sql_int(&s);
        }
    }
}
// @@ challenge 3a-c3 end

// @@ challenge 3a-c4 begin
mod ch_3a_c4 {
    use proptest::prelude::*;

    use bustub::types::int_ops::{checked_abs_i32, checked_add_i32, checked_div_i32, checked_neg_i32, IntOpError};

    #[test]
    fn s3a_c4_ordinary_values() {
        assert_eq!(checked_add_i32(2, 3), Ok(5));
        assert_eq!(checked_neg_i32(7), Ok(-7));
        assert_eq!(checked_abs_i32(-7), Ok(7));
        assert_eq!(checked_div_i32(-7, 2), Ok(-3), "truncation towards zero, as SQL does");
    }

    #[test]
    fn s3a_c4_division_by_zero_is_an_error() {
        assert_eq!(checked_div_i32(5, 0), Err(IntOpError::DivisionByZero));
        assert_eq!(checked_div_i32(i32::MIN, 0), Err(IntOpError::DivisionByZero));
    }

    #[test]
    fn s3a_c4_the_one_division_that_overflows_is_an_error_not_a_panic() {
        assert_eq!(checked_div_i32(i32::MIN, -1), Err(IntOpError::Overflow));
        assert_eq!(checked_div_i32(i32::MIN, 1), Ok(i32::MIN));
        assert_eq!(checked_div_i32(i32::MAX, -1), Ok(-i32::MAX));
    }

    #[test]
    fn s3a_c4_the_other_extremes() {
        assert_eq!(checked_abs_i32(i32::MIN), Err(IntOpError::Overflow));
        assert_eq!(checked_neg_i32(i32::MIN), Err(IntOpError::Overflow));
        assert_eq!(checked_add_i32(i32::MAX, 1), Err(IntOpError::Overflow));
        assert_eq!(checked_add_i32(i32::MIN, -1), Err(IntOpError::Overflow));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: never panics, and agrees with 64-bit arithmetic, over values that include the extremes.
        #[test]
        fn s3a_c4_property_agrees_with_64_bit_arithmetic(a in prop_oneof![any::<i32>(), Just(i32::MIN), Just(i32::MAX), Just(0), Just(-1), Just(1)], b in prop_oneof![any::<i32>(), Just(i32::MIN), Just(i32::MAX), Just(0), Just(-1), Just(1)]) {
            let fit = |x: i64| i32::try_from(x).map_err(|_| IntOpError::Overflow);
            prop_assert_eq!(checked_add_i32(a, b), fit(a as i64 + b as i64));
            prop_assert_eq!(checked_neg_i32(a), fit(-(a as i64)));
            prop_assert_eq!(checked_abs_i32(a), fit((a as i64).abs()));
            let want = if b == 0 { Err(IntOpError::DivisionByZero) } else { fit(a as i64 / b as i64) };
            prop_assert_eq!(checked_div_i32(a, b), want);
        }
    }
}
// @@ challenge 3a-c4 end

// @@ challenge 3a-c5 begin
mod ch_3a_c5 {
    use proptest::prelude::*;

    use bustub::types::like_match::{like, LikeError};

    #[test]
    fn s3a_c5_wildcards_and_literals() {
        assert_eq!(like("hello", "h%o", None), Ok(true));
        assert_eq!(like("hello", "h_llo", None), Ok(true));
        assert_eq!(like("hello", "h_lo", None), Ok(false));
        assert_eq!(like("hello", "%", None), Ok(true));
        assert_eq!(like("", "%", None), Ok(true));
        assert_eq!(like("", "_", None), Ok(false));
        assert_eq!(like("hello", "HELLO", None), Ok(false), "case-sensitive");
        assert_eq!(like("hello", "hell", None), Ok(false), "the whole text must match");
    }

    #[test]
    fn s3a_c5_the_escape_character_makes_the_next_character_literal() {
        assert_eq!(like("100%", "100\\%", Some('\\')), Ok(true));
        assert_eq!(like("1000", "100\\%", Some('\\')), Ok(false));
        assert_eq!(like("a_b", "a\\_b", Some('\\')), Ok(true));
        assert_eq!(like("axb", "a\\_b", Some('\\')), Ok(false));
        assert_eq!(like("a\\b", "a\\\\b", Some('\\')), Ok(true), "an escaped escape is a literal escape");
    }

    #[test]
    fn s3a_c5_a_trailing_escape_is_an_error() {
        assert_eq!(like("ab", "a\\", Some('\\')), Err(LikeError::TrailingEscape));
        assert_eq!(like("a\\", "a\\", None), Ok(true), "without an escape character it is just a character");
    }

    #[test]
    fn s3a_c5_unicode_characters_count_as_one() {
        assert_eq!(like("héllo", "h_llo", None), Ok(true));
        assert_eq!(like("日本語", "___", None), Ok(true));
        assert_eq!(like("日本語", "__", None), Ok(false));
    }

    #[test]
    fn s3a_c5_an_adversarial_pattern_finishes_quickly() {
        let text = "a".repeat(20_000);
        let pat = format!("{}b", "%a".repeat(12));
        let t = std::time::Instant::now();
        assert_eq!(like(&text, &pat, None), Ok(false));
        assert!(t.elapsed() < std::time::Duration::from_secs(3), "took {:?}: a recursive matcher is exponential here", t.elapsed());
    }

    fn brute(t: &[char], p: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some((&'%', rest)) => (0..=t.len()).any(|i| brute(&t[i..], rest)),
            Some((&'_', rest)) => !t.is_empty() && brute(&t[1..], rest),
            Some((c, rest)) => t.first() == Some(c) && brute(&t[1..], rest),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a brute-force recursive matcher on short strings over a small alphabet.
        #[test]
        fn s3a_c5_property_matches_a_brute_force_matcher(text in "[ab]{0,8}", pattern in "[ab%_]{0,7}") {
            let t: Vec<char> = text.chars().collect();
            let p: Vec<char> = pattern.chars().collect();
            prop_assert_eq!(like(&text, &pattern, None), Ok(brute(&t, &p)), "{:?} LIKE {:?}", text, pattern);
        }
    }
}
// @@ challenge 3a-c5 end
