//! Tests for module 3f: aggregation and joins.

mod common;
mod slt;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::exception::ExceptionType;
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::execution::executors::aggregation_executor::{AggregateKey, AggregateValue, SimpleAggregationHashTable};
use bustub::execution::execution_engine::ExecutionEngine;
use bustub::execution::executor_context::ExecutorContext;
use bustub::execution::expressions::abstract_expression::ExprRef;
use bustub::execution::expressions::column_value_expression::ColumnValueExpression;
use bustub::execution::plans::plan_node::{AggregationType, JoinType, PlanKind, PlanNode, PlanRef};
use bustub::types::type_id::TypeId;
use bustub::types::value::Value;
#[path = "common/sql_model.rs"]
mod model;
use proptest::prelude::*;

fn int(v: i32) -> Value {
    Value::integer(v)
}

fn null() -> Value {
    Value::null(TypeId::Integer)
}

fn new_db() -> BusTubInstance {
    BusTubInstance::new(128)
}

fn sql(db: &BusTubInstance, sql: &str) -> Vec<String> {
    let mut out = String::new();
    db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None).unwrap_or_else(|e| panic!("{sql}: {e}"));
    out.lines().map(|l| l.trim_end().to_string()).collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn sql_err(db: &BusTubInstance, sql: &str) -> bool {
    let mut out = String::new();
    db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None).is_err()
}

fn insert_rows(db: &BusTubInstance, table: &str, rows: &[&str]) {
    if !rows.is_empty() {
        sql(db, &format!("insert into {table} values {}", rows.join(", ")));
    }
}

fn key(vs: Vec<Value>) -> AggregateKey {
    AggregateKey { group_bys: vs }
}

fn hash_of(k: &AggregateKey) -> u64 {
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    h.finish()
}

// ---- 3f-01 · group keys ---------------------------------------------------------------------------------------------------------

#[test]
fn s3f_01_equal_keys_are_equal_and_hash_alike() {
    let a = key(vec![int(1), Value::varchar("x")]);
    let b = key(vec![int(1), Value::varchar("x")]);
    assert_eq!(a, b, "equal keys are equal and hash alike");
    assert_eq!(hash_of(&a), hash_of(&b), "equal keys are equal and hash alike");
    assert_ne!(a, key(vec![int(1), Value::varchar("y")]), "equal keys are equal and hash alike");
    assert_ne!(a, key(vec![int(2), Value::varchar("x")]), "equal keys are equal and hash alike");
}

#[test]
fn s3f_01_two_nulls_are_the_same_group() {
    assert_eq!(key(vec![null()]), key(vec![null()]), "two nulls are the same group");
    assert_eq!(hash_of(&key(vec![null()])), hash_of(&key(vec![null()])), "two nulls are the same group");
    assert_eq!(key(vec![null()]), key(vec![Value::null(TypeId::Varchar)]), "a NULL is a NULL whatever its type");
    assert_ne!(key(vec![null()]), key(vec![int(0)]), "NULL is not zero");
}

#[test]
fn s3f_01_equal_integers_of_different_widths_are_the_same_group() {
    assert_eq!(key(vec![Value::tinyint(5)]), key(vec![Value::bigint(5)]), "equal integers of different widths are the same group");
    assert_eq!(hash_of(&key(vec![Value::tinyint(5)])), hash_of(&key(vec![Value::bigint(5)])), "equal integers of different widths are the same group");
    assert_ne!(key(vec![Value::smallint(5)]), key(vec![Value::smallint(6)]), "equal integers of different widths are the same group");
}

#[test]
fn s3f_01_the_number_of_values_matters() {
    assert_ne!(key(vec![int(1)]), key(vec![int(1), int(1)]), "the number of values matters");
    assert_ne!(key(vec![]), key(vec![null()]), "the number of values matters");
    assert_eq!(key(vec![]), key(vec![]), "the key of 'no GROUP BY' is the empty key");
}

#[test]
fn s3f_01_decimal_zero_and_negative_zero_are_one_group() {
    assert_eq!(key(vec![Value::decimal(0.0)]), key(vec![Value::decimal(-0.0)]), "decimal zero and negative zero are one group");
    assert_eq!(hash_of(&key(vec![Value::decimal(0.0)])), hash_of(&key(vec![Value::decimal(-0.0)])), "decimal zero and negative zero are one group");
    assert_ne!(key(vec![Value::decimal(1.5)]), key(vec![Value::decimal(2.5)]), "decimal zero and negative zero are one group");
}

#[test]
fn s3f_01_keys_work_in_a_hash_map() {
    let mut counts: HashMap<AggregateKey, u32> = HashMap::new();
    for v in [int(1), null(), int(1), null(), int(2), Value::bigint(1)] {
        *counts.entry(key(vec![v])).or_insert(0) += 1;
    }
    assert_eq!(counts.len(), 3, "keys work in a hash map");
    assert_eq!(counts[&key(vec![int(1)])], 3, "keys work in a hash map");
    assert_eq!(counts[&key(vec![null()])], 2, "keys work in a hash map");
}

// ---- 3f-01 · combining aggregate values ----------------------------------------------------------------------------------------

fn table(types: &[AggregationType]) -> SimpleAggregationHashTable {
    SimpleAggregationHashTable::new(types.to_vec())
}

fn input(vs: Vec<Value>) -> AggregateValue {
    AggregateValue { aggregates: vs }
}

/// Folds `inputs` (one value per aggregate in each) and returns the final values.
fn fold(types: &[AggregationType], inputs: &[Vec<Value>]) -> Vec<Value> {
    let t = table(types);
    let mut running = t.generate_initial_aggregate_value();
    for i in inputs {
        t.combine_aggregate_values(&mut running, &input(i.clone())).unwrap();
    }
    running.aggregates
}

use AggregationType::*;

#[test]
fn s3f_01_the_initial_values() {
    let t = table(&[CountStarAggregate, CountAggregate, SumAggregate, MinAggregate, MaxAggregate]);
    assert_eq!(t.generate_initial_aggregate_value().aggregates, vec![int(0), null(), null(), null(), null()], "the initial values");
}

#[test]
fn s3f_01_each_aggregate_over_a_few_values() {
    let all = [CountStarAggregate, CountAggregate, SumAggregate, MinAggregate, MaxAggregate];
    let rows: Vec<Vec<Value>> = [5, -2, 9].iter().map(|v| vec![int(1), int(*v), int(*v), int(*v), int(*v)]).collect();
    assert_eq!(fold(&all, &rows), vec![int(3), int(3), int(12), int(-2), int(9)], "each aggregate over a few values");
}

#[test]
fn s3f_01_null_inputs_are_ignored_except_by_count_star() {
    let all = [CountStarAggregate, CountAggregate, SumAggregate, MinAggregate, MaxAggregate];
    let rows = vec![
        vec![int(1), null(), null(), null(), null()],
        vec![int(1), int(4), int(4), int(4), int(4)],
        vec![int(1), null(), null(), null(), null()],
    ];
    assert_eq!(fold(&all, &rows), vec![int(3), int(1), int(4), int(4), int(4)], "null inputs are ignored except by count star");
}

#[test]
fn s3f_01_a_group_of_only_nulls_stays_null() {
    let all = [CountAggregate, SumAggregate, MinAggregate, MaxAggregate];
    let rows = vec![vec![null(); 4], vec![null(); 4]];
    assert_eq!(fold(&all, &rows), vec![null(); 4], "nothing non-NULL was seen: not 0");
}

#[test]
fn s3f_01_the_first_value_seeds_the_running_value() {
    assert_eq!(fold(&[MinAggregate], &[vec![int(7)]]), vec![int(7)], "the first value seeds the running value");
    assert_eq!(fold(&[MaxAggregate], &[vec![int(-7)]]), vec![int(-7)], "max of one negative number is that number, not 0");
    assert_eq!(fold(&[SumAggregate], &[vec![int(-7)]]), vec![int(-7)], "the first value seeds the running value");
}

#[test]
fn s3f_01_sum_overflow_is_an_error() {
    let t = table(&[SumAggregate]);
    let mut running = t.generate_initial_aggregate_value();
    t.combine_aggregate_values(&mut running, &input(vec![int(i32::MAX)])).unwrap();
    let e = t.combine_aggregate_values(&mut running, &input(vec![int(1)])).unwrap_err();
    assert_eq!(e.kind, ExceptionType::OutOfRange, "sum overflow is an error");
}

#[test]
fn s3f_01_insert_combine_keeps_one_running_value_per_group() {
    let mut t = table(&[CountStarAggregate, SumAggregate]);
    for (k, v) in [(1, 10), (2, 20), (1, 5), (2, 1), (1, 1)] {
        t.insert_combine(key(vec![int(k)]), &input(vec![int(1), int(v)])).unwrap();
    }
    let mut groups: Vec<(i64, Vec<Value>)> = t.entries().map(|(k, v)| (k.group_bys[0].as_i64().unwrap(), v.aggregates.clone())).collect();
    groups.sort_by_key(|(k, _)| *k);
    assert_eq!(groups, vec![(1, vec![int(3), int(16)]), (2, vec![int(2), int(21)])], "insert combine keeps one running value per group");
}

// ---- 3f-02 · the aggregation executor --------------------------------------------------------------------------------------------

fn agg_db() -> BusTubInstance {
    let db = new_db();
    sql(&db, "create table t(g int, h int, v int)");
    insert_rows(&db, "t", &["(1, 1, 10)", "(1, 2, 20)", "(2, 1, 30)", "(2, 1, null)", "(null, 1, 40)", "(null, 2, 50)"]);
    db
}

#[test]
fn s3f_02_group_by_one_column() {
    let db = agg_db();
    assert_eq!(
        sorted(sql(&db, "select g, count(*), sum(v), min(v), max(v), count(v) from t group by g")),
        vec!["1 2 30 10 20 2", "2 2 30 30 30 1", "integer_null 2 90 40 50 2"], "group by one column"
    );
}

#[test]
fn s3f_02_group_by_several_columns_and_expressions() {
    let db = agg_db();
    assert_eq!(
        sorted(sql(&db, "select g, h, count(*) from t group by g, h")),
        vec!["1 1 1", "1 2 1", "2 1 2", "integer_null 1 1", "integer_null 2 1"], "group by several columns and expressions"
    );
    assert_eq!(sorted(sql(&db, "select h + 100, sum(v) from t group by h")), vec!["101 80", "102 70"], "group by several columns and expressions");
}

#[test]
fn s3f_02_aggregates_without_group_by_are_one_row() {
    let db = agg_db();
    assert_eq!(sql(&db, "select count(*), count(v), sum(v), min(v), max(v) from t"), vec!["6 5 150 10 50"], "aggregates without group by are one row");
    assert_eq!(sql(&db, "select sum(v + 1), count(*) + 0 from t").len(), 1, "aggregates without group by are one row");
}

#[test]
fn s3f_02_an_empty_input_has_one_group_without_group_by_and_none_with_it() {
    let db = new_db();
    sql(&db, "create table e(g int, v int)");
    assert_eq!(sql(&db, "select count(*), count(v), sum(v), min(v), max(v) from e"), vec!["0 integer_null integer_null integer_null integer_null"], "an empty input has one group without group by and none with it");
    assert!(sql(&db, "select g, count(*) from e group by g").is_empty(), "an empty input has one group without group by and none with it: expected `sql(&db, \"select g, count(*) from e group by g\").is_empty()`");
}

#[test]
fn s3f_02_having_filters_the_groups() {
    let db = agg_db();
    assert_eq!(sorted(sql(&db, "select g, count(*) from t group by g having count(*) >= 2 and sum(v) > 50")), vec!["integer_null 2"], "having filters the groups");
    assert_eq!(sql(&db, "select g from t group by g having sum(v) < 0").len(), 0, "having filters the groups");
}

#[test]
fn s3f_02_more_groups_than_a_batch() {
    let db = new_db();
    sql(&db, "create table big(a int)");
    let rows: Vec<String> = (0..500).map(|i| format!("({})", i % 100)).collect();
    insert_rows(&db, "big", &rows.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let out = sql(&db, "select a, count(*) from big group by a");
    assert_eq!(out.len(), 100, "more groups than a batch");
    assert!(out.iter().all(|l| l.ends_with(" 5")), "more groups than a batch: expected `out.iter().all(|l| l.ends_with(\" 5\"))`");
}

#[test]
fn s3f_02_select_distinct_is_a_group_by_without_aggregates() {
    let db = agg_db();
    assert_eq!(sorted(sql(&db, "select distinct g from t")), vec!["1", "2", "integer_null"], "select distinct is a group by without aggregates");
    assert_eq!(sql(&db, "select distinct h from t").len(), 2, "select distinct is a group by without aggregates");
}

#[test]
fn s3f_02_init_can_be_repeated() {
    let db = agg_db();
    let first = sorted(sql(&db, "select g, sum(v) from t group by g"));
    let second = sorted(sql(&db, "select g, sum(v) from t group by g"));
    assert_eq!(first, second, "init can be repeated");
    let out = sql(&db, "select * from (select g, sum(v) from t group by g)");
    assert_eq!(out.len(), 3, "an aggregation as the input of another operator");
}

// ---- 3f-03 · nested loop join --------------------------------------------------------------------------------------------------

fn join_db() -> BusTubInstance {
    let db = new_db();
    sql(&db, "create table a(x int, s varchar(10))");
    sql(&db, "create table b(y int, z int)");
    insert_rows(&db, "a", &["(1, 'one')", "(2, 'two')", "(3, 'three')", "(null, 'nil')"]);
    insert_rows(&db, "b", &["(1, 100)", "(1, 101)", "(3, 300)", "(4, 400)", "(null, 0)"]);
    db
}

#[test]
fn s3f_03_an_inner_join_outputs_the_matching_pairs() {
    let db = join_db();
    assert_eq!(
        sorted(sql(&db, "select * from a inner join b on a.x = b.y")),
        vec!["1 one 1 100", "1 one 1 101", "3 three 3 300"], "an inner join outputs the matching pairs"
    );
    assert_eq!(sql(&db, "select * from a join b on a.x = b.y and b.z > 200"), vec!["3 three 3 300"], "an inner join outputs the matching pairs");
}

#[test]
fn s3f_03_the_predicate_can_be_anything_not_only_equality() {
    let db = join_db();
    let lt = sql(&db, "select a.x, b.y from a inner join b on a.x < b.y");
    assert_eq!(sorted(lt), vec!["1 3", "1 4", "2 3", "2 4", "3 4"], "the predicate can be anything not only equality");
    let arithmetic = sql(&db, "select * from a inner join b on a.x + 1 = b.y");
    assert_eq!(sorted(arithmetic), vec!["2 two 3 300", "3 three 4 400"], "the predicate can be anything not only equality");
}

#[test]
fn s3f_03_null_never_matches() {
    let db = join_db();
    let rows = sql(&db, "select * from a inner join b on a.x = b.y");
    assert!(rows.iter().all(|r| !r.contains("nil")), "NULL = NULL is not true");
}

#[test]
fn s3f_03_a_cross_join_has_every_pair() {
    let db = join_db();
    assert_eq!(sql(&db, "select * from a, b").len(), 20, "a cross join has every pair");
    assert_eq!(sql(&db, "select * from a, b where a.x = b.y").len(), 3, "a cross join has every pair");
}

#[test]
fn s3f_03_one_left_row_with_more_matches_than_a_batch() {
    let db = new_db();
    sql(&db, "create table l(x int)");
    sql(&db, "create table r(y int)");
    insert_rows(&db, "l", &["(1)", "(2)"]);
    let rows: Vec<String> = (0..70).map(|_| "(1)".to_string()).collect();
    insert_rows(&db, "r", &rows.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    assert_eq!(sql(&db, "select * from l join r on l.x = r.y").len(), 70, "one left row with more matches than a batch");
}

#[test]
fn s3f_03_an_empty_side_gives_no_rows() {
    let db = join_db();
    sql(&db, "create table empty(q int)");
    assert!(sql(&db, "select * from a join empty on a.x = empty.q").is_empty(), "an empty side gives no rows: expected `sql(&db, \"select * from a join empty on a.x = empty.q\").is_empty()`");
    assert!(sql(&db, "select * from empty join a on a.x = empty.q").is_empty(), "an empty side gives no rows: expected `sql(&db, \"select * from empty join a on a.x = empty.q\").is_empty()`");
}

#[test]
fn s3f_03_three_tables_and_a_table_joined_with_itself() {
    let db = join_db();
    sql(&db, "create table c(w int)");
    insert_rows(&db, "c", &["(1)", "(3)"]);
    assert_eq!(sorted(sql(&db, "select a.x, b.z, c.w from a join b on a.x = b.y join c on c.w = a.x")), vec!["1 100 1", "1 101 1", "3 300 3"], "three tables and a table joined with itself");
    assert_eq!(sorted(sql(&db, "select p.x, q.x from a p join a q on p.x = q.x")), vec!["1 1", "2 2", "3 3"], "three tables and a table joined with itself");
}

#[test]
fn s3f_03_the_right_side_is_initialised_again_for_each_left_tuple() {
    let db = join_db();
    // the starter rules have no hash join (module 3h adds one), so the equality join stays a nested loop
    let script = "
statement ok
set force_optimizer_starter_rule=yes

query rowsort +ensure:nlj_init_check
select * from a inner join b on a.x = b.y;
----
1 one 1 100
1 one 1 101
3 three 3 300
";
    slt::run_script(&db, "init-check", script).unwrap_or_else(|e| panic!("{e}"));
}

// ---- 3f-03 · left join ----------------------------------------------------------------------------------------------------------

#[test]
fn s3f_03_a_left_join_keeps_unmatched_left_rows_with_nulls() {
    let db = join_db();
    assert_eq!(
        sorted(sql(&db, "select * from a left join b on a.x = b.y")),
        vec!["1 one 1 100", "1 one 1 101", "2 two integer_null integer_null", "3 three 3 300", "integer_null nil integer_null integer_null"], "a left join keeps unmatched left rows with nulls"
    );
}

#[test]
fn s3f_03_the_padding_has_the_type_of_the_right_columns() {
    let db = join_db();
    sql(&db, "create table r2(k int, name varchar(10))");
    insert_rows(&db, "r2", &["(1, 'x')"]);
    assert_eq!(sorted(sql(&db, "select * from a left join r2 on a.x = r2.k")), vec!["1 one 1 x", "2 two integer_null varlen_null", "3 three integer_null varlen_null", "integer_null nil integer_null varlen_null"], "the padding has the type of the right columns");
}

#[test]
fn s3f_03_a_left_row_is_unmatched_if_no_pair_is_true_even_with_null_answers() {
    let db = join_db();
    // a.x < b.y is NULL for the NULL rows: they are unmatched, not errors
    let rows = sql(&db, "select a.s, b.y from a left join b on a.x < 2 and a.x < b.y");
    let sorted_rows = sorted(rows);
    assert_eq!(sorted_rows, vec!["nil integer_null", "one 3", "one 4", "three integer_null", "two integer_null"], "a left row is unmatched if no pair is true even with null answers");
}

#[test]
fn s3f_03_with_an_empty_right_side_every_left_row_is_unmatched() {
    let db = join_db();
    sql(&db, "create table empty(q int, w int)");
    assert_eq!(sql(&db, "select * from a left join empty on a.x = empty.q").len(), 4, "with an empty right side every left row is unmatched");
    assert!(sql(&db, "select * from empty left join a on a.x = empty.q").is_empty(), "with an empty right side every left row is unmatched: expected `sql(&db, \"select * from empty left join a on a.x = empty.q\").is_empty()`");
}

#[test]
fn s3f_03_a_left_row_that_matches_many_appears_for_each_and_never_unmatched() {
    let db = join_db();
    let rows = sql(&db, "select a.x, b.z from a left join b on a.x = b.y where a.x = 1");
    assert_eq!(sorted(rows), vec!["1 100", "1 101"], "a left row that matches many appears for each and never unmatched");
}

#[test]
fn s3f_03_left_joins_chained_and_with_a_filter_on_the_result() {
    let db = join_db();
    sql(&db, "create table c(w int, label varchar(5))");
    insert_rows(&db, "c", &["(100, 'hundred')"]);
    let rows = sql(&db, "select a.x, c.label from (a left join b on a.x = b.y) left join c on b.z = c.w where a.x = 1");
    assert_eq!(sorted(rows), vec!["1 hundred", "1 varlen_null"], "left joins chained and with a filter on the result");
}

#[test]
fn s3f_03_the_left_join_passes_the_init_check_too() {
    let db = join_db();
    // the starter rules have no hash join (module 3h adds one), so the equality join stays a nested loop
    let script = "
statement ok
set force_optimizer_starter_rule=yes

query rowsort +ensure:nlj_init_check
select * from a left join b on a.x = b.y;
----
1 one 1 100
1 one 1 101
2 two integer_null integer_null
3 three 3 300
integer_null nil integer_null integer_null
";
    slt::run_script(&db, "init-check-left", script).unwrap_or_else(|e| panic!("{e}"));
}

// ---- 3f-04 · hash join: inner ---------------------------------------------------------------------------------------------------

fn scan(db: &BusTubInstance, name: &str) -> PlanRef {
    let catalog = db.catalog.read().unwrap();
    let info = catalog.get_table(name).unwrap();
    let schema = Schema::new(info.schema.columns().iter().map(|c| c.with_column_name(&format!("{name}.{}", c.name()))).collect());
    PlanNode::new(Arc::new(schema), vec![], PlanKind::SeqScan { table_oid: info.oid, table_name: name.to_string(), filter_predicate: None })
}

fn col(idx: u32, type_id: TypeId) -> ExprRef {
    Arc::new(ColumnValueExpression::new(0, idx, Column::new("c", type_id)))
}

/// A hash join plan of two tables on `left_keys[i] = right_keys[i]`; the output has all columns of both.
fn hash_join_plan(db: &BusTubInstance, l: &str, r: &str, left_keys: Vec<ExprRef>, right_keys: Vec<ExprRef>, join_type: JoinType) -> PlanRef {
    let (lp, rp) = (scan(db, l), scan(db, r));
    let mut cols: Vec<Column> = lp.output_schema.columns().to_vec();
    cols.extend(rp.output_schema.columns().iter().cloned());
    PlanNode::new(
        Arc::new(Schema::new(cols)),
        vec![lp, rp],
        PlanKind::HashJoin { left_key_expressions: left_keys, right_key_expressions: right_keys, join_type },
    )
}

fn run_plan(db: &BusTubInstance, plan: &PlanRef) -> Vec<String> {
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let (ok, tuples) = ExecutionEngine::execute(plan, &ctx).unwrap();
    assert!(ok, "in helper `run_plan`: expected `ok`");
    let schema = &plan.output_schema;
    tuples.iter().map(|t| (0..schema.column_count()).map(|i| t.get_value(schema, i).to_string()).collect::<Vec<_>>().join(" ")).collect()
}

#[test]
fn s3f_04_matching_pairs_by_hashing() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Inner);
    assert_eq!(sorted(run_plan(&db, &plan)), vec!["1 one 1 100", "1 one 1 101", "3 three 3 300"], "matching pairs by hashing");
}

#[test]
fn s3f_04_it_gives_the_same_rows_as_the_nested_loop_join() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Inner);
    assert_eq!(sorted(run_plan(&db, &plan)), sorted(sql(&db, "select * from a join b on a.x = b.y")), "it gives the same rows as the nested loop join");
}

#[test]
fn s3f_04_null_keys_match_nothing_not_even_each_other() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Inner);
    let rows = run_plan(&db, &plan);
    assert!(rows.iter().all(|r| !r.contains("nil") && !r.contains("integer_null")), "{rows:?}");
}

#[test]
fn s3f_04_duplicates_on_both_sides_give_every_pair() {
    let db = new_db();
    sql(&db, "create table l(x int, tag int)");
    sql(&db, "create table r(y int, tag int)");
    insert_rows(&db, "l", &["(1, 1)", "(1, 2)", "(2, 3)"]);
    insert_rows(&db, "r", &["(1, 10)", "(1, 20)", "(1, 30)", "(3, 40)"]);
    let plan = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Inner);
    assert_eq!(run_plan(&db, &plan).len(), 6, "2 left rows × 3 right rows with key 1");
}

#[test]
fn s3f_04_several_key_columns_must_all_match() {
    let db = new_db();
    sql(&db, "create table l(a int, b int)");
    sql(&db, "create table r(c int, d int)");
    insert_rows(&db, "l", &["(1, 1)", "(1, 2)", "(2, 2)"]);
    insert_rows(&db, "r", &["(1, 2)", "(2, 2)", "(2, 1)"]);
    let plan = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer), col(1, TypeId::Integer)], vec![col(0, TypeId::Integer), col(1, TypeId::Integer)], JoinType::Inner);
    assert_eq!(sorted(run_plan(&db, &plan)), vec!["1 2 1 2", "2 2 2 2"], "several key columns must all match");
    let swapped = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer), col(1, TypeId::Integer)], vec![col(1, TypeId::Integer), col(0, TypeId::Integer)], JoinType::Inner);
    assert_eq!(sorted(run_plan(&db, &swapped)), vec!["1 2 2 1", "2 2 2 2"], "keys are compared position by position: l.a = r.d and l.b = r.c");
}

#[test]
fn s3f_04_an_empty_side_and_a_large_join() {
    let db = new_db();
    sql(&db, "create table l(x int)");
    sql(&db, "create table r(y int)");
    let plan = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Inner);
    assert!(run_plan(&db, &plan).is_empty(), "an empty side and a large join: expected `run_plan(&db, &plan).is_empty()`");
    let rows: Vec<String> = (0..2000).map(|i| format!("({i})")).collect();
    let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
    insert_rows(&db, "l", &refs);
    insert_rows(&db, "r", &refs[500..1500]);
    assert_eq!(run_plan(&db, &plan).len(), 1000, "an empty side and a large join");
}

#[test]
fn s3f_04_the_join_can_be_run_twice() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Inner);
    assert_eq!(sorted(run_plan(&db, &plan)), sorted(run_plan(&db, &plan)), "the join can be run twice");
}

// ---- 3f-04 · hash join: left ----------------------------------------------------------------------------------------------------

#[test]
fn s3f_04_unmatched_left_rows_are_padded_with_nulls() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Left);
    assert_eq!(
        sorted(run_plan(&db, &plan)),
        vec!["1 one 1 100", "1 one 1 101", "2 two integer_null integer_null", "3 three 3 300", "integer_null nil integer_null integer_null"], "unmatched left rows are padded with nulls"
    );
}

#[test]
fn s3f_04_a_null_key_row_is_unmatched_but_still_output() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Left);
    let rows = run_plan(&db, &plan);
    assert!(rows.contains(&"integer_null nil integer_null integer_null".to_string()), "a null key row is unmatched but still output: expected `rows.contains(&\"integer_null nil integer_null integer_null\".to_string())`");
    assert_eq!(rows.iter().filter(|r| r.contains("nil")).count(), 1, "a null key row is unmatched but still output");
}

#[test]
fn s3f_04_it_gives_the_same_rows_as_the_nested_loop_left_join() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Left);
    assert_eq!(sorted(run_plan(&db, &plan)), sorted(sql(&db, "select * from a left join b on a.x = b.y")), "it gives the same rows as the nested loop left join");
}

#[test]
fn s3f_04_padding_columns_have_the_right_types() {
    let db = new_db();
    sql(&db, "create table l(x int)");
    sql(&db, "create table r(y int, name varchar(10))");
    insert_rows(&db, "l", &["(5)"]);
    let plan = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Left);
    assert_eq!(run_plan(&db, &plan), vec!["5 integer_null varlen_null"], "padding columns have the right types");
}

#[test]
fn s3f_04_with_an_empty_right_side_every_left_row_is_output() {
    let db = new_db();
    sql(&db, "create table l(x int)");
    sql(&db, "create table r(y int)");
    let rows: Vec<String> = (0..100).map(|i| format!("({i})")).collect();
    insert_rows(&db, "l", &rows.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let plan = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Left);
    assert_eq!(run_plan(&db, &plan).len(), 100, "with an empty right side every left row is output");
}

#[test]
fn s3f_04_only_inner_and_left_joins_are_supported() {
    let db = join_db();
    let plan = hash_join_plan(&db, "a", "b", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], JoinType::Right);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = ExecutionEngine::execute(&plan, &ctx).unwrap_err();
    assert_eq!(e.kind, ExceptionType::NotImplemented, "only inner and left joins are supported");
}

// ---- 3f-05 · nested index join --------------------------------------------------------------------------------------------------

fn index_join_db() -> BusTubInstance {
    let db = new_db();
    sql(&db, "create table outer_t(x int, s varchar(10))");
    sql(&db, "create table inner_t(y int, z int)");
    insert_rows(&db, "outer_t", &["(1, 'one')", "(2, 'two')", "(3, 'three')", "(null, 'nil')"]);
    insert_rows(&db, "inner_t", &["(1, 100)", "(3, 300)", "(4, 400)"]);
    sql(&db, "create index inner_y on inner_t(y)");
    sql(&db, "set force_optimizer_starter_rule=yes");
    db
}

#[test]
fn s3f_05_the_optimizer_turns_an_equality_join_on_an_indexed_column_into_an_index_join() {
    let db = index_join_db();
    let plan = sql(&db, "explain (o) select * from outer_t join inner_t on outer_t.x = inner_t.y").join("\n");
    assert!(plan.contains("NestedIndexJoin"), "{plan}");
}

#[test]
fn s3f_05_inner_join_through_the_index() {
    let db = index_join_db();
    assert_eq!(sorted(sql(&db, "select * from outer_t join inner_t on outer_t.x = inner_t.y")), vec!["1 one 1 100", "3 three 3 300"], "inner join through the index");
    assert_eq!(sorted(sql(&db, "select * from outer_t join inner_t on inner_t.y = outer_t.x")), vec!["1 one 1 100", "3 three 3 300"], "either side of the = may be written first");
}

#[test]
fn s3f_05_left_join_pads_the_unmatched_outer_rows() {
    let db = index_join_db();
    assert_eq!(
        sorted(sql(&db, "select * from outer_t left join inner_t on outer_t.x = inner_t.y")),
        vec!["1 one 1 100", "2 two integer_null integer_null", "3 three 3 300", "integer_null nil integer_null integer_null"], "left join pads the unmatched outer rows"
    );
}

#[test]
fn s3f_05_a_deleted_inner_row_is_not_joined() {
    let db = index_join_db();
    sql(&db, "delete from inner_t where y = 3");
    assert_eq!(sql(&db, "select * from outer_t join inner_t on outer_t.x = inner_t.y"), vec!["1 one 1 100"], "a deleted inner row is not joined");
    assert_eq!(sql(&db, "select * from outer_t left join inner_t on outer_t.x = inner_t.y where outer_t.x = 3"), vec!["3 three integer_null integer_null"], "a deleted inner row is not joined");
}

#[test]
fn s3f_05_rows_inserted_after_the_index_was_made_are_found() {
    let db = index_join_db();
    sql(&db, "insert into inner_t values (2, 200)");
    assert_eq!(sorted(sql(&db, "select * from outer_t join inner_t on outer_t.x = inner_t.y")), vec!["1 one 1 100", "2 two 2 200", "3 three 3 300"], "rows inserted after the index was made are found");
}

#[test]
fn s3f_05_it_agrees_with_the_nested_loop_join_on_a_bigger_table() {
    let db = index_join_db();
    sql(&db, "create table o2(x int)");
    sql(&db, "create table i2(y int, z int)");
    let outer: Vec<String> = (0..300).map(|i| format!("({})", i % 120)).collect();
    let inner: Vec<String> = (0..100).map(|i| format!("({i}, {})", i * 10)).collect();
    insert_rows(&db, "o2", &outer.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    insert_rows(&db, "i2", &inner.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    sql(&db, "create index i2y on i2(y)");
    let with_index = sorted(sql(&db, "select * from o2 left join i2 on o2.x = i2.y"));
    sql(&db, "set force_optimizer_starter_rule=no");
    let without = sorted(sql(&db, "select * from o2 left join i2 on o2.x = i2.y"));
    assert_eq!(with_index, without, "it agrees with the nested loop join on a bigger table");
    assert_eq!(with_index.len(), 300, "it agrees with the nested loop join on a bigger table");
}

#[test]
fn s3f_05_without_an_index_the_join_stays_a_nested_loop() {
    let db = index_join_db();
    let plan = sql(&db, "explain (o) select * from inner_t join outer_t on inner_t.z = outer_t.x").join("\n");
    assert!(!plan.contains("NestedIndexJoin"), "{plan}");
    assert!(sql_err(&db, "select * from inner_t join nosuch on inner_t.z = nosuch.x"), "without an index the join stays a nested loop: expected `sql_err(&db, \"select * from inner_t join nosuch on inner_t.z = nosuch.x\")`");
}

// ---- properties: aggregation and joins against plain Rust ------------------------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 40, max_shrink_iters: 1000, failure_persistence: None, ..ProptestConfig::default() }
}

/// A grouping value: NULLs of several types, integers that fit every width, decimals (zero has two spellings), text and booleans.
#[derive(Clone, Debug)]
enum K {
    Null(u8),
    Int(i8),
    Dec(u8),
    Str(u8),
    Bool(bool),
}

fn k_strategy() -> impl Strategy<Value = (K, u8)> {
    let k = prop_oneof![
        2 => (0u8..4).prop_map(K::Null),
        4 => (-3i8..=3).prop_map(K::Int),
        2 => (0u8..5).prop_map(K::Dec),
        2 => (0u8..3).prop_map(K::Str),
        1 => any::<bool>().prop_map(K::Bool),
    ];
    (k, 0u8..4)
}

fn k_value((k, width): &(K, u8)) -> Value {
    match k {
        K::Null(t) => Value::null([TypeId::Integer, TypeId::Varchar, TypeId::Boolean, TypeId::Decimal][*t as usize]),
        K::Int(i) => match width {
            0 => Value::tinyint(*i),
            1 => Value::smallint(*i as i16),
            2 => Value::integer(*i as i32),
            _ => Value::bigint(*i as i64),
        },
        K::Dec(d) => Value::decimal([0.0, -0.0, 1.5, -1.5, 2.5][*d as usize]),
        K::Str(s) => Value::varchar(["", "a", "B"][*s as usize]),
        K::Bool(b) => Value::boolean(*b),
    }
}

/// What a grouping value *means*: every NULL is one value, integers compare by value whatever their width, and `0.0` is `-0.0`.
fn norm((k, _): &(K, u8)) -> String {
    match k {
        K::Null(_) => "null".into(),
        K::Int(i) => format!("int {i}"),
        K::Dec(d) => format!("dec {}", if *d <= 1 { 0 } else { *d }),
        K::Str(s) => format!("str {s}"),
        K::Bool(b) => format!("bool {b}"),
    }
}

fn opt_value(v: Option<i32>) -> Value {
    v.map_or_else(null, int)
}

fn cell_text(v: Option<i64>) -> String {
    v.map_or("integer_null".to_string(), |v| v.to_string())
}

proptest! {
    #![proptest_config(pconfig())]

    /// Two keys are equal exactly when their values mean the same (NULLs together, integers by value, one zero), equal keys hash
    /// alike (the contract `HashMap` relies on), and the key `Eq` is an equivalence.
    #[test]
    fn s3f_01_keys_are_equal_exactly_when_their_values_mean_the_same(a in prop::collection::vec(k_strategy(), 0..3), b in prop::collection::vec(k_strategy(), 0..3)) {
        let (ka, kb) = (key(a.iter().map(k_value).collect()), key(b.iter().map(k_value).collect()));
        let same = a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| norm(x) == norm(y));
        prop_assert_eq!(ka == kb, same, "{:?} vs {:?}", a, b);
        prop_assert_eq!(kb == ka, same, "equality is symmetric");
        prop_assert!(ka == ka.clone(), "equality is reflexive");
        if same { prop_assert_eq!(hash_of(&ka), hash_of(&kb), "equal keys hash alike"); }
    }

    /// Folding a stream of (group, value) rows into the hash table gives, for each group, the count of rows, the count of non-NULL
    /// values, their sum, minimum and maximum (NULL when there were none), whatever the order of the rows.
    #[test]
    fn s3f_01_the_hash_table_agrees_with_per_group_folds(rows in prop::collection::vec((prop::option::of(0i32..4), prop::option::of(-20i32..20)), 0..60)) {
        let mut t = table(&[CountStarAggregate, CountAggregate, SumAggregate, MinAggregate, MaxAggregate]);
        for (g, v) in &rows {
            t.insert_combine(key(vec![opt_value(*g)]), &input(vec![int(1), opt_value(*v), opt_value(*v), opt_value(*v), opt_value(*v)])).unwrap();
        }
        let mut got: Vec<(Option<i64>, Vec<Value>)> = t.entries().map(|(k, v)| (k.group_bys[0].as_i64(), v.aggregates.clone())).collect();
        got.sort_by_key(|(g, _)| *g);
        let mut want: Vec<(Option<i64>, Vec<Value>)> = vec![];
        let mut groups: Vec<Option<i32>> = rows.iter().map(|(g, _)| *g).collect();
        groups.sort();
        groups.dedup();
        for g in groups {
            let vs: Vec<Option<i32>> = rows.iter().filter(|(rg, _)| *rg == g).map(|(_, v)| *v).collect();
            let present: Vec<i32> = vs.iter().flatten().copied().collect();
            want.push((g.map(|g| g as i64), vec![
                int(vs.len() as i32),
                if present.is_empty() { null() } else { int(present.len() as i32) }, // BusTub: count(v) of no values is NULL, not 0
                if present.is_empty() { null() } else { int(present.iter().sum()) },
                present.iter().min().map_or_else(null, |m| int(*m)),
                present.iter().max().map_or_else(null, |m| int(*m)),
            ]));
        }
        want.sort_by_key(|(g, _)| *g);
        prop_assert_eq!(got, want);
    }

    /// `group by` through SQL: for random rows (NULLs in both columns) each group has the right five aggregates, an input with no
    /// `group by` is one row (an empty one still gives one), and `select distinct` is the list of groups.
    #[test]
    fn s3f_02_group_by_agrees_with_per_group_folds(rows in prop::collection::vec((prop::option::of(0i64..4), prop::option::of(-20i64..20)), 0..40)) {
        let db = new_db();
        sql(&db, "create table t(g int, v int)");
        insert_rows(&db, "t", &rows.iter().map(|(g, v)| format!("({}, {})", g.map_or("null".to_string(), |g| g.to_string()), v.map_or("null".to_string(), |v| v.to_string()))).collect::<Vec<_>>().iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let agg = |vs: &[Option<i64>]| {
            let present: Vec<i64> = vs.iter().flatten().copied().collect();
            format!("{} {} {} {} {}", vs.len(), cell_text(if present.is_empty() { None } else { Some(present.len() as i64) }), cell_text(if present.is_empty() { None } else { Some(present.iter().sum()) }), cell_text(present.iter().min().copied()), cell_text(present.iter().max().copied()))
        };
        let mut groups: Vec<Option<i64>> = rows.iter().map(|(g, _)| *g).collect();
        groups.sort();
        groups.dedup();
        let want: Vec<String> = groups.iter().map(|g| format!("{} {}", cell_text(*g), agg(&rows.iter().filter(|(rg, _)| rg == g).map(|(_, v)| *v).collect::<Vec<_>>()))).collect();
        prop_assert_eq!(sorted(sql(&db, "select g, count(*), count(v), sum(v), min(v), max(v) from t group by g")), sorted(want));
        let all: Vec<Option<i64>> = rows.iter().map(|(_, v)| *v).collect();
        prop_assert_eq!(sql(&db, "select count(*), count(v), sum(v), min(v), max(v) from t"), vec![agg(&all)]);
        prop_assert_eq!(sorted(sql(&db, "select distinct g from t")), sorted(groups.iter().map(|g| cell_text(*g)).collect()));
    }
}

/// Two tables `l(k, x)` and `r(k, y)` with small keys (so there are many matches) and NULLs.
fn two_tables(unique_right: bool) -> impl Strategy<Value = (Vec<model::Row>, Vec<model::Row>)> {
    let cell = || prop_oneof![1 => Just(None), 5 => (0i64..4).prop_map(Some)];
    let left = prop::collection::vec((cell(), prop::option::of(-3i64..3)).prop_map(|(k, x)| vec![k, x]), 0..14);
    let right = if unique_right {
        prop::collection::vec(prop::option::of(-3i64..3), 0..4)
            .prop_map(|ys| ys.into_iter().enumerate().map(|(i, y)| vec![Some(i as i64), y]).collect::<Vec<_>>())
            .boxed()
    } else {
        prop::collection::vec((cell(), prop::option::of(-3i64..3)).prop_map(|(k, y)| vec![k, y]), 0..14).boxed()
    };
    (left, right)
}

fn load_two(db: &BusTubInstance, l: &[model::Row], r: &[model::Row]) {
    sql(db, "create table l(k int, x int)");
    sql(db, "create table r(k int, y int)");
    for (name, rows) in [("l", l), ("r", r)] {
        insert_rows(db, name, &rows.iter().map(model::values).collect::<Vec<_>>().iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }
}

/// The naive join: every pair, the ON predicate on the pair; a left join adds the unmatched left rows padded with NULLs.
fn naive_join(l: &[model::Row], r: &[model::Row], left: bool, on: impl Fn(&model::Row) -> bool) -> Vec<String> {
    let mut out = vec![];
    for lr in l {
        let mut matched = false;
        for rr in r {
            let pair: model::Row = lr.iter().chain(rr.iter()).copied().collect();
            if on(&pair) {
                matched = true;
                out.push(model::line(&pair));
            }
        }
        if left && !matched {
            out.push(model::line(&lr.iter().copied().chain([None, None]).collect()));
        }
    }
    out.sort();
    out
}

const PAIR: [&str; 4] = ["l.k", "l.x", "r.k", "r.y"];

proptest! {
    #![proptest_config(pconfig())]

    /// A nested loop join of two random tables on a random predicate (any comparison of sums and differences of the four columns)
    /// gives the pairs the predicate keeps, and a left join also the unmatched left rows padded with NULLs.
    #[test]
    fn s3f_03_nested_loop_joins_agree_with_every_pair_checked((l, r) in two_tables(false), on in model::predicate(&PAIR)) {
        let db = new_db();
        load_two(&db, &l, &r);
        let text = model::show(&on);
        prop_assert_eq!(sorted(sql(&db, &format!("select * from l join r on {text}"))), naive_join(&l, &r, false, |p| model::keeps_in(&on, p, &PAIR)), "inner on {}", text);
        prop_assert_eq!(sorted(sql(&db, &format!("select * from l left join r on {text}"))), naive_join(&l, &r, true, |p| model::keeps_in(&on, p, &PAIR)), "left on {}", text);
    }

    /// A hash join on one or two key columns, inner or left, gives the pairs whose keys are all equal (a NULL key matches nothing,
    /// not even another NULL) and, for a left join, the unmatched left rows padded with NULLs.
    #[test]
    fn s3f_04_hash_joins_agree_with_every_pair_checked((l, r) in two_tables(false), two_keys in any::<bool>(), left in any::<bool>()) {
        let db = new_db();
        load_two(&db, &l, &r);
        let n = if two_keys { 2 } else { 1 };
        let keys = || (0..n).map(|i| col(i, TypeId::Integer)).collect::<Vec<_>>();
        let plan = hash_join_plan(&db, "l", "r", keys(), keys(), if left { JoinType::Left } else { JoinType::Inner });
        let want = naive_join(&l, &r, left, |p| (0..n as usize).all(|i| p[i].is_some() && p[i] == p[2 + i]));
        prop_assert_eq!(sorted(run_plan(&db, &plan)), want);
    }

    /// A nested index join (an equality join on an indexed column of the inner table, chosen by the optimizer) gives the same rows
    /// as the naive join, for inner and left joins, and the plan says so.
    #[test]
    fn s3f_05_nested_index_joins_agree_with_every_pair_checked((l, r) in two_tables(true), left in any::<bool>()) {
        let db = new_db();
        load_two(&db, &l, &r);
        sql(&db, "create index rk on r(k)");
        sql(&db, "set force_optimizer_starter_rule=yes");
        let kind = if left { "left join" } else { "join" };
        let plan = sql(&db, &format!("explain (o) select * from l {kind} r on l.k = r.k")).join("\n");
        prop_assert!(plan.contains("NestedIndexJoin"), "{}", plan);
        prop_assert_eq!(sorted(sql(&db, &format!("select * from l {kind} r on l.k = r.k"))), naive_join(&l, &r, left, |p| p[0].is_some() && p[0] == p[2]));
    }
}

// ---- 3f-06 · boss: three join algorithms and a naive one ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, max_shrink_iters: 1000, failure_persistence: None, ..ProptestConfig::default() })]

    /// The same equality join, run as a nested loop join, as a hash join and as a nested index join, gives the same rows as the
    /// naive one, for inner and left joins; swapping the two tables gives the same multiset with the columns swapped; and an
    /// aggregate over the join (`count(*)`, `sum`) agrees with the same aggregate over the naive rows.
    #[test]
    fn s3f_06_three_join_algorithms_agree_with_each_other_and_with_the_naive_join((l, r) in two_tables(true), left in any::<bool>()) {
        let db = new_db();
        load_two(&db, &l, &r);
        let want = naive_join(&l, &r, left, |p| p[0].is_some() && p[0] == p[2]);
        let kind = if left { "left join" } else { "join" };
        let query = format!("select * from l {kind} r on l.k = r.k");
        prop_assert_eq!(sorted(sql(&db, &query)), want.clone(), "nested loop join");
        let plan = hash_join_plan(&db, "l", "r", vec![col(0, TypeId::Integer)], vec![col(0, TypeId::Integer)], if left { JoinType::Left } else { JoinType::Inner });
        prop_assert_eq!(sorted(run_plan(&db, &plan)), want.clone(), "hash join");
        sql(&db, "create index rk on r(k)");
        sql(&db, "set force_optimizer_starter_rule=yes");
        prop_assert_eq!(sorted(sql(&db, &query)), want.clone(), "nested index join");
        sql(&db, "set force_optimizer_starter_rule=no");
        if !left {
            // inner joins commute: the same pairs with the columns in the other order
            let swapped = sorted(sql(&db, "select l.k, l.x, r.k, r.y from r join l on r.k = l.k"));
            prop_assert_eq!(swapped, want.clone(), "join commutes");
        }
        let count = sql(&db, &format!("select count(*) from l {kind} r on l.k = r.k"));
        prop_assert_eq!(count, vec![want.len().to_string()], "count over the join");
    }
}

// @@ challenge 3f-c1 begin
mod ch_3f_c1 {
    use proptest::prelude::*;

    use bustub::execution::merge_join::merge_join;

    #[test]
    fn s3f_c1_duplicates_on_both_sides_give_the_cross_product() {
        let l = [(1, 10), (2, 11), (2, 12)];
        let r = [(2, 20), (2, 21), (3, 22)];
        assert_eq!(merge_join(&l, &r), vec![(11, 20), (11, 21), (12, 20), (12, 21)], "this check");
    }

    #[test]
    fn s3f_c1_disjoint_and_empty_inputs() {
        assert_eq!(merge_join(&[(1, 1)], &[(2, 2)]), vec![], "this check");
        assert_eq!(merge_join(&[], &[(2, 2)]), vec![], "this check");
        assert_eq!(merge_join(&[(1, 1)], &[]), vec![], "this check");
    }

    #[test]
    fn s3f_c1_every_key_matches_in_turn() {
        let l: Vec<_> = (0..5).map(|k| (k, k as u32)).collect();
        let r: Vec<_> = (0..5).map(|k| (k, 100 + k as u32)).collect();
        assert_eq!(merge_join(&l, &r), (0..5u32).map(|k| (k, 100 + k)).collect::<Vec<_>>(), "this check");
    }

    #[test]
    fn s3f_c1_a_large_join_is_linear_not_quadratic() {
        let l: Vec<_> = (0..200_000i64).map(|k| (k, k as u32)).collect();
        let r: Vec<_> = (0..200_000i64).map(|k| (k * 2, k as u32)).collect();
        let t = std::time::Instant::now();
        let out = merge_join(&l, &r);
        assert_eq!(out.len(), 100_000, "this check");
        assert!(t.elapsed() < std::time::Duration::from_secs(3), "took {:?}", t.elapsed());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the same pairs, in the same order, as a nested-loop join.
        #[test]
        fn s3f_c1_property_equals_a_nested_loop(mut l in proptest::collection::vec(0i64..6, 0..12), mut r in proptest::collection::vec(0i64..6, 0..12)) {
            l.sort();
            r.sort();
            let left: Vec<(i64, u32)> = l.iter().enumerate().map(|(i, &k)| (k, i as u32)).collect();
            let right: Vec<(i64, u32)> = r.iter().enumerate().map(|(i, &k)| (k, 100 + i as u32)).collect();
            let mut want = Vec::new();
            for a in &left { for b in &right { if a.0 == b.0 { want.push((a.1, b.1)); } } }
            prop_assert_eq!(merge_join(&left, &right), want);
        }
    }
}
// @@ challenge 3f-c1 end

// @@ challenge 3f-c2 begin
mod ch_3f_c2 {
    use proptest::prelude::*;

    use bustub::execution::semi_join::{anti_join, not_in, semi_join};

    #[test]
    fn s3f_c2_without_nulls_in_the_subquery() {
        let left = [Some(1), Some(2), None];
        let right = [Some(2), Some(3)];
        assert_eq!(semi_join(&left, &right), vec![1], "this check");
        assert_eq!(anti_join(&left, &right), vec![0, 2], "this check");
        assert_eq!(not_in(&left, &right), vec![0], "a NULL left key is not provably absent");
    }

    #[test]
    fn s3f_c2_a_null_in_the_subquery_empties_not_in_but_not_not_exists() {
        let left = [Some(1), Some(2), None];
        let right = [Some(2), None];
        assert_eq!(semi_join(&left, &right), vec![1], "this check");
        assert_eq!(anti_join(&left, &right), vec![0, 2], "this check");
        assert_eq!(not_in(&left, &right), Vec::<usize>::new(), "this check");
    }

    #[test]
    fn s3f_c2_an_empty_subquery_keeps_everything_for_not_in() {
        let left = [Some(1), None];
        assert_eq!(not_in(&left, &[]), vec![0, 1], "NOT IN over nothing is true for every row, NULL or not");
        assert_eq!(semi_join(&left, &[]), Vec::<usize>::new(), "this check");
        assert_eq!(anti_join(&left, &[]), vec![0, 1], "this check");
    }

    #[test]
    fn s3f_c2_no_left_rows() {
        assert!(semi_join(&[], &[Some(1)]).is_empty(), "this check: expected `semi_join(&[], &[Some(1)]).is_empty()`");
        assert!(not_in(&[], &[Some(1)]).is_empty(), "this check: expected `not_in(&[], &[Some(1)]).is_empty()`");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against three-valued logic written out: IN is true/unknown/false, and NOT IN keeps a row only when `NOT (x IN right)` is true.
        #[test]
        fn s3f_c2_property_matches_three_valued_logic(left in proptest::collection::vec(proptest::option::of(0i64..4), 0..8), right in proptest::collection::vec(proptest::option::of(0i64..4), 0..5)) {
            // x IN right: Some(true) if equal to some non-NULL; Some(false) if right is empty or (x non-NULL and all non-NULL differ and no NULL); None (unknown) otherwise
            let in_result = |x: Option<i64>| -> Option<bool> {
                if right.is_empty() { return Some(false); }
                let Some(x) = x else { return None };
                if right.iter().any(|r| *r == Some(x)) { Some(true) }
                else if right.iter().any(|r| r.is_none()) { None } else { Some(false) }
            };
            let want_not_in: Vec<usize> = (0..left.len()).filter(|&i| in_result(left[i]) == Some(false)).collect();
            prop_assert_eq!(not_in(&left, &right), want_not_in);
            let want_semi: Vec<usize> = (0..left.len()).filter(|&i| in_result(left[i]) == Some(true)).collect();
            prop_assert_eq!(semi_join(&left, &right), want_semi.clone());
            let want_anti: Vec<usize> = (0..left.len()).filter(|i| !want_semi.contains(i)).collect();
            prop_assert_eq!(anti_join(&left, &right), want_anti);
        }
    }
}
// @@ challenge 3f-c2 end

// @@ challenge 3f-c3 begin
mod ch_3f_c3 {
    use proptest::prelude::*;

    use bustub::execution::grace_join::{bucket_of, grace_join, partition};

    #[test]
    fn s3f_c3_equal_keys_share_a_bucket_and_nothing_is_lost() {
        let rows: Vec<(i64, u32)> = (0..100).map(|i| ((i % 7) as i64, i as u32)).collect();
        let parts = partition(&rows, 5);
        assert_eq!(parts.len(), 5, "this check");
        assert_eq!(parts.iter().map(Vec::len).sum::<usize>(), 100, "this check");
        for (b, p) in parts.iter().enumerate() {
            assert!(p.iter().all(|r| bucket_of(r.0, 5) == b), "this check: expected `p.iter().all(|r| bucket_of(r.0, 5) == b)`");
        }
        assert!(parts.iter().all(|p| p.windows(2).all(|w| w[0].1 < w[1].1)), "input order is kept inside a bucket");
    }

    #[test]
    fn s3f_c3_the_join_of_two_small_inputs() {
        let l = [(1, 10), (2, 11), (2, 12)];
        let r = [(2, 20), (3, 21), (2, 22)];
        let mut got = grace_join(&l, &r, 3);
        got.sort();
        assert_eq!(got, vec![(11, 20), (11, 22), (12, 20), (12, 22)], "this check");
    }

    #[test]
    fn s3f_c3_a_single_partition_is_a_plain_hash_join() {
        let l = [(1, 1), (1, 2)];
        let r = [(1, 9)];
        let mut got = grace_join(&l, &r, 1);
        got.sort();
        assert_eq!(got, vec![(1, 9), (2, 9)], "this check");
    }

    #[test]
    fn s3f_c3_disjoint_keys_join_to_nothing_whatever_the_partition_count() {
        let l = [(1, 1), (2, 2)];
        let r = [(3, 3), (4, 4)];
        for k in [1, 2, 7, 64] {
            assert_eq!(grace_join(&l, &r, k), Vec::<(u32, u32)>::new(), "k = {k}");
        }
        assert_eq!(grace_join(&[], &r, 4), Vec::<(u32, u32)>::new(), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the multiset of pairs equals a nested-loop join, whatever the number of partitions.
        #[test]
        fn s3f_c3_property_equals_a_nested_loop_for_every_k(l in proptest::collection::vec(0i64..6, 0..12), r in proptest::collection::vec(0i64..6, 0..12), k in 1usize..7) {
            let left: Vec<(i64, u32)> = l.iter().enumerate().map(|(i, &x)| (x, i as u32)).collect();
            let right: Vec<(i64, u32)> = r.iter().enumerate().map(|(i, &x)| (x, 100 + i as u32)).collect();
            let mut want = Vec::new();
            for a in &left { for b in &right { if a.0 == b.0 { want.push((a.1, b.1)); } } }
            want.sort();
            let mut got = grace_join(&left, &right, k);
            got.sort();
            prop_assert_eq!(got, want);
        }
    }
}
// @@ challenge 3f-c3 end

// @@ challenge 3f-c4 begin
mod ch_3f_c4 {
    use proptest::prelude::*;

    use bustub::execution::aggregates::{avg, count_col, count_star, sum};

    #[test]
    fn s3f_c4_nulls_are_skipped_by_everything_but_count_star() {
        let v = [Some(1), None, Some(3)];
        assert_eq!((count_star(&v), count_col(&v), sum(&v)), (3, 2, Some(4)), "this check");
        assert_eq!(avg(&v), Some(2.0), "this check");
    }

    #[test]
    fn s3f_c4_a_column_of_nulls_has_no_sum_and_no_average() {
        let v = [None, None];
        assert_eq!((count_star(&v), count_col(&v), sum(&v), avg(&v)), (2, 0, None, None), "this check");
        assert_eq!(avg(&[]), None, "this check");
    }

    #[test]
    fn s3f_c4_no_nulls_is_the_ordinary_mean() {
        assert_eq!(avg(&[Some(2), Some(4), Some(9)]), Some(5.0), "this check");
    }

    #[test]
    fn s3f_c4_the_average_is_not_rounded_and_a_zero_sum_is_not_null() {
        assert_eq!(avg(&[Some(1), Some(2)]), Some(1.5), "this check");
        assert_eq!(avg(&[Some(-1), None, Some(-2)]), Some(-1.5), "this check");
        assert_eq!(sum(&[Some(-3), Some(3)]), Some(0), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: removing the NULLs first changes nothing but `count_star`, and the average is sum over non-NULL count.
        #[test]
        fn s3f_c4_property_nulls_are_invisible_except_to_count_star(v in proptest::collection::vec(proptest::option::of(-50i64..50), 0..12)) {
            let clean: Vec<Option<i64>> = v.iter().flatten().map(|&x| Some(x)).collect();
            prop_assert_eq!(sum(&v), sum(&clean));
            prop_assert_eq!(count_col(&v), clean.len());
            prop_assert_eq!(avg(&v), avg(&clean));
            prop_assert!(count_col(&v) <= count_star(&v));
            if let Some(a) = avg(&v) { prop_assert!((a * count_col(&v) as f64 - sum(&v).unwrap() as f64).abs() < 1e-6); }
        }
    }
}
// @@ challenge 3f-c4 end

// @@ challenge 3f-c5 begin
mod ch_3f_c5 {
    use proptest::prelude::*;

    use bustub::execution::stream_group::{stream_group_sums, NotSorted};
    use std::collections::BTreeMap;

    #[test]
    fn s3f_c5_groups_of_adjacent_equal_keys() {
        assert_eq!(stream_group_sums(&[(1, 5), (1, 7), (2, 1)]), Ok(vec![(1, 12, 2), (2, 1, 1)]), "this check");
    }

    #[test]
    fn s3f_c5_empty_and_single_row_inputs() {
        assert_eq!(stream_group_sums(&[]), Ok(vec![]), "this check");
        assert_eq!(stream_group_sums(&[(9, -4)]), Ok(vec![(9, -4, 1)]), "this check");
    }

    #[test]
    fn s3f_c5_a_decreasing_key_is_reported_at_its_row() {
        assert_eq!(stream_group_sums(&[(2, 1), (1, 1)]), Err(NotSorted { at: 1 }), "this check");
        assert_eq!(stream_group_sums(&[(1, 1), (2, 1), (2, 1), (1, 1)]), Err(NotSorted { at: 3 }), "this check");
    }

    #[test]
    fn s3f_c5_negative_keys_and_a_long_run() {
        let mut rows: Vec<(i64, i64)> = vec![(-3, 1); 50];
        rows.push((-1, 7));
        assert_eq!(stream_group_sums(&rows), Ok(vec![(-3, 50, 50), (-1, 7, 1)]), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equals a hash group-by sorted by key, whenever the input is sorted.
        #[test]
        fn s3f_c5_property_equals_a_hash_group_by(mut rows in proptest::collection::vec((0i64..5, -9i64..9), 0..20)) {
            rows.sort_by_key(|r| r.0);
            let mut m: BTreeMap<i64, (i64, usize)> = BTreeMap::new();
            for &(k, v) in &rows { let e = m.entry(k).or_insert((0, 0)); e.0 += v; e.1 += 1; }
            let want: Vec<_> = m.into_iter().map(|(k, (s, c))| (k, s, c)).collect();
            prop_assert_eq!(stream_group_sums(&rows), Ok(want));
        }
    }
}
// @@ challenge 3f-c5 end
