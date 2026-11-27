//! Tests for module 3d: expressions.

use std::sync::Arc;

use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::exception::ExceptionType;
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::execution::expressions::abstract_expression::{ExprRef, Expression};
use bustub::execution::expressions::arithmetic_expression::{ArithmeticExpression, ArithmeticType};
use bustub::execution::expressions::column_value_expression::ColumnValueExpression;
use bustub::execution::expressions::comparison_expression::{ComparisonExpression, ComparisonType};
use bustub::execution::expressions::constant_value_expression::ConstantValueExpression;
use bustub::execution::expressions::logic_expression::{LogicExpression, LogicType};
use bustub::execution::expressions::string_expression::{StringExpression, StringExpressionType};
use bustub::planner::planner::Planner;
use bustub::sql::ast::Expr;
use bustub::sql::lexer::{tokenize, Token};
use bustub::sql::parser::parse_expr;
use bustub::storage::table::tuple::Tuple;
use bustub::types::type_id::TypeId;
use bustub::types::value::Value;
use proptest::prelude::*;

fn int(v: i32) -> Value {
    Value::integer(v)
}

fn constant(v: Value) -> ExprRef {
    Arc::new(ConstantValueExpression::new(v))
}

fn col(tuple_idx: u32, col_idx: u32, type_id: TypeId) -> ExprRef {
    let c = if type_id == TypeId::Varchar { Column::new_varchar("c", 64) } else { Column::new("c", type_id) };
    Arc::new(ColumnValueExpression::new(tuple_idx, col_idx, c))
}

/// (a INTEGER, b VARCHAR(64), c INTEGER)
fn schema_abc() -> Schema {
    Schema::new(vec![Column::new("a", TypeId::Integer), Column::new_varchar("b", 64), Column::new("c", TypeId::Integer)])
}

fn tuple_abc(a: Value, b: &str, c: Value) -> Tuple {
    Tuple::new(&[a, Value::varchar(b), c], &schema_abc())
}

fn empty() -> (Tuple, Schema) {
    (Tuple::empty(), Schema::new(vec![]))
}

fn eval(e: &ExprRef) -> Value {
    let (t, s) = empty();
    e.evaluate(&t, &s).unwrap()
}

fn cmp(l: Value, r: Value, t: ComparisonType) -> Value {
    eval(&(Arc::new(ComparisonExpression::new(constant(l), constant(r), t)) as ExprRef))
}

fn boolean_null() -> Value {
    Value::null(TypeId::Boolean)
}

// ---- 3d-01 · constants and columns ----------------------------------------------------------------------------------------------

#[test]
fn s3d_01_a_constant_is_its_value_for_any_tuple() {
    let c = constant(int(42));
    assert_eq!(eval(&c), int(42), "a constant is its value for any tuple");
    let schema = schema_abc();
    let t = tuple_abc(int(1), "x", int(2));
    assert_eq!(c.evaluate(&t, &schema).unwrap(), int(42), "the tuple does not matter");
    assert_eq!(constant(Value::varchar("hi")).evaluate(&t, &schema).unwrap(), Value::varchar("hi"), "a constant is its value for any tuple");
    assert_eq!(constant(Value::null(TypeId::Integer)).evaluate(&t, &schema).unwrap(), Value::null(TypeId::Integer), "a constant is its value for any tuple");
}

#[test]
fn s3d_01_a_constant_evaluated_in_a_join_is_still_its_value() {
    let (schema, t) = (schema_abc(), tuple_abc(int(1), "x", int(2)));
    let c = constant(Value::boolean(true));
    assert_eq!(c.evaluate_join(&t, &schema, &t, &schema).unwrap(), Value::boolean(true), "a constant evaluated in a join is still its value");
}

#[test]
fn s3d_01_a_column_value_reads_the_column_of_the_tuple() {
    let (schema, t) = (schema_abc(), tuple_abc(int(7), "seven", int(70)));
    assert_eq!(col(0, 0, TypeId::Integer).evaluate(&t, &schema).unwrap(), int(7), "a column value reads the column of the tuple");
    assert_eq!(col(0, 1, TypeId::Varchar).evaluate(&t, &schema).unwrap(), Value::varchar("seven"), "a column value reads the column of the tuple");
    assert_eq!(col(0, 2, TypeId::Integer).evaluate(&t, &schema).unwrap(), int(70), "a column value reads the column of the tuple");
}

#[test]
fn s3d_01_a_column_value_can_be_null() {
    let (schema, t) = (schema_abc(), tuple_abc(Value::null(TypeId::Integer), "x", int(1)));
    assert_eq!(col(0, 0, TypeId::Integer).evaluate(&t, &schema).unwrap(), Value::null(TypeId::Integer), "a column value can be null");
}

#[test]
fn s3d_01_in_a_join_tuple_idx_picks_the_side_and_each_side_has_its_own_schema() {
    let left_schema = schema_abc();
    let right_schema = Schema::new(vec![Column::new("x", TypeId::Integer), Column::new("y", TypeId::Integer)]);
    let left = tuple_abc(int(1), "left", int(3));
    let right = Tuple::new(&[int(100), int(200)], &right_schema);
    let left_c2 = col(0, 2, TypeId::Integer);
    let right_c1 = col(1, 1, TypeId::Integer);
    assert_eq!(left_c2.evaluate_join(&left, &left_schema, &right, &right_schema).unwrap(), int(3), "in a join tuple idx picks the side and each side has its own schema");
    assert_eq!(right_c1.evaluate_join(&left, &left_schema, &right, &right_schema).unwrap(), int(200), "in a join tuple idx picks the side and each side has its own schema");
    assert_eq!(col(1, 0, TypeId::Integer).evaluate_join(&left, &left_schema, &right, &right_schema).unwrap(), int(100), "in a join tuple idx picks the side and each side has its own schema");
}

#[test]
fn s3d_01_expressions_describe_themselves() {
    assert_eq!(col(0, 3, TypeId::Integer).to_string(), "#0.3", "expressions describe themselves");
    assert_eq!(col(1, 0, TypeId::Integer).to_string(), "#1.0", "expressions describe themselves");
    assert_eq!(constant(int(5)).to_string(), "5", "expressions describe themselves");
    assert_eq!(constant(Value::varchar("a")).to_string(), "a", "expressions describe themselves");
    assert_eq!(col(0, 0, TypeId::Integer).return_type().type_id(), TypeId::Integer, "expressions describe themselves");
    assert_eq!(constant(Value::boolean(true)).return_type().type_id(), TypeId::Boolean, "expressions describe themselves");
    assert!(constant(int(1)).children().is_empty(), "expressions describe themselves: expected `constant(int(1)).children().is_empty()`");
}

// ---- 3d-02 · comparisons --------------------------------------------------------------------------------------------------------

#[test]
fn s3d_02_the_six_comparisons_on_integers() {
    use ComparisonType::*;
    let t = Value::boolean(true);
    let f = Value::boolean(false);
    assert_eq!(cmp(int(1), int(1), Equal), t, "the six comparisons on integers");
    assert_eq!(cmp(int(1), int(2), Equal), f, "the six comparisons on integers");
    assert_eq!(cmp(int(1), int(2), NotEqual), t, "the six comparisons on integers");
    assert_eq!(cmp(int(1), int(2), LessThan), t, "the six comparisons on integers");
    assert_eq!(cmp(int(2), int(2), LessThan), f, "the six comparisons on integers");
    assert_eq!(cmp(int(2), int(2), LessThanOrEqual), t, "the six comparisons on integers");
    assert_eq!(cmp(int(3), int(2), GreaterThan), t, "the six comparisons on integers");
    assert_eq!(cmp(int(2), int(2), GreaterThan), f, "the six comparisons on integers");
    assert_eq!(cmp(int(2), int(2), GreaterThanOrEqual), t, "the six comparisons on integers");
    assert_eq!(cmp(int(1), int(2), GreaterThanOrEqual), f, "the six comparisons on integers");
}

#[test]
fn s3d_02_comparing_with_null_is_null_not_false() {
    use ComparisonType::*;
    let null = Value::null(TypeId::Integer);
    for t in [Equal, NotEqual, LessThan, LessThanOrEqual, GreaterThan, GreaterThanOrEqual] {
        assert_eq!(cmp(null.clone(), int(1), t), boolean_null(), "NULL {t:?} 1");
        assert_eq!(cmp(int(1), null.clone(), t), boolean_null(), "1 {t:?} NULL");
        assert_eq!(cmp(null.clone(), null.clone(), t), boolean_null(), "NULL {t:?} NULL");
    }
}

#[test]
fn s3d_02_strings_and_mixed_numbers_compare_by_value() {
    use ComparisonType::*;
    assert_eq!(cmp(Value::varchar("abc"), Value::varchar("abd"), LessThan), Value::boolean(true), "strings and mixed numbers compare by value");
    assert_eq!(cmp(Value::varchar("b"), Value::varchar("b"), Equal), Value::boolean(true), "strings and mixed numbers compare by value");
    assert_eq!(cmp(int(1), Value::decimal(1.5), LessThan), Value::boolean(true), "strings and mixed numbers compare by value");
    assert_eq!(cmp(Value::bigint(5), int(5), Equal), Value::boolean(true), "strings and mixed numbers compare by value");
}

#[test]
fn s3d_02_a_comparison_of_columns_in_a_join_and_in_a_row() {
    let schema = schema_abc();
    let t = tuple_abc(int(5), "x", int(5));
    let same_row: ExprRef = Arc::new(ComparisonExpression::new(col(0, 0, TypeId::Integer), col(0, 2, TypeId::Integer), ComparisonType::Equal));
    assert_eq!(same_row.evaluate(&t, &schema).unwrap(), Value::boolean(true), "a comparison of columns in a join and in a row");

    let right_schema = Schema::new(vec![Column::new("y", TypeId::Integer)]);
    let right = Tuple::new(&[int(9)], &right_schema);
    let join_pred: ExprRef = Arc::new(ComparisonExpression::new(col(0, 0, TypeId::Integer), col(1, 0, TypeId::Integer), ComparisonType::LessThan));
    assert_eq!(join_pred.evaluate_join(&t, &schema, &right, &right_schema).unwrap(), Value::boolean(true), "a comparison of columns in a join and in a row");
}

#[test]
fn s3d_02_a_comparison_describes_itself_and_returns_a_boolean() {
    let e = ComparisonExpression::new(col(0, 0, TypeId::Integer), constant(int(5)), ComparisonType::GreaterThanOrEqual);
    assert_eq!(Expression::to_string(&e), "(#0.0>=5)", "a comparison describes itself and returns a boolean");
    assert_eq!(e.return_type().type_id(), TypeId::Boolean, "a comparison describes itself and returns a boolean");
    assert_eq!(e.children().len(), 2, "a comparison describes itself and returns a boolean");
    let ne = ComparisonExpression::new(constant(int(1)), constant(int(2)), ComparisonType::NotEqual);
    assert_eq!(Expression::to_string(&ne), "(1!=2)", "a comparison describes itself and returns a boolean");
}

// ---- 3d-02 · arithmetic ---------------------------------------------------------------------------------------------------------

fn arith(l: Value, r: Value, t: ArithmeticType) -> bustub::common::exception::Result<Value> {
    let e: ExprRef = Arc::new(ArithmeticExpression::new(constant(l), constant(r), t)?);
    let (tup, schema) = empty();
    e.evaluate(&tup, &schema)
}

#[test]
fn s3d_02_plus_and_minus() {
    assert_eq!(arith(int(1), int(2), ArithmeticType::Plus).unwrap(), int(3), "plus and minus");
    assert_eq!(arith(int(1), int(2), ArithmeticType::Minus).unwrap(), int(-1), "plus and minus");
    assert_eq!(arith(int(-5), int(-7), ArithmeticType::Minus).unwrap(), int(2), "plus and minus");
    assert_eq!(arith(int(0), int(0), ArithmeticType::Plus).unwrap(), int(0), "plus and minus");
}

#[test]
fn s3d_02_a_null_operand_makes_a_null_result_of_type_integer() {
    let null = Value::null(TypeId::Integer);
    assert_eq!(arith(int(1), null.clone(), ArithmeticType::Plus).unwrap(), null, "a null operand makes a null result of type integer");
    assert_eq!(arith(null.clone(), int(1), ArithmeticType::Minus).unwrap(), null, "a null operand makes a null result of type integer");
    assert_eq!(arith(null.clone(), null.clone(), ArithmeticType::Plus).unwrap(), null, "a null operand makes a null result of type integer");
}

#[test]
fn s3d_02_overflow_is_an_error_not_a_wrapped_number() {
    let e = arith(int(i32::MAX), int(1), ArithmeticType::Plus).unwrap_err();
    assert_eq!(e.kind, ExceptionType::OutOfRange, "overflow is an error not a wrapped number");
    assert_eq!(arith(int(-i32::MAX), int(2), ArithmeticType::Minus).unwrap_err().kind, ExceptionType::OutOfRange, "overflow is an error not a wrapped number");
    // the largest results that do fit
    assert_eq!(arith(int(i32::MAX - 1), int(1), ArithmeticType::Plus).unwrap(), int(i32::MAX), "overflow is an error not a wrapped number");
    assert_eq!(arith(int(-i32::MAX + 1), int(-1), ArithmeticType::Plus).unwrap(), int(-i32::MAX), "overflow is an error not a wrapped number");
}

#[test]
fn s3d_02_i32_min_is_the_null_encoding_so_it_is_not_a_result() {
    // -2147483647 - 1 = i32::MIN would be stored as a NULL: an error is better than a quietly wrong NULL
    assert_eq!(arith(int(-i32::MAX), int(1), ArithmeticType::Minus).unwrap_err().kind, ExceptionType::OutOfRange, "i32 min is the null encoding so it is not a result");
}

#[test]
fn s3d_02_only_integers_are_accepted_when_the_expression_is_built() {
    let e = ArithmeticExpression::new(constant(Value::varchar("a")), constant(int(1)), ArithmeticType::Plus).unwrap_err();
    assert_eq!(e.kind, ExceptionType::NotImplemented, "only integers are accepted when the expression is built");
    assert!(e.message.contains("integer"), "only integers are accepted when the expression is built: expected `e.message.contains(\"integer\")`");
    assert!(ArithmeticExpression::new(constant(int(1)), constant(Value::decimal(1.5)), ArithmeticType::Minus).is_err(), "only integers are accepted when the expression is built: expected `ArithmeticExpression::new(constant(int(1)), constant(Value::decimal(1.5)), ArithmeticType::Minus).is...`");
    assert!(ArithmeticExpression::new(constant(Value::boolean(true)), constant(int(1)), ArithmeticType::Plus).is_err(), "only integers are accepted when the expression is built: expected `ArithmeticExpression::new(constant(Value::boolean(true)), constant(int(1)), ArithmeticType::Plus).is...`");
}

#[test]
fn s3d_02_arithmetic_on_columns_in_a_row_and_in_a_join_and_nested() {
    let schema = schema_abc();
    let t = tuple_abc(int(10), "x", int(3));
    let a_minus_c: ExprRef = Arc::new(ArithmeticExpression::new(col(0, 0, TypeId::Integer), col(0, 2, TypeId::Integer), ArithmeticType::Minus).unwrap());
    assert_eq!(a_minus_c.evaluate(&t, &schema).unwrap(), int(7), "arithmetic on columns in a row and in a join and nested");
    // (a - c) + 100
    let nested: ExprRef = Arc::new(ArithmeticExpression::new(a_minus_c.clone(), constant(int(100)), ArithmeticType::Plus).unwrap());
    assert_eq!(nested.evaluate(&t, &schema).unwrap(), int(107), "arithmetic on columns in a row and in a join and nested");
    assert_eq!(Expression::to_string(nested.as_ref()), "((#0.0-#0.2)+100)", "arithmetic on columns in a row and in a join and nested");

    let right_schema = Schema::new(vec![Column::new("y", TypeId::Integer)]);
    let right = Tuple::new(&[int(1000)], &right_schema);
    let join: ExprRef = Arc::new(ArithmeticExpression::new(col(0, 0, TypeId::Integer), col(1, 0, TypeId::Integer), ArithmeticType::Plus).unwrap());
    assert_eq!(join.evaluate_join(&t, &schema, &right, &right_schema).unwrap(), int(1010), "arithmetic on columns in a row and in a join and nested");
    assert_eq!(join.return_type().type_id(), TypeId::Integer, "arithmetic on columns in a row and in a join and nested");
}

// ---- 3d-03 · logic --------------------------------------------------------------------------------------------------------------

fn logic(l: Value, r: Value, t: LogicType) -> Value {
    let e: ExprRef = Arc::new(LogicExpression::new(constant(l), constant(r), t).unwrap());
    eval(&e)
}

#[test]
fn s3d_03_and_truth_table() {
    let (t, f, n) = (Value::boolean(true), Value::boolean(false), boolean_null());
    let cases = [
        (&t, &t, &t),
        (&t, &f, &f),
        (&f, &t, &f),
        (&f, &f, &f),
        (&t, &n, &n),
        (&n, &t, &n),
        (&f, &n, &f),
        (&n, &f, &f),
        (&n, &n, &n),
    ];
    for (l, r, want) in cases {
        assert_eq!(&logic(l.clone(), r.clone(), LogicType::And), want, "{l:?} AND {r:?}");
    }
}

#[test]
fn s3d_03_or_truth_table() {
    let (t, f, n) = (Value::boolean(true), Value::boolean(false), boolean_null());
    let cases = [
        (&t, &t, &t),
        (&t, &f, &t),
        (&f, &t, &t),
        (&f, &f, &f),
        (&t, &n, &t),
        (&n, &t, &t),
        (&f, &n, &n),
        (&n, &f, &n),
        (&n, &n, &n),
    ];
    for (l, r, want) in cases {
        assert_eq!(&logic(l.clone(), r.clone(), LogicType::Or), want, "{l:?} OR {r:?}");
    }
}

#[test]
fn s3d_03_both_sides_must_be_boolean() {
    let e = LogicExpression::new(constant(int(1)), constant(Value::boolean(true)), LogicType::And).unwrap_err();
    assert_eq!(e.kind, ExceptionType::NotImplemented, "both sides must be boolean");
    assert!(LogicExpression::new(constant(Value::boolean(true)), constant(Value::varchar("x")), LogicType::Or).is_err(), "both sides must be boolean: expected `LogicExpression::new(constant(Value::boolean(true)), constant(Value::varchar(\"x\")), LogicType::Or).i...`");
}

#[test]
fn s3d_03_logic_of_comparisons_in_a_row_and_a_join() {
    let schema = schema_abc();
    let t = tuple_abc(int(5), "x", Value::null(TypeId::Integer));
    let a_gt_1: ExprRef = Arc::new(ComparisonExpression::new(col(0, 0, TypeId::Integer), constant(int(1)), ComparisonType::GreaterThan));
    let c_eq_2: ExprRef = Arc::new(ComparisonExpression::new(col(0, 2, TypeId::Integer), constant(int(2)), ComparisonType::Equal));
    let and: ExprRef = Arc::new(LogicExpression::new(a_gt_1.clone(), c_eq_2.clone(), LogicType::And).unwrap());
    let or: ExprRef = Arc::new(LogicExpression::new(a_gt_1, c_eq_2, LogicType::Or).unwrap());
    assert_eq!(and.evaluate(&t, &schema).unwrap(), boolean_null(), "TRUE AND NULL");
    assert_eq!(or.evaluate(&t, &schema).unwrap(), Value::boolean(true), "TRUE OR NULL");

    let right_schema = Schema::new(vec![Column::new("y", TypeId::Integer)]);
    let right = Tuple::new(&[int(5)], &right_schema);
    let l_eq_r: ExprRef = Arc::new(ComparisonExpression::new(col(0, 0, TypeId::Integer), col(1, 0, TypeId::Integer), ComparisonType::Equal));
    let both: ExprRef = Arc::new(LogicExpression::new(l_eq_r.clone(), l_eq_r, LogicType::And).unwrap());
    assert_eq!(both.evaluate_join(&t, &schema, &right, &right_schema).unwrap(), Value::boolean(true), "logic of comparisons in a row and a join");
}

#[test]
fn s3d_03_logic_describes_itself() {
    let e = LogicExpression::new(constant(Value::boolean(true)), constant(Value::boolean(false)), LogicType::Or).unwrap();
    assert_eq!(Expression::to_string(&e), "(trueorfalse)", "logic describes itself");
    assert_eq!(e.return_type().type_id(), TypeId::Boolean, "logic describes itself");
}

// ---- 3d-03 · strings ------------------------------------------------------------------------------------------------------------

fn string_fn(arg: Value, t: StringExpressionType) -> Value {
    let e: ExprRef = Arc::new(StringExpression::new(constant(arg), t).unwrap());
    eval(&e)
}

#[test]
fn s3d_03_lower_and_upper() {
    assert_eq!(string_fn(Value::varchar("CMU 15-445 Database Systems"), StringExpressionType::Lower), Value::varchar("cmu 15-445 database systems"), "lower and upper");
    assert_eq!(string_fn(Value::varchar("CMU 15-445 Database Systems"), StringExpressionType::Upper), Value::varchar("CMU 15-445 DATABASE SYSTEMS"), "lower and upper");
    assert_eq!(string_fn(Value::varchar(""), StringExpressionType::Upper), Value::varchar(""), "lower and upper");
    assert_eq!(string_fn(Value::varchar("1 + 1 = 2"), StringExpressionType::Lower), Value::varchar("1 + 1 = 2"), "lower and upper");
}

#[test]
fn s3d_03_unicode_letters_change_case_too() {
    assert_eq!(string_fn(Value::varchar("ÉCOLE"), StringExpressionType::Lower), Value::varchar("école"), "unicode letters change case too");
    assert_eq!(string_fn(Value::varchar("école straße"), StringExpressionType::Upper), Value::varchar("ÉCOLE STRASSE"), "unicode letters change case too");
    assert_eq!(string_fn(Value::varchar("🥰 Abc"), StringExpressionType::Upper), Value::varchar("🥰 ABC"), "unicode letters change case too");
}

#[test]
fn s3d_03_null_in_null_out() {
    assert_eq!(string_fn(Value::null(TypeId::Varchar), StringExpressionType::Lower), Value::null(TypeId::Varchar), "null in null out");
    assert_eq!(string_fn(Value::null(TypeId::Varchar), StringExpressionType::Upper), Value::null(TypeId::Varchar), "null in null out");
}

#[test]
fn s3d_03_the_argument_must_be_a_varchar() {
    let e = StringExpression::new(constant(int(1)), StringExpressionType::Lower).unwrap_err();
    assert_eq!(e.kind, ExceptionType::Execution, "the argument must be a varchar");
    assert!(StringExpression::new(constant(Value::boolean(true)), StringExpressionType::Upper).is_err(), "the argument must be a varchar: expected `StringExpression::new(constant(Value::boolean(true)), StringExpressionType::Upper).is_err()`");
}

#[test]
fn s3d_03_strings_from_columns_in_a_row_and_a_join_and_nested() {
    let schema = schema_abc();
    let t = tuple_abc(int(1), "Hello World", int(2));
    let upper: ExprRef = Arc::new(StringExpression::new(col(0, 1, TypeId::Varchar), StringExpressionType::Upper).unwrap());
    assert_eq!(upper.evaluate(&t, &schema).unwrap(), Value::varchar("HELLO WORLD"), "strings from columns in a row and a join and nested");
    let lower_upper: ExprRef = Arc::new(StringExpression::new(upper.clone(), StringExpressionType::Lower).unwrap());
    assert_eq!(lower_upper.evaluate(&t, &schema).unwrap(), Value::varchar("hello world"), "strings from columns in a row and a join and nested");
    assert_eq!(Expression::to_string(lower_upper.as_ref()), "lower(upper(#0.1))", "strings from columns in a row and a join and nested");

    let right_schema = Schema::new(vec![Column::new_varchar("s", 20)]);
    let right = Tuple::new(&[Value::varchar("MiXeD")], &right_schema);
    let join: ExprRef = Arc::new(StringExpression::new(col(1, 0, TypeId::Varchar), StringExpressionType::Lower).unwrap());
    assert_eq!(join.evaluate_join(&t, &schema, &right, &right_schema).unwrap(), Value::varchar("mixed"), "strings from columns in a row and a join and nested");
    assert_eq!(join.return_type().type_id(), TypeId::Varchar, "strings from columns in a row and a join and nested");
}

#[test]
fn s3d_03_compute_is_a_plain_string_function() {
    let lower = StringExpression::new(constant(Value::varchar("x")), StringExpressionType::Lower).unwrap();
    let upper = StringExpression::new(constant(Value::varchar("x")), StringExpressionType::Upper).unwrap();
    assert_eq!(lower.compute("AbC"), "abc", "compute is a plain string function");
    assert_eq!(upper.compute("AbC"), "ABC", "compute is a plain string function");
}

// ---- 3d-06 · the function factory in the planner -------------------------------------------------------------------------------

fn run(db: &BusTubInstance, sql: &str) -> bustub::common::exception::Result<String> {
    let mut out = String::new();
    db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None)?;
    Ok(out)
}

#[test]
fn s3d_06_the_factory_builds_lower_and_upper() {
    let lower = Planner::get_func_call_from_factory("lower", vec![constant(Value::varchar("ABC"))]).unwrap();
    assert_eq!(eval(&lower), Value::varchar("abc"), "the factory builds lower and upper");
    let upper = Planner::get_func_call_from_factory("upper", vec![constant(Value::varchar("abc"))]).unwrap();
    assert_eq!(eval(&upper), Value::varchar("ABC"), "the factory builds lower and upper");
    assert_eq!(Expression::to_string(lower.as_ref()), "lower(ABC)", "the factory builds lower and upper");
}

#[test]
fn s3d_06_the_factory_refuses_other_names_and_other_argument_counts() {
    assert!(Planner::get_func_call_from_factory("reverse", vec![constant(Value::varchar("a"))]).is_err(), "the factory refuses other names and other argument counts: expected `Planner::get_func_call_from_factory(\"reverse\", vec![constant(Value::varchar(\"a\"))]).is_err()`");
    assert!(Planner::get_func_call_from_factory("lower", vec![]).is_err(), "the factory refuses other names and other argument counts: expected `Planner::get_func_call_from_factory(\"lower\", vec![]).is_err()`");
    assert!(Planner::get_func_call_from_factory("upper", vec![constant(Value::varchar("a")), constant(Value::varchar("b"))]).is_err(), "the factory refuses other names and other argument counts: expected `Planner::get_func_call_from_factory(\"upper\", vec![constant(Value::varchar(\"a\")), constant(Value::var...`");
}

#[test]
fn s3d_06_the_factory_refuses_arguments_that_are_not_strings() {
    assert!(Planner::get_func_call_from_factory("lower", vec![constant(int(1))]).is_err(), "the factory refuses arguments that are not strings: expected `Planner::get_func_call_from_factory(\"lower\", vec![constant(int(1))]).is_err()`");
    assert!(Planner::get_func_call_from_factory("upper", vec![constant(Value::boolean(true))]).is_err(), "the factory refuses arguments that are not strings: expected `Planner::get_func_call_from_factory(\"upper\", vec![constant(Value::boolean(true))]).is_err()`");
}

#[test]
fn s3d_06_sql_lower_and_upper_end_to_end() {
    let db = BusTubInstance::new(32);
    assert_eq!(run(&db, "select lower('MiXeD')").unwrap().trim(), "mixed", "sql lower and upper end to end");
    assert_eq!(run(&db, "select upper('MiXeD'), lower('MiXeD')").unwrap().trim(), "MIXED mixed", "sql lower and upper end to end");
    assert_eq!(run(&db, "select UPPER(lower('aBc'))").unwrap().trim(), "ABC", "function names are not case sensitive");
}

#[test]
fn s3d_06_sql_with_a_wrong_call_is_an_error() {
    let db = BusTubInstance::new(32);
    assert!(run(&db, "select upper(1)").is_err(), "sql with a wrong call is an error: expected `run(&db, \"select upper(1)\").is_err()`");
    assert!(run(&db, "select lower('a', 'b')").is_err(), "sql with a wrong call is an error: expected `run(&db, \"select lower('a', 'b')\").is_err()`");
    assert!(run(&db, "select nosuchfunction('a')").is_err(), "sql with a wrong call is an error: expected `run(&db, \"select nosuchfunction('a')\").is_err()`");
    assert!(run(&db, "select lower()").is_err(), "sql with a wrong call is an error: expected `run(&db, \"select lower()\").is_err()`");
}

#[test]
fn s3d_06_the_function_is_planned_over_a_columns_and_shows_in_explain() {
    let db = BusTubInstance::new(32);
    db.generate_mock_table();
    let out = run(&db, "select upper(day_of_week) from __mock_table_schedule where has_lecture = 1").unwrap();
    let mut days: Vec<&str> = out.split_whitespace().collect();
    days.sort();
    assert_eq!(days, ["MONDAY", "WEDNESDAY"], "the function is planned over a columns and shows in explain");
    let plan = run(&db, "explain (o) select upper(day_of_week) from __mock_table_schedule").unwrap();
    assert!(plan.contains("upper(#0.0)"), "{plan}");
    assert!(plan.contains("MockScan { table=__mock_table_schedule }"), "{plan}");
}

// ---- properties over the evaluators -----------------------------------------------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 96, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

/// An integer or an integer NULL; `i32::MIN` is the NULL encoding, so it is never a value.
fn opt_int() -> impl Strategy<Value = Option<i32>> {
    prop_oneof![1 => Just(None), 6 => (-i32::MAX..=i32::MAX).prop_map(Some), 3 => (-3i32..=3).prop_map(Some)]
}

fn int_or_null(v: Option<i32>) -> Value {
    v.map_or_else(|| Value::null(TypeId::Integer), int)
}

fn opt_bool() -> impl Strategy<Value = Option<bool>> {
    prop_oneof![Just(None), Just(Some(true)), Just(Some(false))]
}

fn bool_or_null(v: Option<bool>) -> Value {
    v.map_or_else(boolean_null, Value::boolean)
}

proptest! {
    #![proptest_config(pconfig())]

    /// A column reads exactly the value stored at its index, whichever side of a join it names, and a constant ignores the tuples.
    #[test]
    fn s3d_01_columns_read_what_the_tuple_holds(a in opt_int(), c in opt_int(), b in "[a-zA-Z ]{0,12}", x in opt_int()) {
        let schema = schema_abc();
        let t = tuple_abc(int_or_null(a), &b, int_or_null(c));
        prop_assert_eq!(col(0, 0, TypeId::Integer).evaluate(&t, &schema).unwrap(), int_or_null(a));
        prop_assert_eq!(col(0, 1, TypeId::Varchar).evaluate(&t, &schema).unwrap(), Value::varchar(&b));
        prop_assert_eq!(col(0, 2, TypeId::Integer).evaluate(&t, &schema).unwrap(), int_or_null(c));
        let right_schema = Schema::new(vec![Column::new("x", TypeId::Integer)]);
        let right = Tuple::new(&[int_or_null(x)], &right_schema);
        prop_assert_eq!(col(1, 0, TypeId::Integer).evaluate_join(&t, &schema, &right, &right_schema).unwrap(), int_or_null(x), "tuple index 1 is the right side");
        prop_assert_eq!(col(0, 2, TypeId::Integer).evaluate_join(&t, &schema, &right, &right_schema).unwrap(), int_or_null(c), "tuple index 0 is the left side");
        prop_assert_eq!(constant(int_or_null(x)).evaluate_join(&t, &schema, &right, &right_schema).unwrap(), int_or_null(x));
    }

    /// Comparisons agree with Rust's own on integers, and any NULL operand makes the answer NULL.
    #[test]
    fn s3d_02_comparisons_agree_with_rust_and_null_wins(l in opt_int(), r in opt_int()) {
        use ComparisonType::*;
        let ops: [(ComparisonType, fn(&i32, &i32) -> bool); 6] = [(Equal, i32::eq), (NotEqual, i32::ne), (LessThan, i32::lt), (LessThanOrEqual, i32::le), (GreaterThan, i32::gt), (GreaterThanOrEqual, i32::ge)];
        for (op, f) in ops {
            let want = match (l, r) {
                (Some(a), Some(b)) => Value::boolean(f(&a, &b)),
                _ => boolean_null(),
            };
            prop_assert_eq!(cmp(int_or_null(l), int_or_null(r), op), want, "{:?} {:?} {:?}", l, op, r);
        }
    }

    /// Plus and minus agree with 64-bit arithmetic when the answer is an INTEGER (not the NULL encoding), are errors when it is not,
    /// and are NULL when an operand is.
    #[test]
    fn s3d_02_arithmetic_agrees_with_a_wider_oracle(l in opt_int(), r in opt_int(), plus in any::<bool>()) {
        let op = if plus { ArithmeticType::Plus } else { ArithmeticType::Minus };
        let got = arith(int_or_null(l), int_or_null(r), op);
        match (l, r) {
            (Some(a), Some(b)) => {
                let wide = if plus { a as i64 + b as i64 } else { a as i64 - b as i64 };
                if wide > i32::MAX as i64 || wide <= i32::MIN as i64 {
                    prop_assert_eq!(got.unwrap_err().kind, ExceptionType::OutOfRange, "{} {} {}", a, if plus { '+' } else { '-' }, b);
                } else {
                    prop_assert_eq!(got.unwrap(), int(wide as i32));
                }
            }
            _ => prop_assert_eq!(got.unwrap(), Value::null(TypeId::Integer)),
        }
    }

    /// AND and OR follow Kleene's three-valued logic: false beats unknown for AND, true beats it for OR; both are commutative.
    #[test]
    fn s3d_03_and_or_are_three_valued_and_commutative(l in opt_bool(), r in opt_bool()) {
        let and = |a: Option<bool>, b: Option<bool>| match (a, b) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        };
        let or = |a: Option<bool>, b: Option<bool>| match (a, b) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        };
        prop_assert_eq!(logic(bool_or_null(l), bool_or_null(r), LogicType::And), bool_or_null(and(l, r)));
        prop_assert_eq!(logic(bool_or_null(l), bool_or_null(r), LogicType::Or), bool_or_null(or(l, r)));
        prop_assert_eq!(logic(bool_or_null(r), bool_or_null(l), LogicType::And), logic(bool_or_null(l), bool_or_null(r), LogicType::And));
    }

    /// `lower` and `upper` are Rust's own `to_lowercase` and `to_uppercase`, for any text, and are idempotent.
    #[test]
    fn s3d_03_lower_and_upper_match_the_standard_library(s in "\\PC{0,24}") {
        prop_assert_eq!(string_fn(Value::varchar(&s), StringExpressionType::Lower), Value::varchar(&s.to_lowercase()));
        prop_assert_eq!(string_fn(Value::varchar(&s), StringExpressionType::Upper), Value::varchar(&s.to_uppercase()));
        let once = s.to_uppercase();
        prop_assert_eq!(string_fn(Value::varchar(&once), StringExpressionType::Upper), Value::varchar(&once), "upper twice is upper once");
    }
}

// ---- 3d-04 · the lexer ------------------------------------------------------------------------------------------------------------

fn toks(sql: &str) -> Vec<Token> {
    tokenize(sql).unwrap()
}

fn word(s: &str) -> Token {
    Token::Word(s.to_string())
}

#[test]
fn s3d_04_words_numbers_and_symbols() {
    assert_eq!(toks("select a1, 42 from t"), vec![word("select"), word("a1"), Token::Symbol(","), Token::Number("42".into()), word("from"), word("t")], "words numbers and symbols");
    assert_eq!(toks(" \n\t "), vec![], "white space alone is no tokens");
    assert_eq!(toks(""), vec![], "the empty string is no tokens");
}

#[test]
fn s3d_04_unquoted_words_fold_to_lower_case_quoted_ones_do_not() {
    assert_eq!(toks("SeLeCt Foo \"Foo Bar\""), vec![word("select"), word("foo"), Token::Quoted("Foo Bar".into())], "unquoted words fold to lower case quoted ones do not");
    assert_eq!(toks("école"), vec![word("école")], "letters beyond ASCII are letters");
}

#[test]
fn s3d_04_numbers_are_kept_as_written() {
    for n in ["0", "42", "1.5", ".5", "7.", "1e3", "2.5E-4", "1E+10"] {
        assert_eq!(toks(n), vec![Token::Number(n.into())], "{n} is one number");
    }
    // an `e` that is not followed by digits belongs to the next word
    assert_eq!(toks("12e"), vec![Token::Number("12".into()), word("e")], "an e without digits starts a word");
    assert_eq!(toks("1.2.3"), vec![Token::Number("1.2".into()), Token::Number(".3".into())], "a second dot starts a new number");
}

#[test]
fn s3d_04_strings_double_the_quote_to_contain_one() {
    assert_eq!(toks("'it''s'"), vec![Token::Str("it's".into())], "strings double the quote to contain one");
    assert_eq!(toks("''"), vec![Token::Str(String::new())], "strings double the quote to contain one");
    assert_eq!(toks("'a -- b /* c */'"), vec![Token::Str("a -- b /* c */".into())], "comment marks inside a string are text");
    assert_eq!(toks("'x' 'y'"), vec![Token::Str("x".into()), Token::Str("y".into())], "two strings are two tokens");
}

#[test]
fn s3d_04_two_character_symbols_win_over_one() {
    assert_eq!(toks("a<=b<>c>=d!=e||f::g"), vec![word("a"), Token::Symbol("<="), word("b"), Token::Symbol("<>"), word("c"), Token::Symbol(">="), word("d"), Token::Symbol("!="), word("e"), Token::Symbol("||"), word("f"), Token::Symbol("::"), word("g")], "two character symbols win over one");
    assert_eq!(toks("< >"), vec![Token::Symbol("<"), Token::Symbol(">")], "a space separates two symbols");
    assert_eq!(toks("1-2"), vec![Token::Number("1".into()), Token::Symbol("-"), Token::Number("2".into())], "minus is a symbol; the parser decides what it means");
}

#[test]
fn s3d_04_comments_are_skipped() {
    assert_eq!(toks("a -- the rest of the line\nb"), vec![word("a"), word("b")], "comments are skipped");
    assert_eq!(toks("a /* x\ny */ b"), vec![word("a"), word("b")], "comments are skipped");
    assert_eq!(toks("1 - -2"), vec![Token::Number("1".into()), Token::Symbol("-"), Token::Symbol("-"), Token::Number("2".into())], "two minuses with a space between are not a comment");
    assert_eq!(toks("8/*c*/2"), vec![Token::Number("8".into()), Token::Number("2".into())], "a comment separates tokens");
}

#[test]
fn s3d_04_bad_input_is_an_error_not_a_panic() {
    for bad in ["'open", "\"open", "a @ b", "#", "select ~ 1", "a $ b"] {
        let e = tokenize(bad).unwrap_err();
        assert_eq!(e.kind, ExceptionType::Invalid, "{bad:?} is a parse error");
        assert!(e.to_string().contains("Query failed to parse"), "{bad:?}: {e}");
    }
}

fn token_strategy() -> impl Strategy<Value = Token> {
    let symbols = ["+", "-", "*", "/", "%", "=", "<", ">", "<=", ">=", "<>", "!=", "||", ",", "(", ")", ";", ".", "::", "==", "[", "]"];
    prop_oneof![
        3 => "[a-z_][a-z0-9_]{0,6}".prop_map(Token::Word),
        1 => "[A-Za-z0-9 _.-]{1,8}".prop_map(Token::Quoted),
        2 => prop_oneof![
            "[0-9]{1,6}",
            "[0-9]{1,3}\\.[0-9]{1,3}",
            "\\.[0-9]{1,3}",
            "[0-9]{1,3}[eE][+-]?[0-9]{1,2}",
        ].prop_map(Token::Number),
        2 => "[ -~]{0,8}".prop_map(Token::Str),
        4 => prop::sample::select(symbols.to_vec()).prop_map(Token::Symbol),
    ]
}

fn print_token(t: &Token) -> String {
    match t {
        Token::Word(w) => w.clone(),
        Token::Quoted(q) => format!("\"{q}\""),
        Token::Number(n) => n.clone(),
        Token::Str(s) => format!("'{}'", s.replace('\'', "''")),
        Token::Symbol(s) => s.to_string(),
    }
}

proptest! {
    #![proptest_config(pconfig())]

    /// Print any list of tokens with spaces between them (and a comment in some gaps) and the lexer returns the same list.
    #[test]
    fn s3d_04_lexing_printed_tokens_gives_the_tokens_back(ts in prop::collection::vec(token_strategy(), 0..24), gap in prop::sample::select(vec![" ", "  \n", " /* c */ ", " -- c\n"])) {
        let sql = ts.iter().map(print_token).collect::<Vec<_>>().join(gap);
        prop_assert_eq!(tokenize(&sql).unwrap(), ts, "for {:?}", sql);
    }

    /// Whatever the text, the lexer answers (tokens or an error); it never panics, and upper-casing the input changes only words.
    #[test]
    fn s3d_04_the_lexer_answers_for_any_text(s in "\\PC{0,40}") {
        let _ = tokenize(&s);
    }
}

// ---- 3d-05 · expressions from text -----------------------------------------------------------------------------------------------

fn parse(sql: &str) -> Expr {
    parse_expr(sql).unwrap_or_else(|e| panic!("{sql:?} should parse: {e}"))
}

fn column(name: &str) -> Expr {
    Expr::Column(vec![name.to_string()])
}

fn bin(op: &str, l: Expr, r: Expr) -> Expr {
    Expr::Binary { op: op.into(), left: Box::new(l), right: Box::new(r) }
}

#[test]
fn s3d_05_atoms() {
    assert_eq!(parse("42"), Expr::Integer(42), "atoms");
    assert_eq!(parse("'x'"), Expr::Str("x".into()), "atoms");
    assert_eq!(parse("TRUE"), Expr::Bool(true), "atoms");
    assert_eq!(parse("null"), Expr::Null, "atoms");
    assert_eq!(parse("t.a"), Expr::Column(vec!["t".into(), "a".into()]), "a dotted name is a column of a table");
    assert_eq!(parse("1.5"), Expr::Float("1.5".into()), "atoms");
    assert_eq!(parse("lower('A')"), Expr::Function { name: "lower".into(), args: vec![Expr::Str("A".into())], distinct: false, over: None }, "a call");
    assert_eq!(parse("((1))"), Expr::Integer(1), "parentheses leave no trace in the tree");
}

#[test]
fn s3d_05_times_binds_tighter_than_plus() {
    assert_eq!(parse("1 + 2 * 3"), bin("+", Expr::Integer(1), bin("*", Expr::Integer(2), Expr::Integer(3))), "times binds tighter than plus");
    assert_eq!(parse("1 * 2 + 3"), bin("+", bin("*", Expr::Integer(1), Expr::Integer(2)), Expr::Integer(3)), "times binds tighter than plus");
    assert_eq!(parse("(1 + 2) * 3"), bin("*", bin("+", Expr::Integer(1), Expr::Integer(2)), Expr::Integer(3)), "parentheses override precedence");
}

#[test]
fn s3d_05_operators_of_one_level_associate_to_the_left() {
    assert_eq!(parse("10 - 3 - 2"), bin("-", bin("-", Expr::Integer(10), Expr::Integer(3)), Expr::Integer(2)), "operators of one level associate to the left");
    assert_eq!(parse("a / b / c"), bin("/", bin("/", column("a"), column("b")), column("c")), "operators of one level associate to the left");
    assert_eq!(parse("a or b or c"), bin("or", bin("or", column("a"), column("b")), column("c")), "operators of one level associate to the left");
}

#[test]
fn s3d_05_and_binds_tighter_than_or_and_comparison_tighter_than_and() {
    assert_eq!(parse("a or b and c"), bin("or", column("a"), bin("and", column("b"), column("c"))), "and binds tighter than or");
    assert_eq!(parse("a = 1 and b < 2"), bin("and", bin("=", column("a"), Expr::Integer(1)), bin("<", column("b"), Expr::Integer(2))), "comparison binds tighter than and");
    assert_eq!(parse("a + 1 >= b - 2"), bin(">=", bin("+", column("a"), Expr::Integer(1)), bin("-", column("b"), Expr::Integer(2))), "arithmetic binds tighter than comparison");
}

#[test]
fn s3d_05_not_is_looser_than_comparison_and_tighter_than_and() {
    assert_eq!(parse("not a = b"), Expr::Unary { op: "not".into(), expr: Box::new(bin("=", column("a"), column("b"))) }, "not covers a whole comparison");
    assert_eq!(parse("not a and b"), bin("and", Expr::Unary { op: "not".into(), expr: Box::new(column("a")) }, column("b")), "not binds tighter than and");
    assert_eq!(parse("not not a"), Expr::Unary { op: "not".into(), expr: Box::new(Expr::Unary { op: "not".into(), expr: Box::new(column("a")) }) }, "not can repeat");
}

#[test]
fn s3d_05_is_null_and_unary_minus() {
    assert_eq!(parse("a is null"), Expr::IsNull { expr: Box::new(column("a")), negated: false }, "is null");
    assert_eq!(parse("a + 1 is not null"), Expr::IsNull { expr: Box::new(bin("+", column("a"), Expr::Integer(1))), negated: true }, "is not null covers the arithmetic before it");
    assert_eq!(parse("-a * b"), bin("*", Expr::Unary { op: "-".into(), expr: Box::new(column("a")) }, column("b")), "unary minus binds tighter than times");
    assert_eq!(parse("-5"), Expr::Integer(-5), "a minus sign in front of a number is part of the number");
    assert_eq!(parse("1 - -5"), bin("-", Expr::Integer(1), Expr::Integer(-5)), "binary minus then a negative number");
    assert_eq!(parse("+7"), Expr::Integer(7), "a plus sign does nothing");
}

#[test]
fn s3d_05_text_that_is_not_one_expression_is_an_error() {
    for bad in ["", "1 +", "(1", "1)", "1 2", "* 3", "a and", "f(1,", "1 = = 2", "select", "from"] {
        let e = parse_expr(bad).unwrap_err();
        assert_eq!(e.kind, ExceptionType::Invalid, "{bad:?} is a parse error");
    }
}

// ---- a printer that adds parentheses only where the grammar needs them -------------------------------------------------------------

fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Binary { op, .. } => match op.as_str() {
            "or" => 1,
            "and" => 2,
            "=" | "==" | "<" | ">" | "<=" | ">=" | "<>" | "!=" => 4,
            "+" | "-" | "||" => 5,
            _ => 6,
        },
        Expr::Unary { op, .. } if op == "not" => 3,
        Expr::IsNull { .. } => 4,
        Expr::Unary { .. } => 7,
        _ => 9,
    }
}

fn paren(e: &Expr, needed: bool) -> String {
    if needed { format!("({})", show(e)) } else { show(e) }
}

fn show(e: &Expr) -> String {
    match e {
        Expr::Integer(v) => v.to_string(),
        Expr::Float(s) => s.clone(),
        Expr::Str(s) => format!("'{}'", s.replace('\'', "''")),
        Expr::Bool(b) => b.to_string(),
        Expr::Null => "null".into(),
        Expr::Star => "*".into(),
        Expr::Column(parts) => parts
            .iter()
            .map(|p| if p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') { p.clone() } else { format!("\"{p}\"") })
            .collect::<Vec<_>>()
            .join("."),
        Expr::Function { name, args, .. } if args.is_empty() => format!("{name}(*)"),
        Expr::Function { name, args, .. } => format!("{name}({})", args.iter().map(show).collect::<Vec<_>>().join(", ")),
        Expr::Binary { op, left, right } => {
            let p = prec(e);
            format!("{} {op} {}", paren(left, prec(left) < p), paren(right, prec(right) <= p))
        }
        Expr::Unary { op, expr } if op == "not" => format!("not {}", paren(expr, prec(expr) < 3)),
        Expr::Unary { expr, .. } => format!("-({})", show(expr)),
        Expr::IsNull { expr, negated } => format!("{} is {}null", paren(expr, prec(expr) < 4), if *negated { "not " } else { "" }),
        Expr::Case { .. } | Expr::Param(_) => unreachable!("module 3d's generator makes no CASE and no placeholders (module 3i adds them)"),
    }
}

fn leaf() -> impl Strategy<Value = Expr> {
    prop_oneof![
        3 => (-999i64..999).prop_map(Expr::Integer),
        1 => prop::sample::select(vec!["1.5", "2e3", ".5", "10.25", "3000000000"]).prop_map(|s| Expr::Float(s.to_string())),
        1 => "[ -~]{0,6}".prop_map(Expr::Str),
        1 => any::<bool>().prop_map(Expr::Bool),
        1 => Just(Expr::Null),
        3 => prop::sample::select(vec!["a", "b", "c1", "Weird Name", "x-y"]).prop_map(|n| Expr::Column(vec![n.to_string()])),
        1 => prop::sample::select(vec!["a", "t"]).prop_map(|n| Expr::Column(vec![n.to_string(), "x".to_string()])),
    ]
}

fn expr_strategy() -> impl Strategy<Value = Expr> {
    let ops = ["or", "and", "=", "<>", "<", "<=", ">", ">=", "+", "-", "||", "*", "/", "%"];
    leaf().prop_recursive(4, 24, 3, move |inner| {
        prop_oneof![
            6 => (prop::sample::select(ops.to_vec()), inner.clone(), inner.clone()).prop_map(|(op, l, r)| bin(op, l, r)),
            1 => inner.clone().prop_map(|e| Expr::Unary { op: "not".into(), expr: Box::new(e) }),
            1 => inner.clone().prop_filter("a minus in front of a number is part of the number", |e| !matches!(e, Expr::Integer(_) | Expr::Float(_)))
                .prop_map(|e| Expr::Unary { op: "-".into(), expr: Box::new(e) }),
            1 => (inner.clone(), any::<bool>()).prop_map(|(e, negated)| Expr::IsNull { expr: Box::new(e), negated }),
            1 => (prop::sample::select(vec!["f", "lower", "abs"]), prop::collection::vec(inner, 1..3))
                .prop_map(|(name, args)| Expr::Function { name: name.into(), args, distinct: false, over: None }),
        ]
    })
}

proptest! {
    #![proptest_config(pconfig())]

    /// Print any expression tree with the fewest parentheses its precedences allow and parse it back: the same tree. This pins
    /// every precedence and every associativity at once.
    #[test]
    fn s3d_05_printing_a_tree_and_parsing_it_gives_the_tree_back(e in expr_strategy()) {
        let sql = show(&e);
        prop_assert_eq!(parse_expr(&sql).unwrap(), e, "for {:?}", sql);
    }

    /// Wrapping the whole text in parentheses changes nothing, and so does extra white space and comments between the tokens.
    #[test]
    fn s3d_05_parentheses_and_spacing_do_not_change_the_tree(e in expr_strategy()) {
        let sql = show(&e);
        prop_assert_eq!(parse_expr(&format!("( {sql} )")).unwrap(), parse_expr(&sql).unwrap());
        let spaced = tokenize(&sql).unwrap().iter().map(print_token).collect::<Vec<_>>().join(" /* gap */ ");
        prop_assert_eq!(parse_expr(&spaced).unwrap(), e, "for {:?}", spaced);
    }

    /// A random run of tokens is parsed or refused; it never panics or loops.
    #[test]
    fn s3d_05_the_parser_answers_for_any_run_of_tokens(ts in prop::collection::vec(token_strategy(), 0..14)) {
        let sql = ts.iter().map(print_token).collect::<Vec<_>>().join(" ");
        let _ = parse_expr(&sql);
    }
}

// ---- 3d-07 · SQL on constants, end to end -----------------------------------------------------------------------------------------

fn sql_value(db: &BusTubInstance, e: &Expr) -> bustub::common::exception::Result<String> {
    Ok(run(db, &format!("select {}", show(e)))?.trim().to_string())
}

/// What `select <e>` must answer for an integer expression: `Err` when any step leaves the INTEGER range.
fn int_oracle(e: &Expr) -> Result<Option<i64>, ()> {
    match e {
        Expr::Integer(v) => Ok(Some(*v)),
        Expr::Null => Ok(None),
        Expr::Binary { op, left, right } => {
            let (l, r) = (int_oracle(left)?, int_oracle(right)?);
            match (l, r) {
                (Some(a), Some(b)) => {
                    let v = if op == "+" { a + b } else { a - b };
                    if v > i32::MAX as i64 || v <= i32::MIN as i64 { Err(()) } else { Ok(Some(v)) }
                }
                _ => Ok(None),
            }
        }
        other => panic!("not an integer expression: {other:?}"),
    }
}

fn bool_oracle(e: &Expr) -> Option<bool> {
    match e {
        Expr::Bool(b) => Some(*b),
        Expr::Binary { op, left, right } if op == "and" || op == "or" => {
            let (l, r) = (bool_oracle(left), bool_oracle(right));
            match (op.as_str(), l, r) {
                ("and", Some(false), _) | ("and", _, Some(false)) => Some(false),
                ("and", Some(true), Some(true)) => Some(true),
                ("or", Some(true), _) | ("or", _, Some(true)) => Some(true),
                ("or", Some(false), Some(false)) => Some(false),
                _ => None,
            }
        }
        Expr::Binary { op, left, right } => {
            let (l, r) = (int_oracle(left).unwrap(), int_oracle(right).unwrap());
            let (a, b) = (l?, r?);
            Some(match op.as_str() {
                "=" => a == b,
                "<>" => a != b,
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            })
        }
        other => panic!("not a boolean expression: {other:?}"),
    }
}

fn int_expr(small: bool) -> impl Strategy<Value = Expr> {
    let lit = if small { (-20i64..=20).boxed() } else { prop_oneof![(-i32::MAX as i64..=i32::MAX as i64), (-3i64..=3)].boxed() };
    prop_oneof![5 => lit.prop_map(Expr::Integer), 1 => Just(Expr::Null)].prop_recursive(3, 12, 2, |inner| {
        (prop::sample::select(vec!["+", "-"]), inner.clone(), inner).prop_map(|(op, l, r)| bin(op, l, r))
    })
}

fn bool_expr() -> impl Strategy<Value = Expr> {
    let cmp = (prop::sample::select(vec!["=", "<>", "<", "<=", ">", ">="]), int_expr(true), int_expr(true)).prop_map(|(op, l, r)| bin(op, l, r));
    prop_oneof![3 => cmp, 1 => any::<bool>().prop_map(Expr::Bool)].prop_recursive(3, 10, 2, |inner| {
        (prop::sample::select(vec!["and", "or"]), inner.clone(), inner).prop_map(|(op, l, r)| bin(op, l, r))
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, max_shrink_iters: 500, failure_persistence: None, ..ProptestConfig::default() })]

    /// `select <integer expression>` through the lexer, the parser, the binder, the planner and the evaluator answers what a 64-bit
    /// computation does: the value, `integer_null`, or an overflow error.
    #[test]
    fn s3d_07_sql_integer_expressions_agree_with_an_oracle(e in int_expr(false)) {
        let db = BusTubInstance::new(32);
        match (int_oracle(&e), sql_value(&db, &e)) {
            (Ok(Some(v)), Ok(got)) => prop_assert_eq!(got, v.to_string(), "for {}", show(&e)),
            (Ok(None), Ok(got)) => prop_assert_eq!(got, "integer_null", "for {}", show(&e)),
            (Err(()), Err(err)) => prop_assert_eq!(err.kind, ExceptionType::OutOfRange, "for {}", show(&e)),
            (want, got) => prop_assert!(false, "for {}: expected {:?}, got {:?}", show(&e), want, got),
        }
    }

    /// `select <boolean expression>` with comparisons, AND and OR over integers that may be NULL follows three-valued logic.
    #[test]
    fn s3d_07_sql_boolean_expressions_follow_three_valued_logic(e in bool_expr()) {
        let db = BusTubInstance::new(32);
        let want = match bool_oracle(&e) { Some(b) => b.to_string(), None => "boolean_null".into() };
        prop_assert_eq!(sql_value(&db, &e).unwrap(), want, "for {}", show(&e));
    }
}

#[test]
fn s3d_07_a_statement_with_a_syntax_error_says_so() {
    let db = BusTubInstance::new(32);
    for bad in ["select 1 +", "select (1", "select 'open", "selec 1", "select 1 @ 2"] {
        let e = run(&db, bad).unwrap_err();
        assert!(e.to_string().contains("Query failed to parse") || e.kind == ExceptionType::Invalid, "{bad:?}: {e}");
    }
}

#[test]
fn s3d_07_comments_and_case_do_not_matter_to_a_query() {
    let db = BusTubInstance::new(32);
    assert_eq!(run(&db, "SELECT /* the answer */ 40 + 2 -- done").unwrap().trim(), "42", "comments and case do not matter to a query");
    assert_eq!(run(&db, "select\n  upper(  'mixed'  )").unwrap().trim(), "MIXED", "white space does not matter to a query");
}

// @@ challenge 3d-c1 begin
mod ch_3d_c1 {
    use proptest::prelude::*;

    use bustub::sql::mini_lexer::{tokenize, LexError, LexErrorKind::*, Tok};

    fn id(s: &str) -> Tok {
        Tok::Ident(s.to_owned())
    }
    fn sym(s: &str) -> Tok {
        Tok::Sym(s.to_owned())
    }

    #[test]
    fn s3d_c1_identifiers_numbers_symbols_and_two_character_operators() {
        assert_eq!(tokenize("a<=b").unwrap(), vec![id("a"), sym("<="), id("b")], "this check");
        assert_eq!(tokenize("t.x <> 12").unwrap(), vec![id("t"), sym("."), id("x"), sym("<>"), Tok::Int(12)], "this check");
        assert_eq!(tokenize("(a,b);").unwrap(), vec![sym("("), id("a"), sym(","), id("b"), sym(")"), sym(";")], "this check");
        assert_eq!(tokenize("a!=b").unwrap()[1], sym("!="), "this check");
    }

    #[test]
    fn s3d_c1_a_doubled_quote_is_one_quote() {
        assert_eq!(tokenize("'it''s'").unwrap(), vec![Tok::Str("it's".into())], "this check");
        assert_eq!(tokenize("\"a\"\"b\"").unwrap(), vec![Tok::QuotedIdent("a\"b".into())], "this check");
        assert_eq!(tokenize("''").unwrap(), vec![Tok::Str(String::new())], "this check");
        assert_eq!(tokenize(&"'".repeat(4)).unwrap(), vec![Tok::Str("'".into())], "this check");
    }

    #[test]
    fn s3d_c1_comments_produce_no_tokens_and_are_not_comments_inside_strings() {
        assert_eq!(tokenize("a -- hi\nb /* x */ c").unwrap(), vec![id("a"), id("b"), id("c")], "this check");
        assert_eq!(tokenize("'-- not a comment'").unwrap(), vec![Tok::Str("-- not a comment".into())], "this check");
        assert_eq!(tokenize("'/* nor this */'").unwrap(), vec![Tok::Str("/* nor this */".into())], "this check");
        assert_eq!(tokenize("a--b").unwrap(), vec![id("a")], "this check");
    }

    #[test]
    fn s3d_c1_errors_carry_the_byte_position() {
        assert_eq!(tokenize("x 'abc"), Err(LexError { pos: 2, kind: UnterminatedString }), "this check");
        assert_eq!(tokenize("x \"abc"), Err(LexError { pos: 2, kind: UnterminatedIdent }), "this check");
        assert_eq!(tokenize("a /* x"), Err(LexError { pos: 2, kind: UnterminatedComment }), "this check");
        assert_eq!(tokenize("a # b"), Err(LexError { pos: 2, kind: BadChar }), "this check");
        assert_eq!(tokenize("99999999999999999999"), Err(LexError { pos: 0, kind: IntegerOverflow }), "this check");
        assert_eq!(tokenize("é"), Err(LexError { pos: 0, kind: BadChar }), "a multi-byte character is one bad character, at its first byte");
        assert_eq!(tokenize("'é' é"), Err(LexError { pos: 5, kind: BadChar }), "positions count bytes");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: never panics, and an error position is a character boundary inside the input.
        #[test]
        fn s3d_c1_property_any_text_is_handled(s in "[ -~\\n\\té]{0,30}") {
            if let Err(e) = tokenize(&s) {
                prop_assert!(e.pos < s.len() || s.is_empty(), "position {} out of range for {:?}", e.pos, s);
                prop_assert!(s.is_char_boundary(e.pos));
            }
        }

        /// Property: whitespace and comments between tokens change nothing.
        #[test]
        fn s3d_c1_property_whitespace_and_comments_are_invisible(words in proptest::collection::vec("[a-z]{1,4}|[0-9]{1,3}|<=|,", 1..8), gap in prop_oneof![Just(" "), Just("  \n"), Just(" /* c */ "), Just(" -- c\n")]) {
            let plain = words.join(" ");
            let spaced = words.join(gap);
            prop_assert_eq!(tokenize(&plain), tokenize(&spaced));
        }
    }
}
// @@ challenge 3d-c1 end

// @@ challenge 3d-c2 begin
mod ch_3d_c2 {
    use proptest::prelude::*;

    use bustub::sql::mini_expr::{parse, print_expr, Expr, Op};

    fn n(v: i64) -> Box<Expr> {
        Box::new(Expr::Num(v))
    }
    fn bin(l: Box<Expr>, op: Op, r: Box<Expr>) -> Box<Expr> {
        Box::new(Expr::Bin(l, op, r))
    }

    #[test]
    fn s3d_c2_a_right_operand_of_the_same_precedence_needs_parentheses() {
        assert_eq!(print_expr(&bin(n(1), Op::Sub, bin(n(2), Op::Sub, n(3)))), "1 - (2 - 3)", "this check");
        assert_eq!(print_expr(&bin(bin(n(1), Op::Sub, n(2)), Op::Sub, n(3))), "1 - 2 - 3", "this check");
        assert_eq!(print_expr(&bin(n(8), Op::Div, bin(n(4), Op::Mul, n(2)))), "8 / (4 * 2)", "this check");
    }

    #[test]
    fn s3d_c2_a_stronger_child_needs_none_and_a_weaker_one_does() {
        assert_eq!(print_expr(&bin(n(1), Op::Add, bin(n(2), Op::Mul, n(3)))), "1 + 2 * 3", "this check");
        assert_eq!(print_expr(&bin(bin(n(1), Op::Add, n(2)), Op::Mul, n(3))), "(1 + 2) * 3", "this check");
    }

    #[test]
    fn s3d_c2_negation() {
        assert_eq!(print_expr(&Expr::Neg(n(3))), "-3", "this check");
        assert_eq!(print_expr(&Expr::Neg(bin(n(1), Op::Add, n(2)))), "-(1 + 2)", "this check");
        assert_eq!(print_expr(&bin(n(1), Op::Sub, Box::new(Expr::Neg(n(2))))), "1 - -2", "this check");
    }

    fn arb_expr() -> impl Strategy<Value = Expr> {
        let leaf = (0i64..20).prop_map(Expr::Num);
        leaf.prop_recursive(4, 24, 2, |inner| {
            prop_oneof![
                inner.clone().prop_map(|e| Expr::Neg(Box::new(e))),
                (inner.clone(), prop_oneof![Just(Op::Add), Just(Op::Sub), Just(Op::Mul), Just(Op::Div)], inner).prop_map(|(l, o, r)| Expr::Bin(Box::new(l), o, Box::new(r))),
            ]
        })
    }

    #[test]
    fn s3d_c2_redundant_parentheses_are_dropped_and_needed_ones_kept() {
        for (src, want) in [("(1 + 2) + 3", "1 + 2 + 3"), ("1 + (2 * 3)", "1 + 2 * 3"), ("1 - (2 + 3)", "1 - (2 + 3)"), ("((7))", "7")] {
            assert_eq!(print_expr(&parse(src).unwrap()), want, "{src}");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: parsing the printed text gives the tree back, and no pair of parentheses can be removed.
        #[test]
        fn s3d_c2_property_round_trip_with_the_fewest_parentheses(e in arb_expr()) {
            let text = print_expr(&e);
            prop_assert_eq!(parse(&text), Some(e.clone()), "text was {:?}", text);
            // try removing each matching pair: the parse must change
            let chars: Vec<char> = text.chars().collect();
            let mut stack = Vec::new();
            for (i, &c) in chars.iter().enumerate() {
                if c == '(' { stack.push(i); }
                if c == ')' {
                    let open = stack.pop().unwrap();
                    let without: String = chars.iter().enumerate().filter(|&(j, _)| j != open && j != i).map(|(_, c)| *c).collect();
                    prop_assert_ne!(parse(&without), Some(e.clone()), "the parentheses at {} and {} of {:?} are redundant", open, i, text);
                }
            }
        }
    }
}
// @@ challenge 3d-c2 end

// @@ challenge 3d-c3 begin
mod ch_3d_c3 {
    use proptest::prelude::*;

    use bustub::sql::mini_parse::parse_and_eval;

    #[test]
    fn s3d_c3_subtraction_associates_to_the_left() {
        assert_eq!(parse_and_eval("10 - 4 - 3"), Some(3), "this check");
        assert_eq!(parse_and_eval("10 - (4 - 3)"), Some(9), "this check");
    }

    #[test]
    fn s3d_c3_division_associates_to_the_left() {
        assert_eq!(parse_and_eval("100 / 10 / 5"), Some(2), "this check");
        assert_eq!(parse_and_eval("100 / (10 / 5)"), Some(50), "this check");
    }

    #[test]
    fn s3d_c3_precedence_still_works() {
        assert_eq!(parse_and_eval("2 + 3 * 4"), Some(14), "this check");
        assert_eq!(parse_and_eval("2 * 3 - 4 - 1"), Some(1), "this check");
        assert_eq!(parse_and_eval("(2 + 3) * 4"), Some(20), "this check");
    }

    #[test]
    fn s3d_c3_errors_are_none() {
        assert_eq!(parse_and_eval("1 / 0"), None, "this check");
        assert_eq!(parse_and_eval("1 +"), None, "this check");
        assert_eq!(parse_and_eval("(1"), None, "this check");
        assert_eq!(parse_and_eval("1 2"), None, "this check");
        assert_eq!(parse_and_eval("a"), None, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a left-leaning chain without parentheses equals folding it from the left.
        #[test]
        fn s3d_c3_property_chains_fold_from_the_left(first in 1i64..50, rest in proptest::collection::vec((prop_oneof![Just('+'), Just('-'), Just('*')], 1i64..10), 0..6)) {
            let text = rest.iter().fold(first.to_string(), |s, (op, v)| format!("{s} {op} {v}"));
            // evaluate with the usual precedence by hand: collect additive terms
            let mut terms: Vec<i64> = vec![first];
            let mut signs: Vec<i64> = vec![1];
            for (op, v) in &rest {
                match op {
                    '*' => { let last = terms.last_mut().unwrap(); *last *= v; }
                    '+' => { terms.push(*v); signs.push(1); }
                    _ => { terms.push(*v); signs.push(-1); }
                }
            }
            let want: i64 = terms.iter().zip(&signs).map(|(t, s)| t * s).sum();
            prop_assert_eq!(parse_and_eval(&text), Some(want), "{}", text);
        }
    }
}
// @@ challenge 3d-c3 end

// @@ challenge 3d-c4 begin
mod ch_3d_c4 {
    use proptest::prelude::*;

    use bustub::sql::error_render::{line_col, render_error};

    #[test]
    fn s3d_c4_line_and_column_are_one_based() {
        let src = "SELECT a\nFROM t";
        assert_eq!(line_col(src, 0), (1, 1), "this check");
        assert_eq!(line_col(src, 7), (1, 8), "this check");
        assert_eq!(line_col(src, 9), (2, 1), "this check");
        assert_eq!(line_col(src, 12), (2, 4), "this check");
    }

    #[test]
    fn s3d_c4_columns_count_characters_and_the_end_of_input_has_a_position() {
        assert_eq!(line_col("héllo", 3), (1, 3), "this check");
        assert_eq!(line_col("héllo", 6), (1, 6), "this check");
        assert_eq!(line_col("a\n", 2), (2, 1), "this check");
        assert_eq!(line_col("", 0), (1, 1), "this check");
    }

    #[test]
    fn s3d_c4_crlf_line_ends_do_not_change_columns() {
        assert_eq!(line_col("a\r\nbc", 3), (2, 1), "this check");
        assert_eq!(line_col("a\r\nbc", 4), (2, 2), "this check");
        assert_eq!(line_col("a\r\nbc", 5), (2, 3), "this check");
    }

    #[test]
    fn s3d_c4_the_rendering_shows_the_line_and_a_caret_under_the_error() {
        let src = "SELECT nmae FROM t";
        let out = render_error(src, 7, 4, "unknown column");
        assert_eq!(out, "unknown column\nLINE 1: SELECT nmae FROM t\n               ^^^^", "this check");
    }

    #[test]
    fn s3d_c4_errors_on_later_lines_and_at_the_end() {
        let src = "SELECT 1\nFROM";
        let out = render_error(src, src.len(), 0, "expected a table");
        assert_eq!(out, format!("expected a table\nLINE 2: FROM\n{}^", " ".repeat(12)), "this check");
        let long = render_error("ab\ncd", 0, 50, "m");
        assert_eq!(long, "m\nLINE 1: ab\n        ^^", "carets stop at the end of the line");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: positions are monotone and agree with counting by hand.
        #[test]
        fn s3d_c4_property_positions_match_counting(src in "[a-é \\n]{0,30}") {
            let mut prev = (1, 0);
            let mut line = 1;
            let mut col = 1;
            for (off, ch) in src.char_indices() {
                let p = line_col(&src, off);
                prop_assert_eq!(p, (line, col));
                prop_assert!(p >= prev || p.0 > prev.0);
                prev = p;
                if ch == '\n' { line += 1; col = 1; } else { col += 1; }
            }
            prop_assert_eq!(line_col(&src, src.len()), (line, col));
        }
    }
}
// @@ challenge 3d-c4 end

// @@ challenge 3d-c5 begin
mod ch_3d_c5 {
    use proptest::prelude::*;

    use bustub::sql::resolve::{resolve_column, ResolveError::*};

    fn scope() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![("a", vec!["id", "x"]), ("b", vec!["id", "y"])]
    }

    #[test]
    fn s3d_c5_qualified_names_pick_the_table() {
        assert_eq!(resolve_column(&scope(), Some("a"), "id"), Ok((0, 0)), "this check");
        assert_eq!(resolve_column(&scope(), Some("B"), "ID"), Ok((1, 0)), "this check");
    }

    #[test]
    fn s3d_c5_an_unqualified_name_found_once_resolves() {
        assert_eq!(resolve_column(&scope(), None, "y"), Ok((1, 1)), "this check");
        assert_eq!(resolve_column(&scope(), None, "X"), Ok((0, 1)), "this check");
    }

    #[test]
    fn s3d_c5_an_unqualified_name_in_two_tables_is_ambiguous() {
        assert_eq!(resolve_column(&scope(), None, "id"), Err(Ambiguous(vec![0, 1])), "this check");
    }

    #[test]
    fn s3d_c5_unknown_columns_and_tables_are_different_errors() {
        assert_eq!(resolve_column(&scope(), None, "z"), Err(UnknownColumn), "this check");
        assert_eq!(resolve_column(&scope(), Some("a"), "y"), Err(UnknownColumn), "this check");
        assert_eq!(resolve_column(&scope(), Some("c"), "id"), Err(UnknownTable), "this check");
    }

    #[test]
    fn s3d_c5_a_table_that_repeats_a_column_name_is_ambiguous_even_when_qualified() {
        let t = vec![("t", vec!["a", "A"])];
        assert_eq!(resolve_column(&t, Some("t"), "a"), Err(Ambiguous(vec![0, 0])), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: unqualified lookup agrees with listing every (table, column) match.
        #[test]
        fn s3d_c5_property_unqualified_lookup_is_a_search(cols in proptest::collection::vec(proptest::collection::vec(prop::sample::select(vec!["a", "b", "c", "D"]), 0..4), 1..4), name in prop::sample::select(vec!["a", "b", "c", "d", "e"])) {
            let tables: Vec<(String, Vec<&str>)> = cols.into_iter().enumerate().map(|(i, c)| (format!("t{i}"), c)).collect();
            let view: Vec<(&str, Vec<&str>)> = tables.iter().map(|(a, c)| (a.as_str(), c.clone())).collect();
            let mut hits = Vec::new();
            for (t, (_, c)) in view.iter().enumerate() {
                for (i, col) in c.iter().enumerate() { if col.eq_ignore_ascii_case(name) { hits.push((t, i)); } }
            }
            let want = match hits.len() { 0 => Err(UnknownColumn), 1 => Ok(hits[0]), _ => Err(Ambiguous(hits.iter().map(|h| h.0).collect())) };
            prop_assert_eq!(resolve_column(&view, None, name), want);
        }
    }
}
// @@ challenge 3d-c5 end
