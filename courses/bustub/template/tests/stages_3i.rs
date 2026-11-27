//! Tests for module 3i: the SQL surface BusTub does not have: more operators, `BETWEEN` and `IN`, `CASE`, `COALESCE` and `NULLIF`,
//! `LIKE`, `OFFSET` and `DISTINCT` aggregates, `EXPLAIN ANALYZE` and prepared statements.

use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::*;

use bustub::catalog::schema::Schema;
use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::exception::{Exception, ExceptionType};
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::execution::expressions::abstract_expression::{ExprRef, Expression};
use bustub::execution::expressions::constant_value_expression::ConstantValueExpression;
use bustub::execution::expressions::like_expression::like_matches;
use bustub::execution::expressions::operator_expression::{Operator, OperatorExpression};
use bustub::planner::planner::Planner;
use bustub::storage::table::tuple::Tuple;
use bustub::types::type_id::TypeId;
use bustub::types::value::Value;

fn run(db: &BusTubInstance, sql: &str) -> Result<Vec<String>, Exception> {
    let mut out = String::new();
    db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None)?;
    Ok(out.lines().map(|l| l.trim_end().to_string()).collect())
}

/// The rows of `sql`, one string per row (cells separated by one space; NULLs print as `integer_null`, `varlen_null`, `boolean_null`).
fn rows(db: &BusTubInstance, sql: &str) -> Vec<String> {
    run(db, sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"))
}

fn error_kind(db: &BusTubInstance, sql: &str) -> ExceptionType {
    match run(db, sql) {
        Ok(r) => panic!("{sql}: expected an error, got {r:?}"),
        Err(e) => e.kind,
    }
}

fn db() -> BusTubInstance {
    BusTubInstance::new(64)
}

/// `t(a, b)` with a NULL in each column and a row where b is zero.
fn db_with_t() -> BusTubInstance {
    let db = db();
    rows(&db, "create table t(a int, b int)");
    rows(&db, "insert into t values (1, 10), (2, 20), (null, 30), (4, 0), (5, null)");
    db
}

/// `w(name)`: words that LIKE patterns can be tried on (no NULLs: the engine cannot store a NULL string).
fn db_with_words() -> BusTubInstance {
    let db = db();
    rows(&db, "create table w(name varchar(20))");
    rows(&db, "insert into w values ('apple'), ('banana'), ('cherry'), ('a_b%'), ('Apple'), ('')");
    db
}

fn constant(v: Value) -> ExprRef {
    Arc::new(ConstantValueExpression::new(v))
}

fn eval(e: &ExprRef) -> Value {
    e.evaluate(&Tuple::new(&[], &Schema::new(vec![])), &Schema::new(vec![])).unwrap()
}

// ---- 3i-01 · operators BusTub left out ----------------------------------------------------------------------------------------

#[test]
fn s3i_01_multiplication_division_and_remainder_follow_the_usual_precedence() {
    let db = db();
    assert_eq!(
        rows(&db, "select 2 + 3 * 4, (2 + 3) * 4, 17 / 5, 17 % 5"),
        ["14 20 3 2"],
        "* binds tighter than +, / truncates, % is the remainder"
    );
    assert_eq!(
        rows(&db, "select -17 / 5, -17 % 5, 10 - 2 * 3 - 1"),
        ["-3 -2 3"],
        "division and remainder round toward zero; operators of one level go left to right"
    );
}

#[test]
fn s3i_01_a_null_operand_makes_the_arithmetic_result_null() {
    let db = db();
    assert_eq!(
        rows(&db, "select 1 * null, null / 2, null % 3, -null"),
        ["integer_null integer_null integer_null integer_null"],
        "an arithmetic operator with a NULL operand returns NULL"
    );
}

#[test]
fn s3i_01_division_by_zero_and_overflow_are_errors_not_panics() {
    let db = db();
    assert_eq!(error_kind(&db, "select 1 / 0"), ExceptionType::DivideByZero, "division by zero is an error");
    assert_eq!(error_kind(&db, "select 5 % 0"), ExceptionType::DivideByZero, "remainder by zero is an error");
    assert_eq!(
        error_kind(&db, "select 65536 * 65536"),
        ExceptionType::OutOfRange,
        "an INTEGER product that does not fit is an out-of-range error"
    );
}

#[test]
fn s3i_01_plus_and_minus_also_work_on_decimals_and_mixed_numbers() {
    let db = db();
    assert_eq!(
        rows(&db, "select 1.5 + 2, 5 - 0.5, 1.5 * 2, 7 / 2.0"),
        ["3.500000 4.500000 3.000000 3.500000"],
        "an operator on a decimal and an integer gives a decimal"
    );
}

#[test]
fn s3i_01_not_flips_a_boolean_and_keeps_null_as_null() {
    let db = db_with_t();
    assert_eq!(rows(&db, "select not true, not false, not (1 > null)"), ["false true boolean_null"], "NOT of NULL is NULL");
    assert_eq!(rows(&db, "select a from t where not (a > 2)"), ["1", "2"], "a row whose condition is NULL is not selected by NOT either");
}

#[test]
fn s3i_01_is_null_and_is_not_null_never_return_null() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, a is null, a is not null from t where b = 30"),
        ["integer_null true false"],
        "IS NULL is true for a NULL"
    );
    assert_eq!(rows(&db, "select count(*) from t where a is null"), ["1"], "IS NULL in WHERE");
    assert_eq!(rows(&db, "select count(*) from t where b is not null"), ["4"], "IS NOT NULL in WHERE");
    assert_eq!(
        rows(&db, "select null is null, 1 is null, (1 > null) is null"),
        ["true false true"],
        "IS NULL on constants and on a NULL comparison"
    );
}

#[test]
fn s3i_01_concatenation_joins_strings_and_propagates_null() {
    let db = db_with_words();
    assert_eq!(rows(&db, "select 'ab' || 'cd'"), ["abcd"], "|| joins two strings");
    assert_eq!(rows(&db, "select name || '!' from w where name = 'apple'"), ["apple!"], "|| on a column");
    assert_eq!(rows(&db, "select 'a' || 'b' || 'c', '' || 'x'"), ["abc x"], "|| chains, and the empty string is an ordinary string");
}

#[test]
fn s3i_01_operators_work_inside_aggregates_and_filters() {
    let db = db_with_t();
    assert_eq!(rows(&db, "select sum(a * 2), sum(-a) from t"), ["24 -12"], "an operator inside an aggregate");
    assert_eq!(rows(&db, "select a from t where a * 2 > 4"), ["4", "5"], "an operator in WHERE");
    assert_eq!(rows(&db, "select -sum(a) from t"), ["-12"], "an operator over an aggregate");
}

#[test]
fn s3i_01_operands_of_the_wrong_type_are_refused_by_name() {
    let db = db();
    assert_eq!(error_kind(&db, "select 'a' * 2"), ExceptionType::NotImplemented, "arithmetic on a string is refused");
    assert_eq!(error_kind(&db, "select 1 || 2"), ExceptionType::NotImplemented, "|| needs strings");
    assert_eq!(error_kind(&db, "select not 1"), ExceptionType::NotImplemented, "NOT needs a boolean");
}

#[test]
fn s3i_01_the_expression_computes_and_types_its_result() {
    let (two, three, half) = (constant(Value::integer(2)), constant(Value::integer(3)), constant(Value::decimal(0.5)));
    let product = OperatorExpression::new(Operator::Multiply, vec![two.clone(), three.clone()]).unwrap();
    assert_eq!(product.return_type().type_id(), TypeId::Integer, "integer times integer is INTEGER");
    let product: ExprRef = Arc::new(product);
    assert_eq!(eval(&product), Value::integer(6), "2 * 3");
    assert_eq!(
        OperatorExpression::new(Operator::Multiply, vec![two.clone(), half]).unwrap().return_type().type_id(),
        TypeId::Decimal,
        "a decimal operand makes the result DECIMAL"
    );
    assert!(OperatorExpression::new(Operator::Concat, vec![two.clone(), three]).is_err(), "|| refuses numbers");
    assert!(OperatorExpression::new(Operator::Negate, vec![two.clone(), two]).is_err(), "unary minus takes one operand");
    let factory = Planner::get_binary_expression_from_factory("*", constant(Value::integer(7)), constant(Value::integer(6))).unwrap();
    assert_eq!(eval(&factory), Value::integer(42), "the planner's factory knows *");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: `x * y + z`, `x / y` and `x % y` on small and large integers agree with i64 arithmetic, with an error exactly where
    /// the result does not fit in an INTEGER (or the divisor is zero).
    #[test]
    fn s3i_01_property_arithmetic_agrees_with_i64(x in -50_000i64..50_000, y in -50_000i64..50_000, z in -1_000_000i64..1_000_000) {
        let db = db();
        let fits = |v: i64| v > i32::MIN as i64 && v <= i32::MAX as i64;
        let sql = format!("select ({x}) * ({y}) + ({z})");
        let want = x * y + z;
        match run(&db, &sql) {
            Ok(r) => { prop_assert!(fits(want), "{} should not fit", sql); prop_assert_eq!(r, vec![want.to_string()]); }
            Err(e) => { prop_assert!(!fits(want), "{} fits but failed: {:?}", sql, e); }
        }
        if y != 0 {
            prop_assert_eq!(rows(&db, &format!("select ({x}) / ({y}), ({x}) % ({y})")), vec![format!("{} {}", x / y, x % y)]);
        } else {
            prop_assert_eq!(error_kind(&db, &format!("select ({x}) / ({y})")), ExceptionType::DivideByZero);
        }
    }
}

// ---- 3i-02 · BETWEEN and IN ---------------------------------------------------------------------------------------------------

#[test]
fn s3i_02_between_includes_both_ends_and_skips_nulls() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a from t where a between 2 and 4"),
        ["2", "4"],
        "BETWEEN includes both bounds and a NULL is not between anything"
    );
    assert_eq!(
        rows(&db, "select a from t where a not between 2 and 4"),
        ["1", "5"],
        "NOT BETWEEN selects the others, but still not the NULL"
    );
}

#[test]
fn s3i_02_between_with_swapped_bounds_selects_nothing() {
    let db = db_with_t();
    assert_eq!(rows(&db, "select a from t where a between 4 and 2"), Vec::<String>::new(), "x BETWEEN 4 AND 2 is x >= 4 AND x <= 2");
    assert_eq!(rows(&db, "select a from t where a not between 4 and 2"), ["1", "2", "4", "5"], "so NOT BETWEEN selects every non-NULL row");
}

#[test]
fn s3i_02_in_matches_any_item_of_the_list() {
    let db = db_with_t();
    assert_eq!(rows(&db, "select a from t where a in (1, 4, 99)"), ["1", "4"], "IN matches any listed value");
    assert_eq!(rows(&db, "select a from t where a in (3)"), Vec::<String>::new(), "a one-item list works");
    assert_eq!(rows(&db, "select a from t where a in (b / 10, 5)"), ["1", "2", "5"], "items are expressions of the row");
    let w = db_with_words();
    assert_eq!(rows(&w, "select name from w where name in ('apple', 'cherry')"), ["apple", "cherry"], "IN on strings");
}

#[test]
fn s3i_02_not_in_with_a_null_in_the_list_selects_nothing() {
    let db = db_with_t();
    assert_eq!(rows(&db, "select a from t where a not in (1, 4)"), ["2", "5"], "NOT IN without NULLs selects the rest (not the NULL row)");
    assert_eq!(
        rows(&db, "select a from t where a not in (1, null)"),
        Vec::<String>::new(),
        "x NOT IN (1, NULL) is never TRUE: it is FALSE for 1 and NULL for everything else"
    );
    assert_eq!(
        rows(&db, "select a from t where a in (1, null)"),
        ["1"],
        "x IN (1, NULL) is TRUE for 1 and NULL (not selected) for the rest"
    );
}

#[test]
fn s3i_02_a_null_on_the_left_is_never_in_and_never_not_in() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select null in (1, 2), null not in (1, 2), 1 in (2, 3)"),
        ["boolean_null boolean_null false"],
        "NULL IN list is NULL"
    );
    assert_eq!(
        rows(&db, "select count(*) from t where a in (1, 2, 4, 5) or a not in (1, 2, 4, 5)"),
        ["4"],
        "the NULL row satisfies neither IN nor NOT IN"
    );
}

#[test]
fn s3i_02_the_between_and_does_not_end_the_condition() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a from t where a between 1 and 4 and b >= 10"),
        ["1", "2"],
        "the AND after the bounds belongs to the WHERE"
    );
    assert_eq!(rows(&db, "select a from t where a + 1 between 2 and 3"), ["1", "2"], "the bounds bind looser than +");
    assert_eq!(
        rows(&db, "select a from t where not a in (1, 2) and a is not null"),
        ["4", "5"],
        "NOT before the predicate negates the whole test"
    );
}

#[test]
fn s3i_02_a_subquery_in_the_list_is_not_part_of_this_module() {
    let db = db_with_t();
    assert!(
        run(&db, "select a from t where a in (select b from t)").is_err(),
        "IN (subquery) is a later module: refuse it, do not misparse it"
    );
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: IN, NOT IN and BETWEEN on rows and lists with NULLs agree with three-valued logic written out in Rust.
    #[test]
    fn s3i_02_property_predicates_follow_three_valued_logic(
        values in proptest::collection::vec(proptest::option::of(0i32..6), 1..8),
        list in proptest::collection::vec(proptest::option::of(0i32..6), 1..4),
        lo in 0i32..6, hi in 0i32..6,
    ) {
        let db = db();
        rows(&db, "create table p(a int)");
        let tuples: Vec<String> = values.iter().map(|v| format!("({})", v.map_or("null".to_string(), |v| v.to_string()))).collect();
        rows(&db, &format!("insert into p values {}", tuples.join(", ")));
        let items: Vec<String> = list.iter().map(|v| v.map_or("null".to_string(), |v| v.to_string())).collect();
        let list_sql = items.join(", ");
        // three-valued IN: TRUE if any item equals, else NULL if any comparison is NULL, else FALSE
        let in3 = |a: Option<i32>| -> Option<bool> {
            let cmp: Vec<Option<bool>> = list.iter().map(|i| match (a, i) { (Some(a), Some(i)) => Some(a == *i), _ => None }).collect();
            if cmp.contains(&Some(true)) { Some(true) } else if cmp.contains(&None) { None } else { Some(false) }
        };
        let select = |pred: &dyn Fn(Option<i32>) -> Option<bool>| -> Vec<String> {
            values.iter().filter(|v| pred(**v) == Some(true)).map(|v| v.map_or("integer_null".into(), |v| v.to_string())).collect()
        };
        let mut got = rows(&db, &format!("select a from p where a in ({list_sql})"));
        let mut want = select(&|a| in3(a));
        got.sort(); want.sort();
        prop_assert_eq!(got, want, "IN ({})", list_sql);
        let mut got = rows(&db, &format!("select a from p where a not in ({list_sql})"));
        let mut want = select(&|a| in3(a).map(|b| !b));
        got.sort(); want.sort();
        prop_assert_eq!(got, want, "NOT IN ({})", list_sql);
        let mut got = rows(&db, &format!("select a from p where a between {lo} and {hi}"));
        let mut want = select(&|a| a.map(|a| lo <= a && a <= hi));
        got.sort(); want.sort();
        prop_assert_eq!(got, want, "BETWEEN {} AND {}", lo, hi);
    }
}

// ---- 3i-03 · CASE, COALESCE and NULLIF ----------------------------------------------------------------------------------------

#[test]
fn s3i_03_a_searched_case_takes_the_first_branch_that_is_true() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, case when a >= 4 then 'big' when a >= 2 then 'mid' else 'small' end from t where a is not null"),
        ["1 small", "2 mid", "4 big", "5 big"],
        "the first true condition wins, and ELSE catches the rest"
    );
}

#[test]
fn s3i_03_without_else_an_unmatched_case_is_null_and_a_null_condition_is_not_a_match() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, case when a > 2 then 'x' end from t where b = 20 or b = 30"),
        ["2 varlen_null", "integer_null varlen_null"],
        "no ELSE: NULL; a NULL condition is not TRUE"
    );
    assert_eq!(
        rows(&db, "select case when a > 2 then 'x' else 'y' end from t where b = 30"),
        ["y"],
        "a condition that is NULL falls through to ELSE"
    );
}

#[test]
fn s3i_03_the_operand_form_compares_with_equals() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, case a when 1 then 100 when 4 then 400 else 0 end from t where b >= 10 or b = 0"),
        ["1 100", "2 0", "integer_null 0", "4 400"],
        "CASE x WHEN v is x = v"
    );
    assert_eq!(rows(&db, "select case null when null then 1 else 2 end"), ["2"], "NULL = NULL is not TRUE, so WHEN NULL never matches");
}

#[test]
fn s3i_03_only_the_chosen_branch_is_evaluated() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, case when b = 0 then -1 else a / b end from t where b is not null and a is not null"),
        ["1 0", "2 0", "4 -1"],
        "a / 0 in a branch that is not taken must not fail"
    );
    assert_eq!(
        rows(&db, "select a, case when b <> 0 then a / b else -1 end from t where b is not null and a is not null"),
        ["1 0", "2 0", "4 -1"],
        "the same guard with the division in the THEN branch"
    );
    assert_eq!(
        error_kind(&db, "select a / b from t where b is not null"),
        ExceptionType::DivideByZero,
        "the same division without CASE does fail"
    );
}

#[test]
fn s3i_03_coalesce_is_the_first_argument_that_is_not_null() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, coalesce(a, b, -1) from t where b >= 20 or b is null"),
        ["2 2", "integer_null 30", "5 5"],
        "the first non-NULL argument"
    );
    assert_eq!(
        rows(&db, "select coalesce(null, null, 7), coalesce(null, null), coalesce(5)"),
        ["7 integer_null 5"],
        "all NULL gives NULL; one argument is itself"
    );
}

#[test]
fn s3i_03_nullif_turns_a_value_into_null() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select a, nullif(a, 2) from t where a in (1, 2)"),
        ["1 1", "2 integer_null"],
        "NULLIF(a, b) is NULL when a = b, else a"
    );
    assert_eq!(
        rows(&db, "select a / nullif(b, 0) from t where a = 4"),
        ["integer_null"],
        "the classic: dividing by NULLIF(x, 0) gives NULL instead of an error"
    );
    assert_eq!(rows(&db, "select nullif('a', 'a') || 'x'"), ["varlen_null"], "a NULL string makes a concatenation NULL");
}

#[test]
fn s3i_03_branches_must_have_one_type_and_a_bare_null_takes_it() {
    let db = db_with_t();
    assert_eq!(
        error_kind(&db, "select case when a > 1 then 'x' else 1 end from t"),
        ExceptionType::MismatchType,
        "VARCHAR and INTEGER results do not mix"
    );
    assert_eq!(error_kind(&db, "select case when 1 then 'x' else 'y' end"), ExceptionType::MismatchType, "a condition must be a boolean");
    assert_eq!(rows(&db, "select case when 1 > 2 then 'x' else null end"), ["varlen_null"], "a NULL branch takes the type of the others");
}

#[test]
fn s3i_03_case_works_inside_aggregates_and_around_them() {
    let db = db_with_t();
    assert_eq!(
        rows(&db, "select sum(case when a > 1 then 1 else 0 end), count(case when a > 1 then 1 end) from t"),
        ["3 3"],
        "the classic conditional count"
    );
    assert_eq!(rows(&db, "select case when count(*) > 3 then 'many' else 'few' end from t"), ["many"], "a CASE over an aggregate");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: COALESCE, NULLIF and a searched CASE agree with their definitions on every row.
    #[test]
    fn s3i_03_property_conditionals_follow_their_definitions(vals in proptest::collection::vec((proptest::option::of(-5i32..5), proptest::option::of(-5i32..5)), 1..8), k in -5i32..5) {
        let db = db();
        rows(&db, "create table p(a int, b int)");
        let lit = |v: &Option<i32>| v.map_or("null".to_string(), |v| v.to_string());
        let tuples: Vec<String> = vals.iter().map(|(a, b)| format!("({}, {})", lit(a), lit(b))).collect();
        rows(&db, &format!("insert into p values {}", tuples.join(", ")));
        let cell = |v: Option<i32>| v.map_or("integer_null".to_string(), |v| v.to_string());
        let got = rows(&db, &format!("select coalesce(a, b, {k}), nullif(a, b), case when a > b then a when a < b then b else {k} end from p"));
        let want: Vec<String> = vals.iter().map(|(a, b)| {
            let co = a.or(*b).unwrap_or(k);
            let ni = if a.is_some() && a == b { None } else { *a };
            let ca = match (a, b) { (Some(a), Some(b)) if a > b => *a, (Some(a), Some(b)) if a < b => *b, _ => k };
            format!("{} {} {}", cell(Some(co)), cell(ni), cell(Some(ca)))
        }).collect();
        prop_assert_eq!(got, want);
    }
}

// ---- 3i-04 · LIKE -------------------------------------------------------------------------------------------------------------

#[test]
fn s3i_04_percent_matches_any_run_and_underscore_matches_one_character() {
    assert!(like_matches("apple", "a%"), "% at the end");
    assert!(like_matches("apple", "%le"), "% at the start");
    assert!(like_matches("apple", "a%e"), "% in the middle");
    assert!(like_matches("apple", "%"), "% alone matches everything");
    assert!(like_matches("", "%"), "% matches the empty string");
    assert!(like_matches("apple", "_pple"), "_ is one character");
    assert!(!like_matches("pple", "_pple"), "_ is exactly one character, not zero");
    assert!(!like_matches("apple", "app"), "a pattern without % must match the whole text");
    assert!(like_matches("a%a%a", "a%a%a"), "repeated % ");
}

#[test]
fn s3i_04_a_backslash_makes_the_next_character_literal() {
    assert!(like_matches("100%", "100\\%"), "an escaped % is a percent sign");
    assert!(!like_matches("1000", "100\\%"), "and not a wildcard");
    assert!(like_matches("a_b", "a\\_b"), "an escaped _ is an underscore");
    assert!(!like_matches("axb", "a\\_b"), "and not a wildcard");
    assert!(like_matches("a\\b", "a\\\\b"), "an escaped backslash");
}

#[test]
fn s3i_04_characters_are_unicode_characters_and_matching_is_case_sensitive() {
    assert!(like_matches("é", "_"), "_ matches one character, whatever its width in bytes");
    assert!(like_matches("naïve", "na_ve"), "inside a word");
    assert!(!like_matches("Apple", "apple"), "LIKE is case sensitive");
    assert!(like_matches("日本語", "日%語"), "wildcards around multi-byte characters");
}

#[test]
fn s3i_04_like_in_sql_with_not_and_with_a_pattern_taken_from_a_column() {
    let db = db_with_words();
    assert_eq!(rows(&db, "select name from w where name like 'a%'"), ["apple", "a_b%"], "LIKE in WHERE");
    assert_eq!(rows(&db, "select name from w where name like '_____'"), ["apple", "Apple"], "five characters");
    assert_eq!(
        rows(&db, "select name from w where name not like '%a%' and name <> ''"),
        ["cherry", "Apple"],
        "NOT LIKE (the capital A is not an a)"
    );
    assert_eq!(
        rows(&db, "select name from w where name like 'a\\_b\\%'"),
        ["a_b%"],
        "escapes through SQL: the string literal keeps the backslash"
    );
    assert_eq!(
        rows(&db, "select count(*) from w where 'xapplex' like '%' || name || '%'"),
        ["2"],
        "the pattern may be an expression: '' and 'apple' occur in the text"
    );
}

#[test]
fn s3i_04_a_null_operand_gives_null_and_non_strings_are_refused() {
    let db = db_with_words();
    assert_eq!(
        rows(&db, "select nullif('a', 'a') like 'a%', 'a' like nullif('a', 'a'), 'a' not like nullif('a', 'a')"),
        ["boolean_null boolean_null boolean_null"],
        "NULL LIKE x, x LIKE NULL and NOT LIKE are NULL"
    );
    assert_eq!(error_kind(&db, "select 1 like 'a'"), ExceptionType::NotImplemented, "LIKE needs strings");
}

#[test]
fn s3i_04_a_pattern_with_many_wildcards_does_not_take_exponential_time() {
    let text = "a".repeat(4000);
    let (tx, rx) = std::sync::mpsc::channel();
    // a detached thread: if the matcher never returns, the test fails after the timeout instead of waiting for it
    std::thread::spawn(move || {
        let _ = tx.send(like_matches(&text, "%a%a%a%a%a%a%a%a%a%a%a%a%c"));
    });
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(matched) => assert!(!matched, "no 'c' in the text, so no match"),
        Err(_) => panic!("still matching after 10 seconds: the matcher backtracks exponentially (it tries each % against each position)"),
    }
}

/// A model that is obviously right and exponentially slow: try every split.
fn model_like(t: &[char], p: &[char]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some('%') => (0..=t.len()).any(|i| model_like(&t[i..], &p[1..])),
        Some('_') => !t.is_empty() && model_like(&t[1..], &p[1..]),
        Some('\\') if p.len() > 1 => !t.is_empty() && t[0] == p[1] && model_like(&t[1..], &p[2..]),
        Some(c) => !t.is_empty() && t[0] == *c && model_like(&t[1..], &p[1..]),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: the matcher agrees with the try-every-split model on a small alphabet.
    #[test]
    fn s3i_04_property_matches_the_obvious_model(text in "[ab]{0,7}", pattern in "[ab%_]{0,6}") {
        let (t, p): (Vec<char>, Vec<char>) = (text.chars().collect(), pattern.chars().collect());
        prop_assert_eq!(like_matches(&text, &pattern), model_like(&t, &p), "{:?} LIKE {:?}", text, pattern);
    }
}

// ---- 3i-05 · OFFSET and DISTINCT aggregates -----------------------------------------------------------------------------------

fn db_with_numbers() -> BusTubInstance {
    let db = db();
    rows(&db, "create table n(g int, x int)");
    rows(&db, "insert into n values (1, 5), (1, 5), (1, 7), (2, 5), (2, null), (2, null), (3, 9)");
    db
}

#[test]
fn s3i_05_offset_skips_rows_before_limit_counts() {
    let db = db_with_numbers();
    assert_eq!(rows(&db, "select x from n where x is not null order by x limit 2 offset 2"), ["5", "7"], "LIMIT 2 OFFSET 2 of 5 5 5 7 9");
    assert_eq!(
        rows(&db, "select x from n where x is not null order by x desc limit 1 offset 1"),
        ["7"],
        "descending order, one row after skipping the first"
    );
}

#[test]
fn s3i_05_offset_alone_and_offset_past_the_end() {
    let db = db_with_numbers();
    assert_eq!(rows(&db, "select x from n where x is not null order by x offset 3"), ["7", "9"], "OFFSET without LIMIT returns the rest");
    assert_eq!(rows(&db, "select x from n offset 100"), Vec::<String>::new(), "an offset past the end returns nothing");
    assert_eq!(
        rows(&db, "select x from n where x is not null order by x limit 0 offset 1"),
        Vec::<String>::new(),
        "LIMIT 0 returns nothing"
    );
    assert_eq!(
        rows(&db, "select x from n where x is not null order by x offset 0 limit 2"),
        ["5", "5"],
        "OFFSET 0 changes nothing, in either clause order"
    );
}

#[test]
fn s3i_05_offset_works_across_many_batches() {
    let db = db();
    rows(&db, "create table big(i int)");
    for chunk in 0..10 {
        let values: Vec<String> = (0..25).map(|i| format!("({})", chunk * 25 + i)).collect();
        rows(&db, &format!("insert into big values {}", values.join(", ")));
    }
    let got = rows(&db, "select i from big order by i limit 7 offset 63");
    assert_eq!(
        got,
        (63..70).map(|i| i.to_string()).collect::<Vec<_>>(),
        "an offset that is not a multiple of the batch size lands in the middle of a batch"
    );
    assert_eq!(
        rows(&db, "select count(*) from (select i from big offset 249) as r"),
        ["1"],
        "the last row survives an offset of count - 1"
    );
}

#[test]
fn s3i_05_count_and_sum_of_distinct_values_ignore_duplicates_and_nulls() {
    let db = db_with_numbers();
    assert_eq!(rows(&db, "select count(distinct x) from n"), ["3"], "5, 7 and 9: duplicates and NULLs do not count");
    assert_eq!(rows(&db, "select sum(distinct x) from n"), ["21"], "5 + 7 + 9, each once");
    assert_eq!(rows(&db, "select min(distinct x), max(distinct x) from n"), ["5 9"], "min and max are the same with or without DISTINCT");
}

#[test]
fn s3i_05_distinct_aggregates_work_per_group_and_in_having() {
    let db = db_with_numbers();
    let mut per_group = rows(&db, "select g, count(distinct x) from n group by g");
    per_group.sort();
    assert_eq!(per_group, ["1 2", "2 1", "3 1"], "per group (the order of groups is not defined)");
    assert_eq!(
        rows(&db, "select g, sum(distinct x) from n group by g having count(distinct x) > 1"),
        ["1 12"],
        "a DISTINCT aggregate in HAVING"
    );
}

#[test]
fn s3i_05_distinct_on_an_empty_input_and_over_expressions() {
    let db = db_with_numbers();
    assert_eq!(
        rows(&db, "select count(distinct x) from n where g = 99"),
        rows(&db, "select count(x) from n where g = 99"),
        "no rows: the same answer as the plain aggregate gives"
    );
    assert_eq!(rows(&db, "select count(distinct x % 2) from n"), ["1"], "DISTINCT over an expression: 5, 7, 9 are all odd");
    assert_eq!(rows(&db, "select count(distinct x) + 1 from n"), ["4"], "an arithmetic expression around a DISTINCT aggregate");
}

#[test]
fn s3i_05_distinct_mixed_with_plain_aggregates_is_right_or_refused_never_wrong() {
    let db = db_with_numbers();
    match run(&db, "select count(distinct x), count(*) from n") {
        Ok(r) => assert_eq!(r, ["3 7"], "if mixing is supported it must be right"),
        Err(e) => assert_eq!(e.kind, ExceptionType::NotImplemented, "if not, say so clearly"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: `ORDER BY x LIMIT l OFFSET o` is a slice of the sorted values; COUNT(DISTINCT x) and SUM(DISTINCT x) are those of the set.
    #[test]
    fn s3i_05_property_offset_is_a_slice_and_distinct_is_a_set(xs in proptest::collection::vec(0i32..8, 0..30), l in 0usize..12, o in 0usize..35) {
        let db = db();
        rows(&db, "create table p(x int)");
        if !xs.is_empty() {
            rows(&db, &format!("insert into p values {}", xs.iter().map(|x| format!("({x})")).collect::<Vec<_>>().join(", ")));
        }
        let mut sorted = xs.clone();
        sorted.sort();
        let want: Vec<String> = sorted.iter().skip(o).take(l).map(|x| x.to_string()).collect();
        prop_assert_eq!(rows(&db, &format!("select x from p order by x limit {l} offset {o}")), want);
        let set: std::collections::BTreeSet<i32> = xs.iter().copied().collect();
        if !xs.is_empty() {
            prop_assert_eq!(rows(&db, "select count(distinct x), sum(distinct x) from p"), vec![format!("{} {}", set.len(), set.iter().sum::<i32>())]);
        }
    }
}

// ---- 3i-06 · EXPLAIN ANALYZE --------------------------------------------------------------------------------------------------

/// One line of an analysis: the operator's text and what was measured.
#[derive(Debug)]
struct Node {
    depth: usize,
    op: String,
    rows: usize,
    batches: usize,
    loops: usize,
    millis: f64,
}

/// Runs `EXPLAIN ANALYZE sql` and parses the tree under `=== ANALYZE ===`.
fn analyze(db: &BusTubInstance, sql: &str) -> Vec<Node> {
    let out = rows(db, &format!("explain analyze {sql}"));
    let start = out.iter().position(|l| l.starts_with("=== ANALYZE ===")).unwrap_or_else(|| panic!("no ANALYZE section in {out:?}"));
    let mut nodes = vec![];
    for line in &out[start + 1..] {
        if line.trim().is_empty() || line.starts_with("===") {
            continue;
        }
        let depth = (line.len() - line.trim_start().len()) / 2;
        let open = line.rfind(" (rows=").unwrap_or_else(|| panic!("no (rows=...) on {line:?}"));
        let stats = line[open + 2..].trim_end_matches(')');
        let num = |key: &str| -> f64 {
            let at = stats.find(&format!("{key}=")).unwrap_or_else(|| panic!("no {key}= in {line:?}")) + key.len() + 1;
            stats[at..].split([',', ' ', 'm']).next().unwrap().parse().unwrap_or_else(|_| panic!("{key} is not a number in {line:?}"))
        };
        nodes.push(Node {
            depth,
            op: line[..open].trim().to_string(),
            rows: num("rows") as usize,
            batches: num("batches") as usize,
            loops: num("loops") as usize,
            millis: num("time"),
        });
    }
    nodes
}

fn find<'a>(nodes: &'a [Node], prefix: &str) -> &'a Node {
    nodes.iter().find(|n| n.op.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix} node in {nodes:?}"))
}

fn db_with_scores() -> BusTubInstance {
    let db = db();
    rows(&db, "create table s(a int, b int)");
    let values: Vec<String> = (1..=100).map(|i| format!("({i}, {})", i % 7)).collect();
    rows(&db, &format!("insert into s values {}", values.join(", ")));
    db
}

#[test]
fn s3i_06_every_operator_reports_the_rows_it_produced() {
    let db = db_with_scores();
    let nodes = analyze(&db, "select a from s where b = 3");
    assert_eq!(find(&nodes, "Projection").rows, 14, "100 rows, b = i mod 7: fourteen with b = 3");
    assert_eq!(find(&nodes, "SeqScan").rows, 14, "the filter was merged into the scan, which therefore produces only 14");
    assert_eq!(nodes[0].depth, 0, "the root comes first, at depth 0");
    assert_eq!(nodes[1].depth, 1, "its child is one level in");
}

#[test]
fn s3i_06_a_limit_stops_pulling_from_its_child() {
    let db = db_with_scores();
    let nodes = analyze(&db, "select * from s limit 5");
    assert_eq!(find(&nodes, "Limit").rows, 5, "five rows out");
    assert!(find(&nodes, "SeqScan").rows < 100, "the scan was not read to the end: {}", find(&nodes, "SeqScan").rows);
    assert!(find(&nodes, "SeqScan").rows >= 5, "but it produced at least what the limit needed");
}

#[test]
fn s3i_06_batches_count_the_calls_that_produced_rows() {
    let db = db_with_scores();
    let nodes = analyze(&db, "select a from s");
    let scan = find(&nodes, "SeqScan");
    assert_eq!(scan.rows, 100, "the whole table");
    assert!(scan.batches >= 2 && scan.batches <= 100, "100 rows come in batches of at most 20: {}", scan.batches);
    assert!(scan.rows <= scan.batches * 20, "a batch has at most 20 rows");
    rows(&db, "create table empty_t(a int)");
    let empty = analyze(&db, "select a from empty_t");
    let scan = find(&empty, "SeqScan");
    assert_eq!((scan.rows, scan.batches, scan.loops), (0, 0, 1), "the one call that found nothing is not a batch");
}

#[test]
fn s3i_06_the_inner_side_of_a_nested_loop_join_is_started_once_per_outer_row() {
    let db = db();
    rows(&db, "create table l(x int)");
    rows(&db, "create table r(y int)");
    rows(&db, "insert into l values (1), (2), (3), (4)");
    rows(&db, "insert into r values (1), (2), (3), (4), (5)");
    let nodes = analyze(&db, "select x, y from l, r where x < y");
    let scans: Vec<&Node> = nodes.iter().filter(|n| n.op.starts_with("SeqScan")).collect();
    assert_eq!(scans.len(), 2, "two scans: {nodes:?}");
    let (outer, inner) = (scans[0], scans[1]);
    assert_eq!(outer.loops, 1, "the outer side is started once");
    assert!(
        inner.loops >= outer.rows,
        "the inner side is started again for every outer row: {} loops for {} rows",
        inner.loops,
        outer.rows
    );
    assert_eq!(inner.rows, 5 * 4, "and read in full each time: 5 rows, 4 times");
}

#[test]
fn s3i_06_time_is_inclusive_and_never_negative() {
    let db = db_with_scores();
    let nodes = analyze(&db, "select a, count(*) from s group by a");
    assert!(nodes.iter().all(|n| n.millis >= 0.0), "no negative times: {nodes:?}");
    // a parent's time includes its children's: compare each node with the first node below it
    for i in 0..nodes.len() - 1 {
        if nodes[i + 1].depth == nodes[i].depth + 1 {
            assert!(
                nodes[i].millis + 0.001 >= nodes[i + 1].millis,
                "a node's time includes its child's: {:?} then {:?}",
                nodes[i],
                nodes[i + 1]
            );
        }
    }
}

#[test]
fn s3i_06_analysing_a_query_does_not_change_its_result_or_the_data() {
    let db = db_with_scores();
    let before = rows(&db, "select count(*), sum(a) from s");
    let first = analyze(&db, "select a from s where a > 90");
    let second = analyze(&db, "select a from s where a > 90");
    assert_eq!(
        first.iter().map(|n| n.rows).collect::<Vec<_>>(),
        second.iter().map(|n| n.rows).collect::<Vec<_>>(),
        "the same counts every time"
    );
    assert_eq!(rows(&db, "select count(*), sum(a) from s"), before, "the table is untouched");
    assert_eq!(
        error_kind(&db, "explain analyze delete from s where a = 1"),
        ExceptionType::NotImplemented,
        "a statement that changes data is refused, not executed"
    );
    assert_eq!(rows(&db, "select count(*) from s"), ["100"], "and nothing was deleted");
}

// ---- 3i-07 · prepared statements ----------------------------------------------------------------------------------------------

fn exec(db: &BusTubInstance, p: &bustub::common::prepared::PreparedStatement, params: &[Value]) -> Result<Vec<String>, Exception> {
    let mut out = String::new();
    db.execute_prepared(p, params, &mut SimpleStreamWriter::new(&mut out, true, " "))?;
    Ok(out.lines().map(|l| l.trim_end().to_string()).collect())
}

#[test]
fn s3i_07_placeholders_are_counted_in_order() {
    let db = db_with_t();
    assert_eq!(db.prepare("select a from t").unwrap().param_count(), 0, "no placeholders");
    assert_eq!(db.prepare("select a from t where a = ? and b > ?").unwrap().param_count(), 2, "two placeholders");
    assert_eq!(
        db.prepare("select a from t where a between ? and ? limit ? offset ?").unwrap().param_count(),
        4,
        "in BETWEEN, LIMIT and OFFSET"
    );
    assert_eq!(db.prepare("insert into t values (?, ?), (?, ?)").unwrap().param_count(), 4, "in VALUES rows");
}

#[test]
fn s3i_07_values_take_the_place_of_the_placeholders_left_to_right() {
    let db = db_with_t();
    let p = db.prepare("select a, b from t where a >= ? and b < ?").unwrap();
    assert_eq!(exec(&db, &p, &[Value::integer(2), Value::integer(25)]).unwrap(), ["2 20", "4 0"], "a >= 2 and b < 25");
    assert_eq!(
        exec(&db, &p, &[Value::integer(5), Value::integer(100)]).unwrap(),
        Vec::<String>::new(),
        "the same statement with other values: b is NULL for a = 5"
    );
    assert_eq!(exec(&db, &p, &[Value::integer(4), Value::integer(1)]).unwrap(), ["4 0"], "and again");
    let l = db.prepare("select a from t where a is not null order by a limit ? offset ?").unwrap();
    assert_eq!(exec(&db, &l, &[Value::integer(2), Value::integer(1)]).unwrap(), ["2", "4"], "LIMIT and OFFSET are placeholders too");
}

#[test]
fn s3i_07_a_string_value_stays_a_string_whatever_it_contains() {
    let db = db();
    rows(&db, "create table u(name varchar(60))");
    let ins = db.prepare("insert into u values (?)").unwrap();
    let nasty = "x'); drop table u; --";
    exec(&db, &ins, &[Value::varchar(nasty)]).unwrap();
    exec(&db, &ins, &[Value::varchar("plain")]).unwrap();
    assert_eq!(rows(&db, "select name from u"), [nasty, "plain"], "the text went in as data");
    let find = db.prepare("select name from u where name = ?").unwrap();
    assert_eq!(exec(&db, &find, &[Value::varchar(nasty)]).unwrap(), [nasty], "and is found by comparing, not by parsing");
    assert_eq!(
        exec(&db, &find, &[Value::varchar("' or '1'='1")]).unwrap(),
        Vec::<String>::new(),
        "a quote that tries to end the literal matches nothing"
    );
}

#[test]
fn s3i_07_the_number_of_values_must_match_the_number_of_placeholders() {
    let db = db_with_t();
    let p = db.prepare("select a from t where a = ? or a = ?").unwrap();
    assert_eq!(exec(&db, &p, &[Value::integer(1)]).unwrap_err().kind, ExceptionType::Invalid, "too few values");
    assert_eq!(
        exec(&db, &p, &[Value::integer(1), Value::integer(2), Value::integer(3)]).unwrap_err().kind,
        ExceptionType::Invalid,
        "too many values"
    );
    assert_eq!(
        exec(&db, &p, &[Value::integer(1), Value::integer(2)]).unwrap(),
        ["1", "2"],
        "and the right number works afterwards: nothing was left half-bound"
    );
}

#[test]
fn s3i_07_a_null_decimal_and_boolean_value_keep_their_meaning() {
    let db = db_with_t();
    let null = db.prepare("select count(*) from t where a = ?").unwrap();
    assert_eq!(exec(&db, &null, &[Value::null(TypeId::Integer)]).unwrap(), ["0"], "= NULL is never true");
    let isnull = db.prepare("select ? is null, ? + 1, ? and true").unwrap();
    assert_eq!(
        exec(&db, &isnull, &[Value::null(TypeId::Integer), Value::decimal(1.5), Value::boolean(false)]).unwrap(),
        ["true 2.500000 false"],
        "a NULL, a decimal and a boolean"
    );
}

#[test]
fn s3i_07_a_prepared_statement_can_be_executed_many_times_and_changes_data() {
    let db = db();
    rows(&db, "create table k(a int, b int)");
    let ins = db.prepare("insert into k values (?, ?)").unwrap();
    for i in 0..30 {
        exec(&db, &ins, &[Value::integer(i), Value::integer(i * i)]).unwrap();
    }
    let sel = db.prepare("select b from k where a = ?").unwrap();
    for i in [0, 7, 29] {
        assert_eq!(exec(&db, &sel, &[Value::integer(i)]).unwrap(), [(i * i).to_string()], "row {i}");
    }
    let upd = db.prepare("update k set b = ? where a = ?").unwrap();
    exec(&db, &upd, &[Value::integer(-1), Value::integer(7)]).unwrap();
    assert_eq!(exec(&db, &sel, &[Value::integer(7)]).unwrap(), ["-1"], "an UPDATE with placeholders in SET and WHERE");
    let del = db.prepare("delete from k where a >= ?").unwrap();
    exec(&db, &del, &[Value::integer(10)]).unwrap();
    assert_eq!(rows(&db, "select count(*) from k"), ["10"], "a DELETE with a placeholder");
}

#[test]
fn s3i_07_placeholders_work_inside_the_new_syntax() {
    let db = db_with_words();
    let p = db.prepare("select name from w where name like ? and name in (?, ?) and case when ? > 0 then true else false end").unwrap();
    assert_eq!(p.param_count(), 4, "four placeholders");
    let got = exec(&db, &p, &[Value::varchar("a%"), Value::varchar("apple"), Value::varchar("a_b%"), Value::integer(1)]).unwrap();
    assert_eq!(got, ["apple", "a_b%"], "LIKE, IN and CASE with placeholders");
}

#[test]
fn s3i_07_a_placeholder_outside_a_prepared_statement_is_an_error() {
    let db = db_with_t();
    assert!(run(&db, "select a from t where a = ?").is_err(), "no value was given: running it as plain SQL must fail, not guess");
}

// ---- 3i-08 · boss: the whole surface at once ----------------------------------------------------------------------------------

#[path = "slt/mod.rs"]
mod slt;

#[test]
fn s3i_08_the_sql_surface_script() {
    slt::run_slt("sql_surface.slt", 128);
}

/// A tiny typed expression language over `t(a, b)`, printed as SQL and evaluated in Rust with three-valued logic.
#[derive(Clone, Debug)]
enum I {
    A,
    B,
    Const(i64),
    Add(Box<I>, Box<I>),
    Sub(Box<I>, Box<I>),
    Mul(Box<I>, Box<I>),
    Neg(Box<I>),
    DivNullif(Box<I>, Box<I>),
    Coalesce(Box<I>, Box<I>),
    Nullif(Box<I>, Box<I>),
    Case(Box<Bo>, Box<I>, Box<I>),
}

#[derive(Clone, Debug)]
enum Bo {
    Cmp(&'static str, Box<I>, Box<I>),
    And(Box<Bo>, Box<Bo>),
    Or(Box<Bo>, Box<Bo>),
    Not(Box<Bo>),
    IsNull(Box<I>),
    Between(Box<I>, i64, i64),
    In(Box<I>, Vec<Option<i64>>),
}

fn sql_i(e: &I) -> String {
    match e {
        I::A => "a".into(),
        I::B => "b".into(),
        I::Const(c) => format!("({c})"),
        I::Add(x, y) => format!("({} + {})", sql_i(x), sql_i(y)),
        I::Sub(x, y) => format!("({} - {})", sql_i(x), sql_i(y)),
        I::Mul(x, y) => format!("({} * {})", sql_i(x), sql_i(y)),
        I::Neg(x) => format!("(-{})", sql_i(x)),
        I::DivNullif(x, y) => format!("({} / nullif({}, 0))", sql_i(x), sql_i(y)),
        I::Coalesce(x, y) => format!("coalesce({}, {})", sql_i(x), sql_i(y)),
        I::Nullif(x, y) => format!("nullif({}, {})", sql_i(x), sql_i(y)),
        I::Case(c, x, y) => format!("(case when {} then {} else {} end)", sql_b(c), sql_i(x), sql_i(y)),
    }
}

fn sql_b(e: &Bo) -> String {
    match e {
        Bo::Cmp(op, x, y) => format!("({} {op} {})", sql_i(x), sql_i(y)),
        Bo::And(x, y) => format!("({} and {})", sql_b(x), sql_b(y)),
        Bo::Or(x, y) => format!("({} or {})", sql_b(x), sql_b(y)),
        Bo::Not(x) => format!("(not {})", sql_b(x)),
        Bo::IsNull(x) => format!("({} is null)", sql_i(x)),
        Bo::Between(x, lo, hi) => format!("({} between ({lo}) and ({hi}))", sql_i(x)),
        Bo::In(x, list) => {
            let items: Vec<String> = list.iter().map(|v| v.map_or("null".into(), |v| format!("({v})"))).collect();
            format!("({} in ({}))", sql_i(x), items.join(", "))
        }
    }
}

fn ev_i(e: &I, a: Option<i64>, b: Option<i64>) -> Option<i64> {
    let lift = |x: &I, y: &I, f: fn(i64, i64) -> i64| match (ev_i(x, a, b), ev_i(y, a, b)) {
        (Some(x), Some(y)) => Some(f(x, y)),
        _ => None,
    };
    match e {
        I::A => a,
        I::B => b,
        I::Const(c) => Some(*c),
        I::Add(x, y) => lift(x, y, |x, y| x + y),
        I::Sub(x, y) => lift(x, y, |x, y| x - y),
        I::Mul(x, y) => lift(x, y, |x, y| x * y),
        I::Neg(x) => ev_i(x, a, b).map(|v| -v),
        I::DivNullif(x, y) => match (ev_i(x, a, b), ev_i(y, a, b)) {
            (Some(x), Some(y)) if y != 0 => Some(x / y),
            _ => None,
        },
        I::Coalesce(x, y) => ev_i(x, a, b).or(ev_i(y, a, b)),
        I::Nullif(x, y) => {
            let v = ev_i(x, a, b);
            if v.is_some() && v == ev_i(y, a, b) {
                None
            } else {
                v
            }
        }
        I::Case(c, x, y) => {
            if ev_b(c, a, b) == Some(true) {
                ev_i(x, a, b)
            } else {
                ev_i(y, a, b)
            }
        }
    }
}

fn ev_b(e: &Bo, a: Option<i64>, b: Option<i64>) -> Option<bool> {
    match e {
        Bo::Cmp(op, x, y) => match (ev_i(x, a, b), ev_i(y, a, b)) {
            (Some(x), Some(y)) => Some(match *op {
                "=" => x == y,
                "<>" => x != y,
                "<" => x < y,
                _ => x > y,
            }),
            _ => None,
        },
        Bo::And(x, y) => match (ev_b(x, a, b), ev_b(y, a, b)) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        Bo::Or(x, y) => match (ev_b(x, a, b), ev_b(y, a, b)) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        Bo::Not(x) => ev_b(x, a, b).map(|v| !v),
        Bo::IsNull(x) => Some(ev_i(x, a, b).is_none()),
        Bo::Between(x, lo, hi) => ev_i(x, a, b).map(|v| *lo <= v && v <= *hi),
        Bo::In(x, list) => {
            let Some(v) = ev_i(x, a, b) else { return None };
            if list.contains(&Some(v)) {
                Some(true)
            } else if list.contains(&None) {
                None
            } else {
                Some(false)
            }
        }
    }
}

fn arb_i() -> BoxedStrategy<I> {
    let leaf = prop_oneof![Just(I::A), Just(I::B), (-4i64..5).prop_map(I::Const)];
    leaf.prop_recursive(3, 14, 2, |inner| {
        let b = arb_b_over(inner.clone());
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(x, y)| I::Add(x.into(), y.into())),
            (inner.clone(), inner.clone()).prop_map(|(x, y)| I::Sub(x.into(), y.into())),
            (inner.clone(), inner.clone()).prop_map(|(x, y)| I::Mul(x.into(), y.into())),
            inner.clone().prop_map(|x| I::Neg(x.into())),
            (inner.clone(), inner.clone()).prop_map(|(x, y)| I::DivNullif(x.into(), y.into())),
            (inner.clone(), inner.clone()).prop_map(|(x, y)| I::Coalesce(x.into(), y.into())),
            (inner.clone(), inner.clone()).prop_map(|(x, y)| I::Nullif(x.into(), y.into())),
            (b, inner.clone(), inner).prop_map(|(c, x, y)| I::Case(c.into(), x.into(), y.into())),
        ]
    })
    .boxed()
}

fn arb_b_over(i: BoxedStrategy<I>) -> BoxedStrategy<Bo> {
    let cmp = (prop_oneof![Just("="), Just("<>"), Just("<"), Just(">")], i.clone(), i.clone())
        .prop_map(|(op, x, y)| Bo::Cmp(op, x.into(), y.into()));
    let isnull = i.clone().prop_map(|x| Bo::IsNull(x.into()));
    let between = (i.clone(), -4i64..5, -4i64..5).prop_map(|(x, lo, hi)| Bo::Between(x.into(), lo, hi));
    let in_list = (i, proptest::collection::vec(proptest::option::of(-4i64..5), 1..4)).prop_map(|(x, l)| Bo::In(x.into(), l));
    prop_oneof![cmp, isnull, between, in_list].boxed()
}

fn arb_b() -> BoxedStrategy<Bo> {
    let leaf = arb_b_over(arb_i());
    leaf.prop_recursive(2, 8, 2, |inner| {
        prop_oneof![
            (inner.clone(), inner.clone()).prop_map(|(x, y)| Bo::And(x.into(), y.into())),
            (inner.clone(), inner.clone()).prop_map(|(x, y)| Bo::Or(x.into(), y.into())),
            inner.prop_map(|x| Bo::Not(x.into())),
        ]
    })
    .boxed()
}

const MODEL_ROWS: [(Option<i64>, Option<i64>); 9] = [
    (Some(0), Some(0)),
    (Some(1), Some(-2)),
    (Some(-3), Some(3)),
    (Some(4), None),
    (None, Some(2)),
    (None, None),
    (Some(2), Some(2)),
    (Some(-1), Some(1)),
    (Some(3), Some(0)),
];

fn model_db() -> BusTubInstance {
    let db = db();
    rows(&db, "create table t(a int, b int)");
    let lit = |v: &Option<i64>| v.map_or("null".to_string(), |v| v.to_string());
    let values: Vec<String> = MODEL_ROWS.iter().map(|(a, b)| format!("({}, {})", lit(a), lit(b))).collect();
    rows(&db, &format!("insert into t values {}", values.join(", ")));
    db
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 60, failure_persistence: None, ..ProptestConfig::default() })]

    /// Boss property: a random integer expression selected from a table, row by row, agrees with the model (rows come back in insertion
    /// order); results that would not fit an INTEGER are not generated (values are small).
    #[test]
    fn s3i_08_property_random_integer_expressions_agree_with_the_model(e in arb_i()) {
        let db = model_db();
        let got = rows(&db, &format!("select {} from t", sql_i(&e)));
        let want: Vec<String> = MODEL_ROWS.iter().map(|(a, b)| ev_i(&e, *a, *b).map_or("integer_null".to_string(), |v| v.to_string())).collect();
        prop_assert_eq!(got, want, "select {} from t", sql_i(&e));
    }

    /// Boss property: a random condition selects exactly the rows for which the model says TRUE.
    #[test]
    fn s3i_08_property_random_conditions_select_what_the_model_selects(c in arb_b()) {
        let db = model_db();
        let got = rows(&db, &format!("select a, b from t where {}", sql_b(&c)));
        let cell = |v: &Option<i64>| v.map_or("integer_null".to_string(), |v| v.to_string());
        let want: Vec<String> = MODEL_ROWS.iter().filter(|(a, b)| ev_b(&c, *a, *b) == Some(true)).map(|(a, b)| format!("{} {}", cell(a), cell(b))).collect();
        prop_assert_eq!(got, want, "where {}", sql_b(&c));
    }
}

// @@ challenge 3i-c1 begin
mod ch_3i_c1 {

    use super::*;
    use bustub::execution::in_set::InSet;

    /// The definition: an OR chain of equalities in three-valued logic.
    fn chain(x: Option<i64>, list: &[Option<i64>]) -> Option<bool> {
        let cmp: Vec<Option<bool>> = list
            .iter()
            .map(|v| match (x, v) {
                (Some(a), Some(b)) => Some(a == *b),
                _ => None,
            })
            .collect();
        if cmp.contains(&Some(true)) {
            Some(true)
        } else if cmp.contains(&None) {
            None
        } else {
            Some(false)
        }
    }

    #[test]
    fn s3i_c1_hits_misses_and_a_null_on_the_left() {
        let s = InSet::new(&[Some(1), Some(2)]);
        assert_eq!((s.contains(Some(2)), s.contains(Some(3)), s.contains(None)), (Some(true), Some(false), None), "this check");
        assert_eq!((s.not_in(Some(2)), s.not_in(Some(3)), s.not_in(None)), (Some(false), Some(true), None), "this check");
    }

    #[test]
    fn s3i_c1_a_null_in_the_list_turns_misses_into_unknown() {
        let s = InSet::new(&[Some(1), None]);
        assert_eq!(s.contains(Some(1)), Some(true), "a hit is still a hit");
        assert_eq!(s.contains(Some(3)), None, "a miss might have been equal to the NULL");
        assert_eq!(s.not_in(Some(1)), Some(false), "this check");
        assert_eq!(s.not_in(Some(3)), None, "NOT IN with a NULL in the list is never TRUE");
    }

    #[test]
    fn s3i_c1_the_empty_list_and_duplicates() {
        let e = InSet::new(&[]);
        assert_eq!(
            (e.contains(Some(1)), e.contains(None), e.not_in(None)),
            (Some(false), Some(false), Some(true)),
            "nothing equals anything"
        );
        let d = InSet::new(&[Some(5), Some(5), None, None]);
        assert_eq!(d.len(), 1, "distinct non-NULL values");
        assert_eq!(d.contains(Some(5)), Some(true), "this check");
    }

    #[test]
    fn s3i_c1_only_a_null_in_the_list() {
        let n = InSet::new(&[None]);
        assert_eq!((n.contains(Some(1)), n.contains(None)), (None, None), "everything is unknown");
        assert_eq!(n.len(), 0, "no non-NULL values");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: agrees with the OR chain for every list and value; NOT IN is the negation.
        #[test]
        fn s3i_c1_property_agrees_with_the_or_chain(list in proptest::collection::vec(proptest::option::of(0i64..6), 0..8), x in proptest::option::of(0i64..8)) {
            let s = InSet::new(&list);
            prop_assert_eq!(s.contains(x), chain(x, &list));
            prop_assert_eq!(s.not_in(x), chain(x, &list).map(|b| !b));
        }
    }
}
// @@ challenge 3i-c1 end

// @@ challenge 3i-c2 begin
mod ch_3i_c2 {

    use super::*;
    use bustub::execution::not_in::not_in;

    #[test]
    fn s3i_c2_the_textbook_cases() {
        assert_eq!(not_in(Some(3), &[Some(1), Some(2)]), Some(true), "not in the list");
        assert_eq!(not_in(Some(1), &[Some(1), Some(2)]), Some(false), "in the list");
    }

    #[test]
    fn s3i_c2_a_null_in_the_list_makes_a_miss_unknown() {
        assert_eq!(not_in(Some(3), &[Some(1), None]), None, "3 might equal the NULL: unknown");
        assert_eq!(not_in(Some(1), &[Some(1), None]), Some(false), "a hit is decided whatever else is in the list");
    }

    #[test]
    fn s3i_c2_a_null_on_the_left_is_unknown_for_a_non_empty_list() {
        assert_eq!(not_in(None, &[Some(1)]), None, "NULL NOT IN (1) is NULL");
        assert_eq!(not_in(None, &[None]), None, "NULL NOT IN (NULL) is NULL");
    }

    #[test]
    fn s3i_c2_the_empty_list_has_nothing_to_equal() {
        assert_eq!(not_in(Some(1), &[]), Some(true), "1 NOT IN () is TRUE");
        assert_eq!(not_in(None, &[]), Some(true), "and so is NULL NOT IN ()");
    }

    #[test]
    fn s3i_c2_only_nulls_in_the_list() {
        assert_eq!(not_in(Some(7), &[None, None]), None, "every comparison is unknown");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the definition `NOT (x = v1 OR x = v2 ...)`, evaluated in three-valued logic.
        #[test]
        fn s3i_c2_property_agrees_with_not_of_the_or_chain(x in proptest::option::of(0i64..5), list in proptest::collection::vec(proptest::option::of(0i64..5), 0..6)) {
            let cmp: Vec<Option<bool>> = list.iter().map(|v| match (x, v) { (Some(a), Some(b)) => Some(a == *b), _ => None }).collect();
            let or = if cmp.contains(&Some(true)) { Some(true) } else if cmp.contains(&None) { None } else { Some(false) };
            prop_assert_eq!(not_in(x, &list), or.map(|b| !b));
        }
    }
}
// @@ challenge 3i-c2 end

// @@ challenge 3i-c3 begin
mod ch_3i_c3 {

    use super::*;
    use bustub::optimizer::case_fold::{fold_case, Case, Cond, Cond::*, Folded};

    fn case(branches: &[(Cond, u32)], otherwise: Option<u32>) -> Case {
        Case { branches: branches.to_vec(), otherwise }
    }

    /// What the CASE gives when the unknown conditions take the values in `env` (index = Unknown number).
    fn eval(c: &Case, env: &[Option<bool>]) -> Option<u32> {
        for &(cond, r) in &c.branches {
            let v = match cond {
                True => Some(true),
                False => Some(false),
                Null => None,
                Unknown(i) => env[i as usize],
            };
            if v == Some(true) {
                return Some(r);
            }
        }
        c.otherwise
    }

    fn eval_folded(f: &Folded, env: &[Option<bool>]) -> Option<u32> {
        match f {
            Folded::Result(r) => *r,
            Folded::Case(c) => eval(c, env),
        }
    }

    #[test]
    fn s3i_c3_branches_that_can_never_be_taken_are_removed() {
        let c = case(&[(False, 1), (Unknown(0), 2), (Null, 3), (Unknown(1), 4)], Some(9));
        assert_eq!(
            fold_case(&c),
            Folded::Case(case(&[(Unknown(0), 2), (Unknown(1), 4)], Some(9))),
            "FALSE and NULL conditions are never TRUE"
        );
    }

    #[test]
    fn s3i_c3_a_true_branch_ends_the_case_and_becomes_the_else() {
        let c = case(&[(Unknown(0), 1), (True, 2), (Unknown(1), 3)], Some(4));
        assert_eq!(fold_case(&c), Folded::Case(case(&[(Unknown(0), 1)], Some(2))), "what follows a TRUE branch is unreachable");
    }

    #[test]
    fn s3i_c3_a_case_with_nothing_left_is_decided() {
        assert_eq!(fold_case(&case(&[(False, 1)], None)), Folded::Result(None), "no branch and no ELSE: NULL");
        assert_eq!(fold_case(&case(&[(Null, 1), (False, 2)], Some(5))), Folded::Result(Some(5)), "the ELSE");
        assert_eq!(fold_case(&case(&[(True, 7), (Unknown(0), 8)], Some(5))), Folded::Result(Some(7)), "a leading TRUE");
    }

    #[test]
    fn s3i_c3_a_case_of_unknown_conditions_is_left_alone() {
        let c = case(&[(Unknown(0), 1), (Unknown(1), 2)], None);
        assert_eq!(fold_case(&c), Folded::Case(c.clone()), "nothing to fold");
    }

    #[test]
    fn s3i_c3_branch_order_is_kept() {
        let c = case(&[(Unknown(2), 1), (False, 9), (Unknown(0), 2), (Unknown(1), 3)], Some(0));
        match fold_case(&c) {
            Folded::Case(f) => assert_eq!(
                f.branches,
                vec![(Unknown(2), 1), (Unknown(0), 2), (Unknown(1), 3)],
                "first match wins: the order is the meaning"
            ),
            other => panic!("{other:?}"),
        }
    }

    fn arb_cond() -> impl Strategy<Value = Cond> {
        prop_oneof![Just(True), Just(False), Just(Null), (0u32..3).prop_map(Unknown)]
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 400, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the folded CASE gives the same result for every assignment of the unknowns; folding is idempotent and never adds branches.
        #[test]
        fn s3i_c3_property_folding_preserves_meaning(branches in proptest::collection::vec((arb_cond(), 0u32..5), 0..6), otherwise in proptest::option::of(5u32..9)) {
            let c = Case { branches, otherwise };
            let f = fold_case(&c);
            for bits in 0..27u32 {
                let env: Vec<Option<bool>> = (0..3).map(|i| match (bits / 3u32.pow(i)) % 3 { 0 => Some(true), 1 => Some(false), _ => None }).collect();
                prop_assert_eq!(eval_folded(&f, &env), eval(&c, &env), "env {:?}", env);
            }
            if let Folded::Case(inner) = &f {
                prop_assert!(inner.branches.len() <= c.branches.len());
                prop_assert_eq!(fold_case(inner), Folded::Case(inner.clone()), "idempotent");
            }
        }
    }
}
// @@ challenge 3i-c3 end

// @@ challenge 3i-c4 begin
mod ch_3i_c4 {

    use super::*;
    use bustub::execution::expressions::like_expression::like_matches;
    use bustub::optimizer::like_prefix::{prefix_range, successor, LikeRange};

    fn range(prefix: &str, upper: Option<&str>, recheck: bool) -> LikeRange {
        LikeRange::Range { prefix: prefix.to_string(), upper: upper.map(String::from), recheck }
    }

    #[test]
    fn s3i_c4_a_prefix_followed_by_percent_is_a_range_with_nothing_to_recheck() {
        assert_eq!(prefix_range("abc%"), range("abc", Some("abd"), false), "abc% is [abc, abd)");
        assert_eq!(prefix_range("abc%%"), range("abc", Some("abd"), false), "extra percents change nothing");
    }

    #[test]
    fn s3i_c4_anything_after_the_prefix_means_the_matches_are_rechecked() {
        assert_eq!(prefix_range("abc%d"), range("abc", Some("abd"), true), "a suffix");
        assert_eq!(prefix_range("ab_"), range("ab", Some("ac"), true), "an underscore");
        assert_eq!(prefix_range("ab_%"), range("ab", Some("ac"), true), "an underscore then a percent");
    }

    #[test]
    fn s3i_c4_a_leading_wildcard_has_no_prefix_and_no_wildcard_is_exact() {
        assert_eq!(prefix_range("%abc"), LikeRange::Everything, "this check");
        assert_eq!(prefix_range("_abc"), LikeRange::Everything, "this check");
        assert_eq!(prefix_range("%"), LikeRange::Everything, "this check");
        assert_eq!(prefix_range("abc"), LikeRange::Exact("abc".into()), "this check");
        assert_eq!(prefix_range(""), LikeRange::Exact(String::new()), "the empty pattern matches only the empty string");
    }

    #[test]
    fn s3i_c4_escapes_are_part_of_the_prefix() {
        assert_eq!(prefix_range("a\\%b%"), range("a%b", Some("a%c"), false), "an escaped percent is a letter");
        assert_eq!(prefix_range("100\\%"), LikeRange::Exact("100%".into()), "an escaped percent at the end: no wildcard");
    }

    #[test]
    fn s3i_c4_the_upper_bound_at_the_end_of_the_alphabet() {
        assert_eq!(successor("az"), Some("a{".into()), "z + 1");
        assert_eq!(
            successor(&format!("a{}", char::MAX)),
            Some("b".into()),
            "the largest character is dropped and the one before increased"
        );
        assert_eq!(successor(&char::MAX.to_string()), None, "nothing is above it");
        assert_eq!(successor("a\u{D7FF}"), Some("a\u{E000}".into()), "surrogates are not characters: skip them");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 400, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every string the pattern matches lies in [prefix, upper); an Exact pattern matches only its string.
        #[test]
        fn s3i_c4_property_matches_stay_inside_the_range(pattern in "[ab%_]{0,5}", text in "[abc]{0,6}") {
            match prefix_range(&pattern) {
                LikeRange::Everything => {}
                LikeRange::Exact(s) => prop_assert_eq!(like_matches(&text, &pattern), text == s),
                LikeRange::Range { prefix, upper, recheck } => {
                    if like_matches(&text, &pattern) {
                        prop_assert!(text.starts_with(&prefix), "{:?} matches {:?} but does not start with {:?}", text, pattern, prefix);
                        prop_assert!(text >= prefix);
                        if let Some(u) = &upper { prop_assert!(&text < u, "{:?} is not below {:?}", text, u); }
                    }
                    if !recheck && text.starts_with(&prefix) {
                        prop_assert!(like_matches(&text, &pattern), "no recheck needed, so every string with the prefix must match");
                    }
                }
            }
        }
    }
}
// @@ challenge 3i-c4 end

// @@ challenge 3i-c5 begin
mod ch_3i_c5 {

    use super::*;
    use bustub::execution::analyze_times::{exclusive_times, slowest};

    fn lines(l: &[&str]) -> String {
        l.iter().map(|x| format!("{x}\n")).collect()
    }

    fn sample() -> String {
        lines(&[
            "=== ANALYZE ===",
            "Limit { limit=5 } (rows=5, batches=1, loops=1, time=0.900ms)",
            "  ExternalMergeSort { order_bys=[(Default, Default, #0.0)] } (rows=5, batches=1, loops=1, time=0.800ms)",
            "    SeqScan { table=t } (rows=100, batches=5, loops=1, time=0.500ms)",
        ])
    }

    #[test]
    fn s3i_c5_each_line_gives_its_depth_text_and_inclusive_time() {
        let ops = exclusive_times(&sample());
        assert_eq!(ops.len(), 3, "one entry per operator, the header skipped");
        assert_eq!(ops.iter().map(|o| o.depth).collect::<Vec<_>>(), [0, 1, 2], "indentation is depth");
        assert_eq!(ops[0].op, "Limit { limit=5 }", "the text without the statistics");
        assert_eq!(ops.iter().map(|o| o.inclusive_ms).collect::<Vec<_>>(), [0.9, 0.8, 0.5], "this check");
    }

    #[test]
    fn s3i_c5_exclusive_time_is_what_is_left_after_the_children() {
        let ops = exclusive_times(&sample());
        let ex: Vec<f64> = ops.iter().map(|o| (o.exclusive_ms * 1000.0).round() / 1000.0).collect();
        assert_eq!(ex, [0.1, 0.3, 0.5], "0.9 - 0.8, 0.8 - 0.5, and a leaf keeps its own");
        assert_eq!(slowest(&sample()).as_deref(), Some("SeqScan { table=t }"), "the scan did the work: not the root");
    }

    #[test]
    fn s3i_c5_siblings_are_subtracted_from_their_common_parent_only() {
        let text = lines(&[
            "NestedLoopJoin { type=Inner } (rows=4, batches=1, loops=1, time=3.000ms)",
            "  SeqScan { table=l } (rows=2, batches=1, loops=1, time=0.500ms)",
            "  Filter { x } (rows=2, batches=1, loops=3, time=2.000ms)",
            "    SeqScan { table=r } (rows=6, batches=2, loops=3, time=1.500ms)",
        ]);
        let ops = exclusive_times(&text);
        let ex: Vec<f64> = ops.iter().map(|o| (o.exclusive_ms * 1000.0).round() / 1000.0).collect();
        assert_eq!(ex, [0.5, 0.5, 0.5, 1.5], "3.0 - 0.5 - 2.0; 0.5; 2.0 - 1.5; 1.5");
    }

    #[test]
    fn s3i_c5_a_node_that_never_ran_costs_nothing_and_negatives_are_clamped() {
        let text =
            lines(&["Projection { exprs=[#0.0] } (rows=0, batches=0, loops=1, time=0.100ms)", "  SeqScan { table=t } (never executed)"]);
        let ops = exclusive_times(&text);
        assert_eq!(ops[1].inclusive_ms, 0.0, "(never executed) is zero");
        assert!((ops[0].exclusive_ms - 0.1).abs() < 1e-9, "this check: expected `(ops[0].exclusive_ms - 0.1).abs() < 1e-9`");
        let jitter = "A (rows=1, batches=1, loops=1, time=1.000ms)\n  B (rows=1, batches=1, loops=1, time=1.200ms)\n";
        assert_eq!(exclusive_times(jitter)[0].exclusive_ms, 0.0, "a child that measured longer than its parent: never below zero");
    }

    #[test]
    fn s3i_c5_no_operators_no_slowest() {
        assert!(exclusive_times("").is_empty(), "this check: expected `exclusive_times(\"\").is_empty()`");
        assert_eq!(slowest("=== ANALYZE ===\n"), None, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 200, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: for a tree where every child is shorter than its parent, the exclusive times are non-negative and add up to the root's.
        #[test]
        fn s3i_c5_property_exclusive_times_add_up_to_the_root(shape in proptest::collection::vec((0usize..3, 1u32..50), 1..8)) {
            // build a valid tree text: the node's depth is at most one more than the previous node's
            let mut depth_prev = 0usize;
            let mut lines: Vec<(usize, u32)> = vec![];
            for (i, (d, self_cost)) in shape.iter().enumerate() {
                let depth = if i == 0 { 0 } else { (*d).min(depth_prev + 1) };
                let depth = if i > 0 && depth == 0 { 1 } else { depth };
                lines.push((depth, *self_cost));
                depth_prev = depth;
            }
            // inclusive = own + children's inclusive, computed bottom-up in tenths of a millisecond
            let n = lines.len();
            let mut inclusive = vec![0u32; n];
            for i in (0..n).rev() {
                let mut total = lines[i].1;
                let mut j = i + 1;
                while j < n && lines[j].0 > lines[i].0 {
                    if lines[j].0 == lines[i].0 + 1 { total += inclusive[j]; }
                    j += 1;
                }
                inclusive[i] = total;
            }
            let text: String = lines.iter().enumerate().map(|(i, (d, _))| format!("{}Op{} (rows=1, batches=1, loops=1, time={:.3}ms)\n", "  ".repeat(*d), i, inclusive[i] as f64 / 10.0)).collect();
            let ops = exclusive_times(&text);
            prop_assert_eq!(ops.len(), n);
            prop_assert!(ops.iter().all(|o| o.exclusive_ms >= 0.0));
            let roots_total: f64 = ops.iter().filter(|o| o.depth == 0).map(|o| o.inclusive_ms).sum();
            let exclusive_total: f64 = ops.iter().map(|o| o.exclusive_ms).sum();
            prop_assert!((roots_total - exclusive_total).abs() < 1e-6, "{} vs {}", roots_total, exclusive_total);
        }
    }
}
// @@ challenge 3i-c5 end

// @@ challenge 3i-c6 begin
mod ch_3i_c6 {

    use super::*;
    use std::any::Any;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bustub::catalog::column::Column;
    use bustub::execution::expressions::coalesce_expression::CoalesceExpression;

    /// A child that says how often it was evaluated, and can be made to fail.
    #[derive(Debug)]
    struct Probe {
        value: Value,
        calls: Arc<AtomicUsize>,
        fails: bool,
        ret: Column,
    }

    fn probe(value: Value, fails: bool) -> (ExprRef, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let ret =
            if value.type_id() == TypeId::Varchar { Column::new_varchar("<probe>", 64) } else { Column::new("<probe>", value.type_id()) };
        (Arc::new(Probe { value, calls: calls.clone(), fails, ret }), calls)
    }

    impl Expression for Probe {
        fn evaluate(&self, _: &Tuple, _: &Schema) -> bustub::common::exception::Result<Value> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fails {
                return Err(Exception::new(ExceptionType::Execution, "evaluated a child that should not have been"));
            }
            Ok(self.value.clone())
        }
        fn evaluate_join(&self, l: &Tuple, ls: &Schema, _: &Tuple, _: &Schema) -> bustub::common::exception::Result<Value> {
            self.evaluate(l, ls)
        }
        fn children(&self) -> &[ExprRef] {
            &[]
        }
        fn return_type(&self) -> &Column {
            &self.ret
        }
        fn to_string(&self) -> String {
            "probe".into()
        }
        fn clone_with_children(&self, _: Vec<ExprRef>) -> ExprRef {
            unreachable!("a probe has no children")
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn eval_c(c: &CoalesceExpression) -> bustub::common::exception::Result<Value> {
        c.evaluate(&Tuple::new(&[], &Schema::new(vec![])), &Schema::new(vec![]))
    }

    fn null() -> Value {
        Value::null(TypeId::Integer)
    }

    #[test]
    fn s3i_c6_the_first_non_null_wins_and_later_children_are_not_evaluated() {
        let (a, ca) = probe(null(), false);
        let (b, cb) = probe(Value::integer(7), false);
        let (c, cc) = probe(Value::integer(9), true);
        let e = CoalesceExpression::new(vec![a, b, c]).unwrap();
        assert_eq!(eval_c(&e).unwrap(), Value::integer(7), "the second child");
        assert_eq!(
            (ca.load(Ordering::SeqCst), cb.load(Ordering::SeqCst), cc.load(Ordering::SeqCst)),
            (1, 1, 0),
            "each earlier child once, the third never"
        );
    }

    #[test]
    fn s3i_c6_a_set_first_child_is_the_only_one_evaluated() {
        let (a, ca) = probe(Value::integer(3), false);
        let (b, cb) = probe(Value::integer(1), true);
        let e = CoalesceExpression::new(vec![a, b]).unwrap();
        assert_eq!(eval_c(&e).unwrap(), Value::integer(3), "a is set");
        assert_eq!((ca.load(Ordering::SeqCst), cb.load(Ordering::SeqCst)), (1, 0), "b would have failed: it is never reached");
    }

    #[test]
    fn s3i_c6_all_null_gives_null_of_the_shared_type_and_one_argument_is_itself() {
        let (a, _) = probe(null(), false);
        let (b, _) = probe(null(), false);
        let e = CoalesceExpression::new(vec![a, b]).unwrap();
        assert!(eval_c(&e).unwrap().is_null(), "every argument NULL");
        assert_eq!(e.return_type().type_id(), TypeId::Integer, "this check");
        let (only, calls) = probe(Value::integer(5), false);
        assert_eq!(eval_c(&CoalesceExpression::new(vec![only]).unwrap()).unwrap(), Value::integer(5), "coalesce(a) is a");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "evaluated once");
    }

    #[test]
    fn s3i_c6_types_must_agree_and_a_bare_null_adopts_them() {
        let (i, _) = probe(Value::integer(1), false);
        let (s, _) = probe(Value::varchar("x"), false);
        assert!(CoalesceExpression::new(vec![i, s]).is_err(), "an integer and a string do not coalesce");
        assert!(CoalesceExpression::new(vec![]).is_err(), "at least one argument");
        let (s2, _) = probe(Value::varchar("y"), false);
        let nul: ExprRef = Arc::new(ConstantValueExpression::new(null()));
        let e = CoalesceExpression::new(vec![nul, s2]).unwrap();
        assert_eq!(e.return_type().type_id(), TypeId::Varchar, "a bare NULL takes the string type");
        assert_eq!(eval_c(&e).unwrap(), Value::varchar("y"), "and is skipped");
    }

    #[test]
    fn s3i_c6_an_error_in_a_reached_child_is_passed_on() {
        let (a, _) = probe(null(), false);
        let (b, _) = probe(Value::integer(1), true);
        let e = CoalesceExpression::new(vec![a, b]).unwrap();
        assert!(eval_c(&e).is_err(), "the failing child was needed: its error is the result");
    }
}
// @@ challenge 3i-c6 end
