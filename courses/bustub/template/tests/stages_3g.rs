//! Tests for module 3g: sorting, limits, top-N and window functions.

use std::cmp::Ordering;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

use bustub::binder::bound_order_by::{OrderBy, OrderByNullType, OrderByType};
use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::exception::ExceptionType;
use bustub::common::result_writer::SimpleStreamWriter;
use bustub::common::rid::Rid;
use bustub::execution::check_options::{CheckOption, CheckOptions};
use bustub::execution::execution_common::{generate_sort_key, SortEntry, TupleComparator};
use bustub::execution::execution_engine::ExecutionEngine;
use bustub::execution::executor_context::ExecutorContext;
use bustub::execution::executor_factory::create_executor;
use bustub::execution::executors::abstract_executor::Executor;
use bustub::execution::executors::external_merge_sort_executor::{ExternalMergeSortExecutor, MergeSortRun, RunBuilder};
use bustub::execution::executors::topn_executor::TopNExecutor;
use bustub::execution::expressions::abstract_expression::ExprRef;
use bustub::execution::expressions::column_value_expression::ColumnValueExpression;
use bustub::execution::plans::plan_node::{PlanKind, PlanNode, PlanRef};
use bustub::storage::table::tuple::Tuple;
use bustub::types::type_id::TypeId;
use bustub::types::value::Value;
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

fn sql_err(db: &BusTubInstance, sql: &str) -> bool {
    let mut out = String::new();
    db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None).is_err()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn insert_rows(db: &BusTubInstance, table: &str, rows: &[String]) {
    for chunk in rows.chunks(500) {
        sql(db, &format!("insert into {table} values {}", chunk.join(", ")));
    }
}

fn col(idx: u32, type_id: TypeId) -> ExprRef {
    let c = if type_id == TypeId::Varchar { Column::new_varchar("c", 100) } else { Column::new("c", type_id) };
    Arc::new(ColumnValueExpression::new(0, idx, c))
}

fn ob(idx: u32, ty: OrderByType, nulls: OrderByNullType) -> OrderBy {
    OrderBy::new(ty, nulls, col(idx, TypeId::Integer))
}

fn asc(idx: u32) -> OrderBy {
    ob(idx, OrderByType::Asc, OrderByNullType::Default)
}

fn desc(idx: u32) -> OrderBy {
    ob(idx, OrderByType::Desc, OrderByNullType::Default)
}

fn entry(keys: Vec<Value>) -> SortEntry {
    (keys, Tuple::empty())
}

// ---- 3g-01 · sort keys and the comparator ---------------------------------------------------------------------------------------

#[test]
fn s3g_01_the_sort_key_is_the_values_of_the_order_by_expressions() {
    let schema = Schema::new(vec![Column::new("a", TypeId::Integer), Column::new_varchar("b", 20), Column::new("c", TypeId::Integer)]);
    let tuple = Tuple::new(&[int(1), Value::varchar("x"), int(3)], &schema);
    let key = generate_sort_key(&tuple, &[desc(2), asc(0)], &schema).unwrap();
    assert_eq!(key, vec![int(3), int(1)], "the sort key is the values of the order by expressions");
    assert!(generate_sort_key(&tuple, &[], &schema).unwrap().is_empty(), "the sort key is the values of the order by expressions: expected `generate_sort_key(&tuple, &[], &schema).unwrap().is_empty()`");
}

#[test]
fn s3g_01_ascending_descending_and_default_directions() {
    let (a, b) = (entry(vec![int(1)]), entry(vec![int(2)]));
    assert_eq!(TupleComparator::new(vec![asc(0)]).compare(&a, &b), Ordering::Less, "ascending descending and default directions");
    assert_eq!(TupleComparator::new(vec![asc(0)]).compare(&b, &a), Ordering::Greater, "ascending descending and default directions");
    assert_eq!(TupleComparator::new(vec![desc(0)]).compare(&a, &b), Ordering::Greater, "ascending descending and default directions");
    assert_eq!(TupleComparator::new(vec![ob(0, OrderByType::Default, OrderByNullType::Default)]).compare(&a, &b), Ordering::Less, "no direction means ascending");
    assert_eq!(TupleComparator::new(vec![asc(0)]).compare(&a, &entry(vec![int(1)])), Ordering::Equal, "ascending descending and default directions");
}

#[test]
fn s3g_01_the_first_key_that_differs_decides() {
    let cmp = TupleComparator::new(vec![asc(0), desc(1)]);
    assert_eq!(cmp.compare(&entry(vec![int(1), int(5)]), &entry(vec![int(2), int(9)])), Ordering::Less, "the first key decides");
    assert_eq!(cmp.compare(&entry(vec![int(1), int(5)]), &entry(vec![int(1), int(9)])), Ordering::Greater, "tie on the first: the second, descending");
    assert_eq!(cmp.compare(&entry(vec![int(1), int(5)]), &entry(vec![int(1), int(5)])), Ordering::Equal, "the first key that differs decides");
}

#[test]
fn s3g_01_null_is_the_smallest_value_by_default() {
    let (n, one) = (entry(vec![null()]), entry(vec![int(1)]));
    assert_eq!(TupleComparator::new(vec![asc(0)]).compare(&n, &one), Ordering::Less, "ascending: NULLs first");
    assert_eq!(TupleComparator::new(vec![desc(0)]).compare(&n, &one), Ordering::Greater, "descending: NULLs last");
    assert_eq!(TupleComparator::new(vec![asc(0)]).compare(&n, &entry(vec![null()])), Ordering::Equal, "null is the smallest value by default");
}

#[test]
fn s3g_01_nulls_first_and_nulls_last_win_over_the_direction() {
    let (n, one) = (entry(vec![null()]), entry(vec![int(1)]));
    let desc_nulls_first = TupleComparator::new(vec![ob(0, OrderByType::Desc, OrderByNullType::NullsFirst)]);
    let asc_nulls_last = TupleComparator::new(vec![ob(0, OrderByType::Asc, OrderByNullType::NullsLast)]);
    assert_eq!(desc_nulls_first.compare(&n, &one), Ordering::Less, "nulls first and nulls last win over the direction");
    assert_eq!(desc_nulls_first.compare(&one, &n), Ordering::Greater, "nulls first and nulls last win over the direction");
    assert_eq!(asc_nulls_last.compare(&n, &one), Ordering::Greater, "nulls first and nulls last win over the direction");
    assert_eq!(asc_nulls_last.compare(&one, &n), Ordering::Less, "nulls first and nulls last win over the direction");
}

#[test]
fn s3g_01_strings_compare_bytewise_and_the_comparator_orders_a_whole_vector() {
    let s = |x: &str| entry(vec![Value::varchar(x)]);
    let by_string = TupleComparator::new(vec![OrderBy::new(OrderByType::Asc, OrderByNullType::Default, col(0, TypeId::Varchar))]);
    assert_eq!(by_string.compare(&s("apple"), &s("banana")), Ordering::Less, "strings compare bytewise and the comparator orders a whole vector");
    assert_eq!(by_string.compare(&s("Zebra"), &s("apple")), Ordering::Less, "upper case sorts before lower case");
    assert_eq!(by_string.compare(&s("ab"), &s("abc")), Ordering::Less, "a prefix is smaller");

    let mut rows: Vec<SortEntry> = vec![vec![int(3), null()], vec![int(1), int(2)], vec![null(), int(0)], vec![int(1), int(1)], vec![int(3), int(5)]].into_iter().map(entry).collect();
    let cmp = TupleComparator::new(vec![asc(0), desc(1)]);
    rows.sort_by(|a, b| cmp.compare(a, b));
    let keys: Vec<Vec<Value>> = rows.into_iter().map(|r| r.0).collect();
    assert_eq!(keys, vec![vec![null(), int(0)], vec![int(1), int(2)], vec![int(1), int(1)], vec![int(3), int(5)], vec![int(3), null()]], "strings compare bytewise and the comparator orders a whole vector");
}

// ---- helpers for the plan-level tests --------------------------------------------------------------------------------------------

fn scan(db: &BusTubInstance, name: &str) -> PlanRef {
    let catalog = db.catalog.read().unwrap();
    let info = catalog.get_table(name).unwrap();
    let schema = Schema::new(info.schema.columns().iter().map(|c| c.with_column_name(&format!("{name}.{}", c.name()))).collect());
    PlanNode::new(Arc::new(schema), vec![], PlanKind::SeqScan { table_oid: info.oid, table_name: name.to_string(), filter_predicate: None })
}

fn sort_plan(db: &BusTubInstance, table: &str, order_bys: Vec<OrderBy>) -> PlanRef {
    let child = scan(db, table);
    PlanNode::new(child.output_schema.clone(), vec![child], PlanKind::Sort { order_bys })
}

fn first_ints(tuples: &[Tuple], schema: &Schema) -> Vec<Value> {
    tuples.iter().map(|t| t.get_value(schema, 0)).collect()
}

fn pages_db() -> BusTubInstance {
    BusTubInstance::new(24)
}

/// A table `t(a int, tag int)` with the given `a` values; `tag` is the position of the row.
fn table_a_tag(db: &BusTubInstance, name: &str, a: &[i32]) {
    sql(db, &format!("create table {name}(a int, tag int)"));
    let rows: Vec<String> = a.iter().enumerate().map(|(i, v)| format!("({v}, {i})")).collect();
    insert_rows(db, name, &rows);
}

// ---- 3g-02 · runs on pages -------------------------------------------------------------------------------------------------------

fn int_tuple(v: i32) -> Tuple {
    Tuple::new(&[int(v)], &Schema::new(vec![Column::new("a", TypeId::Integer)]))
}

fn blob(byte: u8, len: usize) -> Tuple {
    Tuple::from_bytes(Rid::default(), &vec![byte; len])
}

#[test]
fn s3g_02_a_few_tuples_fit_one_page_and_read_back_in_order() {
    let db = pages_db();
    let tuples: Vec<Tuple> = (0..10).map(int_tuple).collect();
    let run = MergeSortRun::from_tuples(db.buffer_pool_manager, tuples.iter()).unwrap();
    assert_eq!(run.page_count(), 1, "a few tuples fit one page and read back in order");
    let back: Vec<Tuple> = run.iter().collect();
    assert_eq!(back.len(), 10, "a few tuples fit one page and read back in order");
    for (i, t) in back.iter().enumerate() {
        assert_eq!(t.data(), tuples[i].data(), "a few tuples fit one page and read back in order");
    }
}

#[test]
fn s3g_02_a_full_page_starts_the_next_one() {
    let db = pages_db();
    // a 1000-byte tuple takes 1004 bytes in a page; 4 bytes of count + 8 * 1004 fit in 8192, a ninth does not
    let tuples: Vec<Tuple> = (0..20).map(|i| blob(i as u8, 1000)).collect();
    let run = MergeSortRun::from_tuples(db.buffer_pool_manager, tuples.iter()).unwrap();
    assert_eq!(run.page_count(), 3, "a full page starts the next one");
    assert_eq!(run.read_page(0).len(), 8, "a full page starts the next one");
    assert_eq!(run.read_page(1).len(), 8, "a full page starts the next one");
    assert_eq!(run.read_page(2).len(), 4, "a full page starts the next one");
    let back: Vec<Tuple> = run.iter().collect();
    assert_eq!(back.len(), 20, "a full page starts the next one");
    assert!(back.iter().enumerate().all(|(i, t)| t.data() == &vec![i as u8; 1000][..]), "order and contents survive");
}

#[test]
fn s3g_02_an_empty_run_has_no_pages() {
    let db = pages_db();
    let run = MergeSortRun::from_tuples(db.buffer_pool_manager, std::iter::empty()).unwrap();
    assert_eq!(run.page_count(), 0, "an empty run has no pages");
    assert_eq!(run.iter().count(), 0, "an empty run has no pages");
}

#[test]
fn s3g_02_a_tuple_that_cannot_fit_in_a_page_is_an_error() {
    let db = pages_db();
    let mut b = RunBuilder::new(db.buffer_pool_manager);
    b.push(&blob(1, 100)).unwrap();
    let e = b.push(&blob(2, 9000)).unwrap_err();
    assert_eq!(e.kind, ExceptionType::Execution, "a tuple that cannot fit in a page is an error");
    // a tuple that exactly fills a page (4 bytes count + 4 bytes length + data) is fine
    let mut b = RunBuilder::new(db.buffer_pool_manager);
    b.push(&blob(3, 8192 - 8)).unwrap();
    assert_eq!(b.finish().page_count(), 1, "a tuple that cannot fit in a page is an error");
}

#[test]
fn s3g_02_a_builder_can_be_filled_tuple_by_tuple_and_the_pages_are_in_the_pool() {
    let db = pages_db();
    let mut b = RunBuilder::new(db.buffer_pool_manager);
    for i in 0..5000 {
        b.push(&int_tuple(i)).unwrap();
    }
    let run = b.finish();
    assert!(run.page_count() >= 5, "5000 small tuples need several pages, more than the pool of 24 frames holds along with a table: they are written out");
    assert_eq!(run.iter().count(), 5000, "a builder can be filled tuple by tuple and the pages are in the pool");
    let sum: i64 = run.iter().map(|t| t.get_value(&Schema::new(vec![Column::new("a", TypeId::Integer)]), 0).as_i64().unwrap()).sum();
    assert_eq!(sum, (0..5000i64).sum::<i64>(), "a builder can be filled tuple by tuple and the pages are in the pool");
    run.delete_pages();
}

#[test]
fn s3g_02_page_by_page_reading_matches_the_iterator() {
    let db = pages_db();
    let tuples: Vec<Tuple> = (0..3000).map(int_tuple).collect();
    let run = MergeSortRun::from_tuples(db.buffer_pool_manager, tuples.iter()).unwrap();
    let by_page: usize = (0..run.page_count()).map(|i| run.read_page(i).len()).sum();
    assert_eq!(by_page, 3000, "page by page reading matches the iterator");
    assert_eq!(run.iter().count(), 3000, "page by page reading matches the iterator");
}

// ---- 3g-02 · initial runs -------------------------------------------------------------------------------------------------------

fn executor_for<'e>(ctx: &'e ExecutorContext<'e>, plan: &PlanRef) -> ExternalMergeSortExecutor<'e, 2> {
    let child = create_executor(ctx, &plan.children[0]).unwrap();
    ExternalMergeSortExecutor::<2>::new(ctx, plan.clone(), child)
}

fn run_values(run: &MergeSortRun<'_>, schema: &Schema) -> Vec<(i64, i64)> {
    run.iter().map(|t| (t.get_value(schema, 0).as_i64().unwrap(), t.get_value(schema, 1).as_i64().unwrap())).collect()
}

#[test]
fn s3g_02_an_empty_input_makes_no_runs() {
    let db = pages_db();
    table_a_tag(&db, "t", &[]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    assert!(executor_for(&ctx, &plan).generate_initial_runs().unwrap().is_empty(), "an empty input makes no runs: expected `executor_for(&ctx, &plan).generate_initial_runs().unwrap().is_empty()`");
}

#[test]
fn s3g_02_a_small_input_is_one_sorted_run_of_one_page() {
    let db = pages_db();
    table_a_tag(&db, "t", &[5, 3, 9, 1, 7]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let runs = executor_for(&ctx, &plan).generate_initial_runs().unwrap();
    assert_eq!(runs.len(), 1, "a small input is one sorted run of one page");
    assert_eq!(runs[0].page_count(), 1, "a small input is one sorted run of one page");
    let values = run_values(&runs[0], &plan.output_schema);
    assert_eq!(values.iter().map(|v| v.0).collect::<Vec<_>>(), vec![1, 3, 5, 7, 9], "a small input is one sorted run of one page");
}

#[test]
fn s3g_02_a_big_input_is_cut_into_runs_of_one_page_each_sorted_inside() {
    let db = pages_db();
    let a: Vec<i32> = (0..4000).map(|i| (i * 7919) % 4001).collect(); // a permutation-ish
    table_a_tag(&db, "t", &a);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let runs = executor_for(&ctx, &plan).generate_initial_runs().unwrap();
    assert!(runs.len() > 1, "4000 rows of 8 bytes (+4 each in a page) do not fit in one page");
    let mut total = 0;
    let mut tags = vec![];
    for r in &runs {
        assert_eq!(r.page_count(), 1, "pass 0 makes runs of exactly one page");
        let v = run_values(r, &plan.output_schema);
        assert!(v.windows(2).all(|w| w[0].0 <= w[1].0), "each run is sorted");
        total += v.len();
        tags.extend(v.iter().map(|x| x.1));
    }
    assert_eq!(total, 4000, "a big input is cut into runs of one page each sorted inside");
    tags.sort();
    assert_eq!(tags, (0..4000).collect::<Vec<_>>(), "every row is in exactly one run");
}

#[test]
fn s3g_02_a_page_is_filled_before_a_new_run_starts() {
    let db = pages_db();
    let a: Vec<i32> = (0..3000).collect();
    table_a_tag(&db, "t", &a);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let runs = executor_for(&ctx, &plan).generate_initial_runs().unwrap();
    // each tuple is 8 bytes of data + 4 of length; a page holds (8192 - 4) / 12 = 682 of them
    let sizes: Vec<usize> = runs.iter().map(|r| r.iter().count()).collect();
    assert_eq!(sizes, vec![682, 682, 682, 682, 272], "a page is filled before a new run starts");
}

#[test]
fn s3g_02_descending_order_and_stability_inside_a_run() {
    let db = pages_db();
    table_a_tag(&db, "t", &[2, 1, 2, 1, 2]);
    let plan = sort_plan(&db, "t", vec![desc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let runs = executor_for(&ctx, &plan).generate_initial_runs().unwrap();
    assert_eq!(run_values(&runs[0], &plan.output_schema), vec![(2, 0), (2, 2), (2, 4), (1, 1), (1, 3)], "equal keys keep their input order");
}

#[test]
fn s3g_02_the_sort_can_be_started_again() {
    let db = pages_db();
    table_a_tag(&db, "t", &[3, 1, 2]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let mut e = executor_for(&ctx, &plan);
    let first = run_values(&e.generate_initial_runs().unwrap()[0], &plan.output_schema);
    let second = run_values(&e.generate_initial_runs().unwrap()[0], &plan.output_schema);
    assert_eq!(first, second, "the sort can be started again");
}

// ---- 3g-03 · merging runs ---------------------------------------------------------------------------------------------------------

fn pair(a: i32, tag: i32) -> Tuple {
    Tuple::new(&[int(a), int(tag)], &Schema::new(vec![Column::new("a", TypeId::Integer), Column::new("tag", TypeId::Integer)]))
}

fn make_run<'e>(db: &'e BusTubInstance, rows: &[(i32, i32)]) -> MergeSortRun<'e> {
    let tuples: Vec<Tuple> = rows.iter().map(|(a, t)| pair(*a, *t)).collect();
    MergeSortRun::from_tuples(db.buffer_pool_manager, tuples.iter()).unwrap()
}

#[test]
fn s3g_03_two_runs_merge_into_one_sorted_run() {
    let db = pages_db();
    table_a_tag(&db, "t", &[]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = executor_for(&ctx, &plan);
    let merged = e.merge_runs(vec![make_run(&db, &[(1, 0), (4, 0), (6, 0)]), make_run(&db, &[(2, 1), (3, 1), (9, 1)])]).unwrap();
    assert_eq!(run_values(&merged, &plan.output_schema).iter().map(|v| v.0).collect::<Vec<_>>(), vec![1, 2, 3, 4, 6, 9], "two runs merge into one sorted run");
}

#[test]
fn s3g_03_many_runs_at_once_and_runs_of_different_lengths() {
    let db = pages_db();
    table_a_tag(&db, "t", &[]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = executor_for(&ctx, &plan);
    let runs = vec![make_run(&db, &[(5, 0)]), make_run(&db, &[(1, 1), (2, 1), (3, 1), (4, 1)]), make_run(&db, &[]), make_run(&db, &[(0, 3), (9, 3)]), make_run(&db, &[(7, 4), (8, 4)])];
    let merged = e.merge_runs(runs).unwrap();
    assert_eq!(run_values(&merged, &plan.output_schema).iter().map(|v| v.0).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5, 7, 8, 9], "many runs at once and runs of different lengths");
}

#[test]
fn s3g_03_equal_keys_come_from_the_earlier_run_first() {
    let db = pages_db();
    table_a_tag(&db, "t", &[]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = executor_for(&ctx, &plan);
    let merged = e.merge_runs(vec![make_run(&db, &[(1, 10), (2, 10)]), make_run(&db, &[(1, 20), (2, 20)]), make_run(&db, &[(1, 30)])]).unwrap();
    assert_eq!(run_values(&merged, &plan.output_schema), vec![(1, 10), (1, 20), (1, 30), (2, 10), (2, 20)], "ties: run order");
}

#[test]
fn s3g_03_merging_respects_descending_order_and_nulls() {
    let db = pages_db();
    sql(&db, "create table t(a int, tag int)");
    let plan = sort_plan(&db, "t", vec![ob(0, OrderByType::Desc, OrderByNullType::NullsFirst)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = executor_for(&ctx, &plan);
    let nullish = Tuple::new(&[null(), int(0)], &Schema::new(vec![Column::new("a", TypeId::Integer), Column::new("tag", TypeId::Integer)]));
    let r1 = MergeSortRun::from_tuples(db.buffer_pool_manager, [nullish.clone(), pair(9, 0), pair(3, 0)].iter()).unwrap();
    let r2 = make_run(&db, &[(8, 1), (3, 1), (1, 1)]);
    let merged = e.merge_runs(vec![r1, r2]).unwrap();
    let out: Vec<Value> = merged.iter().map(|t| t.get_value(&plan.output_schema, 0)).collect();
    assert_eq!(out, vec![null(), int(9), int(8), int(3), int(3), int(1)], "merging respects descending order and nulls");
}

#[test]
fn s3g_03_a_merged_run_is_a_run_like_any_other_and_can_be_merged_again() {
    let db = pages_db();
    table_a_tag(&db, "t", &[]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = executor_for(&ctx, &plan);
    let ab = e.merge_runs(vec![make_run(&db, &[(1, 0), (5, 0)]), make_run(&db, &[(2, 1), (6, 1)])]).unwrap();
    let cd = e.merge_runs(vec![make_run(&db, &[(0, 2), (7, 2)]), make_run(&db, &[(3, 3), (4, 3)])]).unwrap();
    let all = e.merge_runs(vec![ab, cd]).unwrap();
    assert_eq!(run_values(&all, &plan.output_schema).iter().map(|v| v.0).collect::<Vec<_>>(), (0..=7).collect::<Vec<_>>(), "a merged run is a run like any other and can be merged again");
}

#[test]
fn s3g_03_merging_a_lot_of_data_through_a_small_pool() {
    let db = pages_db();
    table_a_tag(&db, "t", &[]);
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let e = executor_for(&ctx, &plan);
    let evens: Vec<(i32, i32)> = (0..6000).filter(|v| v % 2 == 0).map(|v| (v, 0)).collect();
    let odds: Vec<(i32, i32)> = (0..6000).filter(|v| v % 2 == 1).map(|v| (v, 1)).collect();
    let merged = e.merge_runs(vec![make_run(&db, &evens), make_run(&db, &odds)]).unwrap();
    let values = run_values(&merged, &plan.output_schema);
    assert_eq!(values.len(), 6000, "merging a lot of data through a small pool");
    assert!(values.iter().enumerate().all(|(i, v)| v.0 == i as i64), "merging a lot of data through a small pool: expected `values.iter().enumerate().all(|(i, v)| v.0 == i as i64)`");
}

// ---- 3g-03 · the external merge sort executor --------------------------------------------------------------------------------------

#[test]
fn s3g_03_order_by_sorts_ascending_and_descending() {
    let db = new_db();
    sql(&db, "create table t(a int, b int)");
    sql(&db, "insert into t values (3, 30), (1, 10), (2, 20), (5, 50), (4, 40)");
    assert_eq!(sql(&db, "select * from t order by a"), vec!["1 10", "2 20", "3 30", "4 40", "5 50"], "order by sorts ascending and descending");
    assert_eq!(sql(&db, "select * from t order by a desc"), vec!["5 50", "4 40", "3 30", "2 20", "1 10"], "order by sorts ascending and descending");
    assert_eq!(sql(&db, "select b from t order by b desc"), vec!["50", "40", "30", "20", "10"], "order by sorts ascending and descending");
}

#[test]
fn s3g_03_several_keys_expressions_and_nulls() {
    let db = new_db();
    sql(&db, "create table t(a int, b int)");
    sql(&db, "insert into t values (1, 2), (null, 5), (1, 1), (2, null), (null, 1)");
    assert_eq!(sql(&db, "select * from t order by a, b desc"), vec!["integer_null 5", "integer_null 1", "1 2", "1 1", "2 integer_null"], "several keys expressions and nulls");
    assert_eq!(sql(&db, "select * from t order by a desc nulls first, b nulls last"), vec!["integer_null 1", "integer_null 5", "2 integer_null", "1 1", "1 2"], "several keys expressions and nulls");
    assert_eq!(sql(&db, "select * from t where a = 1 order by a + b desc, b"), vec!["1 2", "1 1"], "several keys expressions and nulls");
}

#[test]
fn s3g_03_order_by_an_indexed_column_keeps_the_rows_whose_key_is_null() {
    let db = new_db();
    sql(&db, "create table t(a int, b int)");
    sql(&db, "create index ta on t(a)");
    sql(&db, "insert into t values (null, 1), (2, 2), (1, 3), (null, 4), (3, 5)");
    let mut rows = sql(&db, "select * from t order by a");
    assert_eq!(rows.len(), 5, "every row comes back, though the rows with a NULL key are not in the index: {rows:?}");
    rows[..2].sort();
    assert_eq!(rows, vec!["integer_null 1", "integer_null 4", "1 3", "2 2", "3 5"], "a NULL sorts first in ascending order");
    sql(&db, "delete from t where b = 1 or b = 4");
    let plan = sql(&db, "explain (o) select * from t order by a").join("\n");
    assert!(plan.contains("IndexScan"), "once every row is indexed, the index gives the order\n{plan}");
    assert_eq!(sql(&db, "select * from t order by a"), vec!["1 3", "2 2", "3 5"], "the index scan returns every live row in key order");
}

#[test]
fn s3g_03_a_sort_bigger_than_the_buffer_pool() {
    let db = BusTubInstance::new(16);
    sql(&db, "create table t(a int, tag int)");
    let a: Vec<i32> = (0..20000).map(|i| ((i as i64 * 7919) % 20011) as i32).collect();
    let rows: Vec<String> = a.iter().enumerate().map(|(i, v)| format!("({v}, {i})")).collect();
    insert_rows(&db, "t", &rows);
    let out = sql(&db, "select * from t order by a");
    assert_eq!(out.len(), 20000, "a sort bigger than the buffer pool");
    let firsts: Vec<i64> = out.iter().map(|l| l.split(' ').next().unwrap().parse().unwrap()).collect();
    assert!(firsts.windows(2).all(|w| w[0] <= w[1]), "sorted");
}

#[test]
fn s3g_03_equal_keys_keep_their_input_order() {
    let db = pages_db();
    let a: Vec<i32> = (0..3000).map(|i| i % 3).collect();
    table_a_tag(&db, "t", &a);
    let out = sql(&db, "select * from t order by a");
    let tags_of_zero: Vec<i64> = out.iter().filter(|l| l.starts_with("0 ")).map(|l| l.split(' ').nth(1).unwrap().parse().unwrap()).collect();
    assert_eq!(tags_of_zero, (0..3000).step_by(3).collect::<Vec<_>>(), "a stable sort");
}

#[test]
fn s3g_03_an_empty_table_and_a_single_row() {
    let db = new_db();
    sql(&db, "create table t(a int)");
    assert!(sql(&db, "select * from t order by a").is_empty(), "an empty table and a single row: expected `sql(&db, \"select * from t order by a\").is_empty()`");
    sql(&db, "insert into t values (7)");
    assert_eq!(sql(&db, "select * from t order by a"), vec!["7"], "an empty table and a single row");
}

#[test]
fn s3g_03_strings_sort_and_the_sort_can_run_twice() {
    let db = new_db();
    sql(&db, "create table t(s varchar(20))");
    sql(&db, "insert into t values ('pear'), ('Apple'), ('apple'), ('banana')");
    assert_eq!(sql(&db, "select * from t order by s"), vec!["Apple", "apple", "banana", "pear"], "strings sort and the sort can run twice");
    assert_eq!(sql(&db, "select * from t order by s"), vec!["Apple", "apple", "banana", "pear"], "strings sort and the sort can run twice");
    let plan = sort_plan(&db, "t", vec![OrderBy::new(OrderByType::Asc, OrderByNullType::Default, col(0, TypeId::Varchar))]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let (_, rows) = ExecutionEngine::execute(&plan, &ctx).unwrap();
    assert_eq!(rows.len(), 4, "strings sort and the sort can run twice");
}

#[test]
fn s3g_03_the_sorted_output_comes_in_batches() {
    let db = new_db();
    table_a_tag(&db, "t", &(0..45).rev().collect::<Vec<_>>());
    let plan = sort_plan(&db, "t", vec![asc(0)]);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let mut e = create_executor(&ctx, &plan).unwrap();
    e.init().unwrap();
    let (mut tuples, mut rids) = (vec![], vec![]);
    let mut sizes = vec![];
    while e.next(&mut tuples, &mut rids, 20).unwrap() {
        sizes.push(tuples.len());
        assert_eq!(tuples.len(), rids.len(), "the sorted output comes in batches");
    }
    assert_eq!(sizes, vec![20, 20, 5], "the sorted output comes in batches");
}

// ---- 3g-04 · limit ---------------------------------------------------------------------------------------------------------------------

#[test]
fn s3g_04_limit_returns_the_first_n_rows() {
    let db = new_db();
    table_a_tag(&db, "t", &(0..100).collect::<Vec<_>>());
    assert_eq!(sql(&db, "select a from t limit 3"), vec!["0", "1", "2"], "limit returns the first n rows");
    assert_eq!(sql(&db, "select a from t limit 1"), vec!["0"], "limit returns the first n rows");
    assert_eq!(sql(&db, "select a from t limit 0").len(), 0, "limit returns the first n rows");
}

#[test]
fn s3g_04_a_limit_larger_than_the_input_returns_everything() {
    let db = new_db();
    table_a_tag(&db, "t", &[1, 2, 3]);
    assert_eq!(sql(&db, "select a from t limit 1000"), vec!["1", "2", "3"], "a limit larger than the input returns everything");
}

#[test]
fn s3g_04_limits_that_are_not_a_multiple_of_the_batch_size() {
    let db = new_db();
    table_a_tag(&db, "t", &(0..100).collect::<Vec<_>>());
    assert_eq!(sql(&db, "select a from t limit 20").len(), 20, "limits that are not a multiple of the batch size");
    assert_eq!(sql(&db, "select a from t limit 21").len(), 21, "limits that are not a multiple of the batch size");
    assert_eq!(sql(&db, "select a from t limit 45").len(), 45, "limits that are not a multiple of the batch size");
    assert_eq!(sql(&db, "select a from t limit 99").len(), 99, "limits that are not a multiple of the batch size");
}

#[test]
fn s3g_04_the_rest_of_the_child_is_never_read() {
    // __mock_t9 has 10,000,000 rows: reading them all would take far longer than this test
    let db = new_db();
    db.generate_mock_table();
    let started = std::time::Instant::now();
    assert_eq!(sql(&db, "select * from __mock_t9 limit 5").len(), 5, "the rest of the child is never read");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "the limit must stop pulling from its child");
}

#[test]
fn s3g_04_limit_after_order_by_and_inside_a_subquery() {
    let db = new_db();
    table_a_tag(&db, "t", &[5, 3, 9, 1, 7]);
    assert_eq!(sql(&db, "select a from t order by a desc limit 2"), vec!["9", "7"], "limit after order by and inside a subquery");
    assert_eq!(sql(&db, "select * from (select a from t order by a limit 3) order by a desc limit 2"), vec!["5", "3"], "limit after order by and inside a subquery");
}

#[test]
fn s3g_04_init_starts_the_count_over() {
    let db = new_db();
    table_a_tag(&db, "t", &(0..50).collect::<Vec<_>>());
    let child = scan(&db, "t");
    let plan = PlanNode::new(child.output_schema.clone(), vec![child], PlanKind::Limit { limit: 5 });
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let mut e = create_executor(&ctx, &plan).unwrap();
    let (mut tuples, mut rids) = (vec![], vec![]);
    for _ in 0..2 {
        e.init().unwrap();
        let mut n = 0;
        while e.next(&mut tuples, &mut rids, 20).unwrap() {
            n += tuples.len();
        }
        assert_eq!(n, 5, "each run yields 5 rows");
    }
}

// ---- 3g-04 · top-N ----------------------------------------------------------------------------------------------------------------------

fn topn_plan(db: &BusTubInstance, table: &str, order_bys: Vec<OrderBy>, n: usize) -> PlanRef {
    let child = scan(db, table);
    PlanNode::new(child.output_schema.clone(), vec![child], PlanKind::TopN { order_bys, n })
}

fn run_topn(db: &BusTubInstance, plan: &PlanRef, check: bool) -> Vec<Tuple> {
    let catalog = db.catalog.read().unwrap();
    let mut options = CheckOptions::default();
    if check {
        options.check_options_set.insert(CheckOption::EnableTopnCheck);
    }
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false).with_check_options(options);
    let (ok, tuples) = ExecutionEngine::execute(plan, &ctx).unwrap();
    assert!(ok, "in helper `run_topn`: expected `ok`");
    tuples
}

#[test]
fn s3g_04_the_best_n_rows_in_order() {
    let db = new_db();
    table_a_tag(&db, "t", &[5, 9, 1, 7, 8, 3, 6]);
    let plan = topn_plan(&db, "t", vec![desc(0)], 3);
    assert_eq!(first_ints(&run_topn(&db, &plan, true), &plan.output_schema), vec![int(9), int(8), int(7)], "the best n rows in order");
    let plan = topn_plan(&db, "t", vec![asc(0)], 2);
    assert_eq!(first_ints(&run_topn(&db, &plan, true), &plan.output_schema), vec![int(1), int(3)], "the best n rows in order");
}

#[test]
fn s3g_04_n_larger_than_the_input_and_n_zero() {
    let db = new_db();
    table_a_tag(&db, "t", &[2, 3, 1]);
    let plan = topn_plan(&db, "t", vec![asc(0)], 10);
    assert_eq!(first_ints(&run_topn(&db, &plan, true), &plan.output_schema), vec![int(1), int(2), int(3)], "n larger than the input and n zero");
    let plan = topn_plan(&db, "t", vec![asc(0)], 0);
    assert!(run_topn(&db, &plan, true).is_empty(), "n larger than the input and n zero: expected `run_topn(&db, &plan, true).is_empty()`");
    table_a_tag(&db, "e", &[]);
    assert!(run_topn(&db, &topn_plan(&db, "e", vec![asc(0)], 5), true).is_empty(), "n larger than the input and n zero: expected `run_topn(&db, &topn_plan(&db, \"e\", vec![asc(0)], 5), true).is_empty()`");
}

#[test]
fn s3g_04_ties_keep_the_earlier_rows() {
    let db = new_db();
    table_a_tag(&db, "t", &[1, 1, 1, 1, 1, 0]);
    let plan = topn_plan(&db, "t", vec![desc(0)], 3);
    let out = run_topn(&db, &plan, true);
    let tags: Vec<Value> = out.iter().map(|t| t.get_value(&plan.output_schema, 1)).collect();
    assert_eq!(tags, vec![int(0), int(1), int(2)], "what a stable sort followed by a limit would return");
}

#[test]
fn s3g_04_several_keys_nulls_and_a_big_input() {
    let db = new_db();
    sql(&db, "create table t(a int, b int)");
    sql(&db, "insert into t values (1, 3), (1, 5), (null, 9), (2, 1), (2, 7)");
    let plan = topn_plan(&db, "t", vec![asc(0), desc(1)], 4);
    let out = run_topn(&db, &plan, true);
    let rows: Vec<(Value, Value)> = out.iter().map(|t| (t.get_value(&plan.output_schema, 0), t.get_value(&plan.output_schema, 1))).collect();
    assert_eq!(rows, vec![(null(), int(9)), (int(1), int(5)), (int(1), int(3)), (int(2), int(7))], "several keys nulls and a big input");

    let big: Vec<i32> = (0..5000).map(|i| ((i as i64 * 7919) % 5003) as i32).collect();
    table_a_tag(&db, "big", &big);
    let plan = topn_plan(&db, "big", vec![desc(0)], 5);
    let top = first_ints(&run_topn(&db, &plan, true), &plan.output_schema);
    let mut expect = big.clone();
    expect.sort_by(|a, b| b.cmp(a));
    assert_eq!(top, expect[..5].iter().map(|v| int(*v)).collect::<Vec<_>>(), "several keys nulls and a big input");
}

#[test]
fn s3g_04_the_heap_never_holds_more_than_n_tuples() {
    let db = new_db();
    table_a_tag(&db, "t", &(0..1000).collect::<Vec<_>>());
    let plan = topn_plan(&db, "t", vec![desc(0)], 7);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let child = create_executor(&ctx, &plan.children[0]).unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let mut e = TopNExecutor::new(plan.clone(), child, counter.clone());
    e.init().unwrap();
    assert_eq!(e.get_num_in_heap(), 7, "the heap never holds more than n tuples");
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 7, "the heap never holds more than n tuples");
    let (mut tuples, mut rids) = (vec![], vec![]);
    assert!(e.next(&mut tuples, &mut rids, 20).unwrap(), "the heap never holds more than n tuples: expected `e.next(&mut tuples, &mut rids, 20).unwrap()`");
    assert_eq!(tuples.len(), 7, "the heap never holds more than n tuples");
    assert!(!e.next(&mut tuples, &mut rids, 20).unwrap(), "the heap never holds more than n tuples: expected `!e.next(&mut tuples, &mut rids, 20).unwrap()`");
}

#[test]
fn s3g_04_the_executor_can_be_initialised_again() {
    let db = new_db();
    table_a_tag(&db, "t", &[4, 8, 2, 6]);
    let plan = topn_plan(&db, "t", vec![asc(0)], 2);
    let catalog = db.catalog.read().unwrap();
    let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
    let mut e = create_executor(&ctx, &plan).unwrap();
    for _ in 0..2 {
        e.init().unwrap();
        let (mut tuples, mut rids) = (vec![], vec![]);
        assert!(e.next(&mut tuples, &mut rids, 20).unwrap(), "the executor can be initialised again: expected `e.next(&mut tuples, &mut rids, 20).unwrap()`");
        assert_eq!(first_ints(&tuples, &plan.output_schema), vec![int(2), int(4)], "the executor can be initialised again");
    }
}

// ---- 3g-05 · window functions: partitions ---------------------------------------------------------------------------------------------

fn window_db() -> BusTubInstance {
    let db = new_db();
    sql(&db, "create table t(g int, v int)");
    sql(&db, "insert into t values (1, 100), (1, 200), (1, 300), (2, 400), (2, 500), (3, null)");
    db
}

#[test]
fn s3g_05_an_empty_window_covers_the_whole_table_on_every_row() {
    let db = window_db();
    let rows = sql(&db, "select count(*) over (), sum(v) over (), min(v) over (), max(v) over (), count(v) over () from t");
    assert_eq!(rows.len(), 6, "a window function keeps every row");
    assert!(rows.iter().all(|r| r == "6 1500 100 500 5"), "{rows:?}");
}

#[test]
fn s3g_05_partition_by_gives_each_group_its_own_aggregate() {
    let db = window_db();
    assert_eq!(
        sorted(sql(&db, "select g, v, sum(v) over (partition by g), count(*) over (partition by g) from t")),
        vec!["1 100 600 3", "1 200 600 3", "1 300 600 3", "2 400 900 2", "2 500 900 2", "3 integer_null integer_null 1"], "partition by gives each group its own aggregate"
    );
}

#[test]
fn s3g_05_nulls_in_the_argument_are_ignored_and_a_null_partition_is_a_partition() {
    let db = new_db();
    sql(&db, "create table t(g int, v int)");
    sql(&db, "insert into t values (null, 1), (null, 2), (1, null), (1, null)");
    assert_eq!(
        sorted(sql(&db, "select g, count(v) over (partition by g), sum(v) over (partition by g), count(*) over (partition by g) from t")),
        vec!["1 integer_null integer_null 2", "1 integer_null integer_null 2", "integer_null 2 3 2", "integer_null 2 3 2"], "nulls in the argument are ignored and a null partition is a partition"
    );
}

#[test]
fn s3g_05_several_window_functions_with_different_partitions_are_independent() {
    let db = window_db();
    assert_eq!(
        sorted(sql(&db, "select v, sum(v) over (partition by g), sum(v) over () from t where v >= 200")),
        vec!["200 500 1400", "300 500 1400", "400 900 1400", "500 900 1400"], "several window functions with different partitions are independent"
    );
}

#[test]
fn s3g_05_input_order_is_kept_without_an_order_by() {
    let db = new_db();
    sql(&db, "create table t(g int, v int)");
    sql(&db, "insert into t values (2, 1), (1, 2), (2, 3), (1, 4)");
    assert_eq!(sql(&db, "select g, v, count(*) over (partition by g) from t"), vec!["2 1 2", "1 2 2", "2 3 2", "1 4 2"], "input order is kept without an order by");
}

#[test]
fn s3g_05_empty_table_and_expressions_in_the_select_list() {
    let db = new_db();
    sql(&db, "create table e(g int, v int)");
    assert!(sql(&db, "select count(*) over (), sum(v) over (partition by g) from e").is_empty(), "no rows, no window rows");
    let db = window_db();
    assert_eq!(sql(&db, "select g + 1, sum(v) over (partition by g) from t where g = 2").len(), 2, "empty table and expressions in the select list");
}

#[test]
fn s3g_05_the_planner_refuses_a_group_by_next_to_a_window_function() {
    let db = window_db();
    assert!(sql_err(&db, "select g, count(*) over () from t group by g"), "the planner refuses a group by next to a window function: expected `sql_err(&db, \"select g, count(*) over () from t group by g\")`");
    assert!(sql_err(&db, "select count(*), count(*) over () from t"), "the planner refuses a group by next to a window function: expected `sql_err(&db, \"select count(*), count(*) over () from t\")`");
}

// ---- 3g-05 · window functions: order by, running aggregates and rank -------------------------------------------------------------

#[test]
fn s3g_05_with_an_order_by_the_aggregate_runs_up_to_the_current_row() {
    let db = new_db();
    sql(&db, "create table t(v int)");
    sql(&db, "insert into t values (3), (1), (2), (5), (4)");
    assert_eq!(
        sql(&db, "select v, count(*) over (order by v), sum(v) over (order by v), min(v) over (order by v), max(v) over (order by v) from t"),
        vec!["1 1 1 1 1", "2 2 3 1 2", "3 3 6 1 3", "4 4 10 1 4", "5 5 15 1 5"],
        "rows come out in the window's order"
    );
}

#[test]
fn s3g_05_peers_share_the_value_of_the_last_peer() {
    let db = new_db();
    sql(&db, "create table t(v int)");
    sql(&db, "insert into t values (1), (1), (2), (3), (3), (3)");
    assert_eq!(sql(&db, "select v, sum(v) over (order by v), count(*) over (order by v) from t"), vec!["1 2 2", "1 2 2", "2 4 3", "3 13 6", "3 13 6", "3 13 6"], "peers share the value of the last peer");
}

#[test]
fn s3g_05_rank_numbers_peers_alike_and_leaves_gaps() {
    let db = new_db();
    sql(&db, "create table t(v int)");
    sql(&db, "insert into t values (-99999), (99999), (0), (1), (2), (3)");
    assert_eq!(sorted(sql(&db, "select v, rank() over (order by v) from t")), sorted(vec!["-99999 1".into(), "0 2".into(), "1 3".into(), "2 4".into(), "3 5".into(), "99999 6".into()]), "rank numbers peers alike and leaves gaps");
    sql(&db, "insert into t values (1), (3)");
    assert_eq!(
        sql(&db, "select v, rank() over (order by v) from t"),
        vec!["-99999 1", "0 2", "1 3", "1 3", "2 5", "3 6", "3 6", "99999 8"], "rank numbers peers alike and leaves gaps"
    );
}

#[test]
fn s3g_05_partition_by_with_order_by_restarts_in_each_partition() {
    let db = new_db();
    sql(&db, "create table t(g int, v int)");
    sql(&db, "insert into t values (1, 100), (1, 200), (1, 300), (2, 400), (2, 500)");
    assert_eq!(
        sorted(sql(&db, "select g, v, sum(v) over (partition by g order by v), rank() over (partition by g order by v) from t")),
        vec!["1 100 100 1", "1 200 300 2", "1 300 600 3", "2 400 400 1", "2 500 900 2"], "partition by with order by restarts in each partition"
    );
}

#[test]
fn s3g_05_descending_order_and_nulls() {
    let db = new_db();
    sql(&db, "create table t(v int)");
    sql(&db, "insert into t values (1), (null), (2)");
    assert_eq!(sql(&db, "select v, count(v) over (order by v desc), rank() over (order by v desc) from t"), vec!["2 1 1", "1 2 2", "integer_null 2 3"], "descending order and nulls");
    assert_eq!(sql(&db, "select v, count(*) over (order by v) from t"), vec!["integer_null 1", "1 2", "2 3"], "NULL is smallest ascending");
}

#[test]
fn s3g_05_one_order_by_serves_every_function_that_has_one() {
    let db = new_db();
    sql(&db, "create table t(v int)");
    sql(&db, "insert into t values (2), (1), (3)");
    assert_eq!(sql(&db, "select v, sum(v) over (order by v), count(*) over (order by v) from t"), vec!["1 1 1", "2 3 2", "3 6 3"], "one order by serves every function that has one");
    assert!(sql_err(&db, "select v, sum(v) over (), sum(v) over (order by v) from t"), "BusTub's planner refuses to mix windows with and without order by");
}

#[test]
fn s3g_05_rank_needs_an_order_by_and_other_frames_are_refused() {
    let db = new_db();
    sql(&db, "create table t(v int)");
    assert!(sql_err(&db, "select rank() over () from t"), "rank needs an order by and other frames are refused: expected `sql_err(&db, \"select rank() over () from t\")`");
    assert!(sql_err(&db, "select sum(v) over (order by v rows between 1 preceding and current row) from t"), "rank needs an order by and other frames are refused: expected `sql_err(&db, \"select sum(v) over (order by v rows between 1 preceding and current row) from t\")`");
    assert!(sql_err(&db, "select sum(v) over (order by v), sum(v) over (order by v desc) from t"), "the order-by clauses must agree");
}

// ---- properties: sorting, limits and windows against plain Rust --------------------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 40, max_shrink_iters: 1000, failure_persistence: None, ..ProptestConfig::default() }
}

fn opt_value(v: Option<i32>) -> Value {
    v.map_or_else(null, int)
}

fn cell_text(v: Option<i32>) -> String {
    v.map_or("integer_null".to_string(), |v| v.to_string())
}

fn sql_cell(v: Option<i32>) -> String {
    v.map_or("null".to_string(), |v| v.to_string())
}

/// One `ORDER BY` item over a column: its index, whether it is descending, and `NULLS FIRST` (1), `NULLS LAST` (2) or neither (0).
#[derive(Clone, Copy, Debug)]
struct Spec {
    column: usize,
    desc: bool,
    nulls: u8,
}

fn spec_strategy(columns: usize) -> impl Strategy<Value = Spec> {
    (0..columns, any::<bool>(), 0u8..3).prop_map(|(column, desc, nulls)| Spec { column, desc, nulls })
}

impl Spec {
    fn order_by(&self) -> OrderBy {
        ob(
            self.column as u32,
            if self.desc { OrderByType::Desc } else { OrderByType::Asc },
            [OrderByNullType::Default, OrderByNullType::NullsFirst, OrderByNullType::NullsLast][self.nulls as usize],
        )
    }

    fn sql(&self, names: &[&str]) -> String {
        format!("{} {}{}", names[self.column], if self.desc { "desc" } else { "asc" }, [" ", " nulls first", " nulls last"][self.nulls as usize])
    }

    /// The model: NULL is the smallest value unless `NULLS FIRST/LAST` says otherwise; a descending key reverses the values.
    fn compare(&self, a: &[Option<i32>], b: &[Option<i32>]) -> Ordering {
        let nulls_first = self.nulls == 1 || (self.nulls == 0 && !self.desc);
        match (a[self.column], b[self.column]) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => if nulls_first { Ordering::Less } else { Ordering::Greater },
            (Some(_), None) => if nulls_first { Ordering::Greater } else { Ordering::Less },
            (Some(x), Some(y)) => if self.desc { y.cmp(&x) } else { x.cmp(&y) },
        }
    }
}

fn model_compare(specs: &[Spec], a: &[Option<i32>], b: &[Option<i32>]) -> Ordering {
    specs.iter().map(|s| s.compare(a, b)).find(|o| *o != Ordering::Equal).unwrap_or(Ordering::Equal)
}

fn small_cell() -> impl Strategy<Value = Option<i32>> {
    prop_oneof![1 => Just(None), 6 => (-4i32..=4).prop_map(Some)]
}

proptest! {
    #![proptest_config(pconfig())]

    /// The comparator orders entries as a model that spells out the rules (a NULL is smallest unless NULLS FIRST/LAST; descending
    /// reverses the values; the first key that differs decides); it is antisymmetric, and sorting with it is the model's stable sort.
    #[test]
    fn s3g_01_the_comparator_agrees_with_a_model_of_the_rules(rows in prop::collection::vec(prop::collection::vec(small_cell(), 3), 0..40), specs in prop::collection::vec(spec_strategy(3), 1..4)) {
        let cmp = TupleComparator::new(specs.iter().map(Spec::order_by).collect());
        let entries: Vec<(SortEntry, Vec<Option<i32>>)> = rows.iter().map(|r| {
            let keys: Vec<Value> = specs.iter().map(|s| opt_value(r[s.column])).collect();
            (entry(keys), r.clone())
        }).collect();
        // the sort key holds the values of the order by columns in order, so the comparator needs a spec per position of the key
        let positional: Vec<Spec> = specs.iter().enumerate().map(|(i, s)| Spec { column: i, ..*s }).collect();
        let keyed: Vec<Vec<Option<i32>>> = rows.iter().map(|r| specs.iter().map(|s| r[s.column]).collect()).collect();
        for (i, a) in entries.iter().enumerate() {
            for (j, b) in entries.iter().enumerate() {
                let want = model_compare(&positional, &keyed[i], &keyed[j]);
                prop_assert_eq!(cmp.compare(&a.0, &b.0), want, "{:?} vs {:?} by {:?}", keyed[i], keyed[j], specs);
                prop_assert_eq!(cmp.compare(&b.0, &a.0), want.reverse(), "antisymmetry");
            }
        }
        let mut sorted = entries.clone();
        sorted.sort_by(|a, b| cmp.compare(&a.0, &b.0));
        let mut model = rows.iter().cloned().zip(keyed.iter()).collect::<Vec<_>>();
        model.sort_by(|a, b| model_compare(&positional, a.1, b.1));
        prop_assert_eq!(sorted.into_iter().map(|e| e.1).collect::<Vec<_>>(), model.into_iter().map(|m| m.0).collect::<Vec<_>>());
    }

    /// A run keeps its tuples in order whatever their sizes; pages are filled greedily (4 bytes of count, then 4 bytes of length and
    /// the data for each tuple, 8192 bytes in all), so the number of pages is what a simulation of that rule gives.
    #[test]
    fn s3g_02_a_run_reads_back_what_it_was_given_on_the_pages_the_rule_predicts(sizes in prop::collection::vec(1usize..3000, 0..80)) {
        let db = pages_db();
        let tuples: Vec<Tuple> = sizes.iter().enumerate().map(|(i, n)| blob(i as u8, *n)).collect();
        let run = MergeSortRun::from_tuples(db.buffer_pool_manager, tuples.iter()).unwrap();
        let (mut pages, mut used) = (0usize, 0usize);
        for n in &sizes {
            if pages == 0 || used + 4 + n > 8192 { pages += 1; used = 4; }
            used += 4 + n;
        }
        prop_assert_eq!(run.page_count(), pages);
        let back: Vec<Tuple> = run.iter().collect();
        prop_assert_eq!(back.len(), tuples.len());
        for (a, b) in back.iter().zip(&tuples) { prop_assert_eq!(a.data(), b.data()); }
        prop_assert_eq!((0..run.page_count()).map(|p| run.read_page(p).len()).sum::<usize>(), tuples.len());
    }

    /// Pass 0 cuts the input into runs of one page, each sorted by the key, and together they hold exactly the input rows.
    #[test]
    fn s3g_02_initial_runs_are_sorted_pages_holding_exactly_the_input(a in prop::collection::vec(-50i32..50, 0..1500)) {
        let db = pages_db();
        table_a_tag(&db, "t", &a);
        let plan = sort_plan(&db, "t", vec![asc(0)]);
        let catalog = db.catalog.read().unwrap();
        let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
        let runs = executor_for(&ctx, &plan).generate_initial_runs().unwrap();
        let mut all = vec![];
        for run in &runs {
            prop_assert_eq!(run.page_count(), 1, "each initial run is one page");
            let values = run_values(run, &plan.output_schema);
            prop_assert!(values.windows(2).all(|w| w[0].0 <= w[1].0), "a run is sorted");
            all.extend(values);
        }
        let mut want: Vec<(i64, i64)> = a.iter().enumerate().map(|(i, v)| (*v as i64, i as i64)).collect();
        all.sort();
        want.sort();
        prop_assert_eq!(all, want);
    }

    /// Merging sorted runs gives the sorted whole, and rows with equal keys come out in the order of the runs they came from.
    #[test]
    fn s3g_03_merging_runs_is_a_stable_merge(parts in prop::collection::vec(prop::collection::vec(-6i32..6, 0..40), 1..5)) {
        let db = pages_db();
        table_a_tag(&db, "t", &[]);
        let plan = sort_plan(&db, "t", vec![asc(0)]);
        let catalog = db.catalog.read().unwrap();
        let ctx = ExecutorContext::new(&catalog, db.buffer_pool_manager, false);
        let e = executor_for(&ctx, &plan);
        let mut runs = vec![];
        let mut model: Vec<(i32, i32)> = vec![];
        for (i, part) in parts.iter().enumerate() {
            let mut rows: Vec<(i32, i32)> = part.iter().map(|v| (*v, i as i32)).collect();
            rows.sort_by_key(|r| r.0);
            model.extend(rows.iter().copied());
            runs.push(make_run(&db, &rows));
        }
        model.sort_by_key(|r| r.0); // stable: ties keep the order of the runs
        let merged = e.merge_runs(runs).unwrap();
        let got: Vec<(i32, i32)> = run_values(&merged, &plan.output_schema).into_iter().map(|(a, t)| (a as i32, t as i32)).collect();
        prop_assert_eq!(got, model);
    }

    /// `order by` on random rows (NULLs in the keys, several keys, both directions, NULLS FIRST/LAST) is the model's stable sort,
    /// through a small buffer pool, whether or not the rows fit in memory.
    #[test]
    fn s3g_03_order_by_agrees_with_a_stable_sort(rows in prop::collection::vec((small_cell(), small_cell()), 0..1200), specs in prop::collection::vec(spec_strategy(2), 1..3)) {
        let db = pages_db();
        sql(&db, "create table t(a int, b int, tag int)");
        insert_rows(&db, "t", &rows.iter().enumerate().map(|(i, (a, b))| format!("({}, {}, {i})", sql_cell(*a), sql_cell(*b))).collect::<Vec<_>>());
        let clause = specs.iter().map(|s| s.sql(&["a", "b"])).collect::<Vec<_>>().join(", ");
        let mut model: Vec<(Vec<Option<i32>>, usize)> = rows.iter().enumerate().map(|(i, (a, b))| (vec![*a, *b], i)).collect();
        model.sort_by(|x, y| model_compare(&specs, &x.0, &y.0));
        let want: Vec<String> = model.iter().map(|(r, i)| format!("{} {} {i}", cell_text(r[0]), cell_text(r[1]))).collect();
        prop_assert_eq!(sql(&db, &format!("select * from t order by {clause}")), want, "order by {}", clause);
    }

    /// `order by .. limit n` (top-N) is the first n rows of the stable sort, for any n including 0 and more than there are rows.
    #[test]
    fn s3g_04_limit_and_top_n_are_prefixes_of_the_stable_sort(rows in prop::collection::vec(small_cell(), 0..60), specs in prop::collection::vec(spec_strategy(1), 1..2), n in 0usize..70) {
        let db = new_db();
        sql(&db, "create table t(a int, tag int)");
        insert_rows(&db, "t", &rows.iter().enumerate().map(|(i, a)| format!("({}, {i})", sql_cell(*a))).collect::<Vec<_>>());
        let mut model: Vec<(Vec<Option<i32>>, usize)> = rows.iter().enumerate().map(|(i, a)| (vec![*a], i)).collect();
        model.sort_by(|x, y| model_compare(&specs, &x.0, &y.0));
        let prefix: Vec<String> = model.iter().take(n).map(|(r, i)| format!("{} {i}", cell_text(r[0]))).collect();
        let plan = topn_plan(&db, "t", specs.iter().map(Spec::order_by).collect(), n);
        let got: Vec<String> = run_topn(&db, &plan, true).iter().map(|t| format!("{} {}", t.get_value(&plan.output_schema, 0).to_string(), t.get_value(&plan.output_schema, 1))).collect();
        prop_assert_eq!(got, prefix.clone(), "top-{}", n);
        prop_assert_eq!(sql(&db, &format!("select * from t order by {} limit {n}", specs[0].sql(&["a"]))), prefix, "order by .. limit {}", n);
        prop_assert_eq!(sql(&db, &format!("select a, tag from t limit {n}")).len(), n.min(rows.len()), "limit {}", n);
    }

    /// Window functions agree with an `O(n²)` oracle that looks at the whole partition for every row: aggregates over the partition,
    /// running aggregates (with peers) when there is an `ORDER BY`, and `rank` with gaps.
    #[test]
    fn s3g_05_window_functions_agree_with_a_quadratic_oracle(rows in prop::collection::vec((prop::option::of(0i32..3), small_cell(), small_cell()), 0..30)) {
        let db = new_db();
        sql(&db, "create table t(g int, o int, v int)");
        insert_rows(&db, "t", &rows.iter().map(|(g, o, v)| format!("({}, {}, {})", sql_cell(*g), sql_cell(*o), sql_cell(*v))).collect::<Vec<_>>());
        let agg = |vs: Vec<Option<i32>>| {
            let present: Vec<i32> = vs.iter().flatten().copied().collect();
            let none = present.is_empty();
            format!(
                "{} {} {} {} {}",
                vs.len(),
                if none { "integer_null".to_string() } else { present.len().to_string() },
                if none { "integer_null".to_string() } else { present.iter().sum::<i32>().to_string() },
                present.iter().min().map_or("integer_null".to_string(), |m| m.to_string()),
                present.iter().max().map_or("integer_null".to_string(), |m| m.to_string()),
            )
        };
        // no ORDER BY: the whole partition
        let mut want: Vec<String> = rows.iter().map(|(g, o, v)| {
            let part: Vec<Option<i32>> = rows.iter().filter(|r| r.0 == *g).map(|r| r.2).collect();
            format!("{} {} {} {}", cell_text(*g), cell_text(*o), cell_text(*v), agg(part))
        }).collect();
        want.sort();
        let q = "select g, o, v, count(*) over (partition by g), count(v) over (partition by g), sum(v) over (partition by g), min(v) over (partition by g), max(v) over (partition by g) from t";
        prop_assert_eq!(sorted(sql(&db, q)), want);
        // ORDER BY o: up to and including the peers (equal o); NULL o sorts first
        let before = |a: Option<i32>, b: Option<i32>| a <= b; // None < Some(_)
        let mut want: Vec<String> = rows.iter().map(|(g, o, v)| {
            let upto: Vec<Option<i32>> = rows.iter().filter(|r| r.0 == *g && before(r.1, *o)).map(|r| r.2).collect();
            let rank = 1 + rows.iter().filter(|r| r.0 == *g && r.1 < *o).count();
            format!("{} {} {} {} {rank}", cell_text(*g), cell_text(*o), cell_text(*v), agg(upto))
        }).collect();
        want.sort();
        let q = "select g, o, v, count(*) over (partition by g order by o), count(v) over (partition by g order by o), sum(v) over (partition by g order by o), min(v) over (partition by g order by o), max(v) over (partition by g order by o), rank() over (partition by g order by o) from t";
        prop_assert_eq!(sorted(sql(&db, q)), want);
    }
}

// ---- 3g-06 · boss: the whole pipeline against a sorted vector ----------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 500, failure_persistence: None, ..ProptestConfig::default() })]

    /// A query that sorts, limits and ranks through the optimizer (which turns `order by .. limit` into a top-N) agrees with a
    /// sorted vector three ways: the full sort, its prefix, and the rank of every row in that order.
    #[test]
    fn s3g_06_a_sorted_pipeline_agrees_with_a_sorted_vector(rows in prop::collection::vec((small_cell(), small_cell()), 0..400), specs in prop::collection::vec(spec_strategy(2), 1..3), n in 0usize..30) {
        let db = pages_db();
        sql(&db, "create table t(a int, b int, tag int)");
        insert_rows(&db, "t", &rows.iter().enumerate().map(|(i, (a, b))| format!("({}, {}, {i})", sql_cell(*a), sql_cell(*b))).collect::<Vec<_>>());
        let clause = specs.iter().map(|s| s.sql(&["a", "b"])).collect::<Vec<_>>().join(", ");
        let mut model: Vec<(Vec<Option<i32>>, usize)> = rows.iter().enumerate().map(|(i, (a, b))| (vec![*a, *b], i)).collect();
        model.sort_by(|x, y| model_compare(&specs, &x.0, &y.0));
        let line = |(r, i): &(Vec<Option<i32>>, usize)| format!("{} {} {i}", cell_text(r[0]), cell_text(r[1]));
        let full: Vec<String> = model.iter().map(line).collect();
        prop_assert_eq!(sql(&db, &format!("select * from t order by {clause}")), full.clone(), "sort");
        prop_assert_eq!(sql(&db, &format!("select * from t order by {clause} limit {n}")), full.iter().take(n).cloned().collect::<Vec<_>>(), "top-n");
        let ranks = sql(&db, &format!("select a, b, tag, rank() over (order by {clause}) from t"));
        // `rank` over several keys ranks by all of them
        let want_all: Vec<String> = rows.iter().enumerate().map(|(i, (a, b))| {
            let me = vec![*a, *b];
            let r = 1 + model.iter().filter(|(o, _)| model_compare(&specs, o, &me) == Ordering::Less).count();
            format!("{} {} {i} {r}", cell_text(*a), cell_text(*b))
        }).collect();
        prop_assert_eq!(sorted(ranks), sorted(want_all), "rank");
    }
}

// @@ challenge 3g-c1 begin
mod ch_3g_c1 {
    use proptest::prelude::*;

    use bustub::execution::sort_plan::{plan_sort, SortPlan};

    #[test]
    fn s3g_c1_a_typical_plan() {
        assert_eq!(plan_sort(100, 10), Some(SortPlan { initial_runs: 10, passes: 3, io_pages: 600 }), "this check");
    }

    #[test]
    fn s3g_c1_an_input_that_fits_in_memory_is_one_pass() {
        assert_eq!(plan_sort(8, 10), Some(SortPlan { initial_runs: 1, passes: 1, io_pages: 16 }), "this check");
        assert_eq!(plan_sort(10, 10), Some(SortPlan { initial_runs: 1, passes: 1, io_pages: 20 }), "this check");
    }

    #[test]
    fn s3g_c1_exact_powers_of_the_fan_in_do_not_cost_an_extra_pass() {
        // buffer 5: fan-in 4. 5 * 4^3 pages -> 64 runs -> 16 -> 4 -> 1: three merge passes
        assert_eq!(plan_sort(5 * 64, 5).unwrap().passes, 4, "this check");
        assert_eq!(plan_sort(5 * 64 + 1, 5).unwrap().passes, 5, "one page more makes 65 runs and one more pass");
    }

    #[test]
    fn s3g_c1_no_pages_and_not_enough_memory() {
        assert_eq!(plan_sort(0, 10), Some(SortPlan { initial_runs: 0, passes: 0, io_pages: 0 }), "this check");
        assert_eq!(plan_sort(100, 2), None, "this check");
        assert_eq!(plan_sort(100, 0), None, "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: monotone in both arguments, one pass when it fits, and equal to simulating the merges.
        #[test]
        fn s3g_c1_property_the_plan_is_monotone_and_matches_a_simulation(pages in 1u64..5000, buffer in 3u64..40) {
            let p = plan_sort(pages, buffer).unwrap();
            let mut runs = pages.div_ceil(buffer);
            let mut passes = 1;
            while runs > 1 { runs = runs.div_ceil(buffer - 1); passes += 1; }
            prop_assert_eq!(p.passes, passes);
            prop_assert_eq!(p.io_pages, 2 * pages * passes as u64);
            prop_assert!(plan_sort(pages, buffer + 1).unwrap().passes <= p.passes);
            prop_assert!(plan_sort(pages + 1, buffer).unwrap().passes >= p.passes);
            if pages <= buffer { prop_assert_eq!(p.passes, 1); }
        }
    }
}
// @@ challenge 3g-c1 end

// @@ challenge 3g-c2 begin
mod ch_3g_c2 {
    use proptest::prelude::*;

    use bustub::execution::top_n::TopN;

    #[test]
    fn s3g_c2_the_smallest_n_with_ties_by_arrival() {
        let mut t = TopN::new(2);
        for (k, p) in [(5, 0), (1, 1), (5, 2), (1, 3)] {
            t.push(k, p);
        }
        assert_eq!(t.finish(), vec![(1, 1), (1, 3)], "this check");
    }

    #[test]
    fn s3g_c2_an_earlier_arrival_wins_a_tie_at_the_boundary() {
        let mut t = TopN::new(2);
        for (k, p) in [(3, 10), (3, 11), (3, 12)] {
            t.push(k, p);
        }
        assert_eq!(t.finish(), vec![(3, 10), (3, 11)], "this check");
    }

    #[test]
    fn s3g_c2_n_zero_holds_nothing() {
        let mut t = TopN::new(0);
        t.push(1, 1);
        assert_eq!((t.len(), t.is_empty()), (0, true), "this check");
        assert_eq!(t.finish(), vec![], "this check");
    }

    #[test]
    fn s3g_c2_fewer_rows_than_n_and_bounded_memory() {
        let mut t = TopN::new(3);
        t.push(9, 0);
        assert_eq!(t.len(), 1, "this check");
        for i in 0..1000 {
            t.push(1000 - i, i as u32);
            assert!(t.len() <= 3, "this check: expected `t.len() <= 3`");
        }
        let r = t.finish();
        assert_eq!(r.iter().map(|x| x.0).collect::<Vec<_>>(), vec![1, 2, 3], "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a stable sort of everything, cut at `n`.
        #[test]
        fn s3g_c2_property_equals_a_stable_sort_and_take(rows in proptest::collection::vec(-5i64..5, 0..30), n in 0usize..10) {
            let mut t = TopN::new(n);
            for (i, &k) in rows.iter().enumerate() { t.push(k, i as u32); prop_assert!(t.len() <= n); }
            let mut want: Vec<(i64, u32)> = rows.iter().enumerate().map(|(i, &k)| (k, i as u32)).collect();
            want.sort_by_key(|r| r.0);
            want.truncate(n);
            prop_assert_eq!(t.finish(), want);
        }
    }
}
// @@ challenge 3g-c2 end

// @@ challenge 3g-c3 begin
mod ch_3g_c3 {
    use proptest::prelude::*;

    use bustub::execution::moving_sum::moving_sum;

    fn s(v: &[i64]) -> Vec<Option<i64>> {
        v.iter().map(|&x| Some(x)).collect()
    }

    #[test]
    fn s3g_c3_a_running_total_and_a_trailing_window() {
        assert_eq!(moving_sum(&s(&[1, 2, 3, 4]), 100, 0), s(&[1, 3, 6, 10]), "this check");
        assert_eq!(moving_sum(&s(&[1, 2, 3, 4]), 1, 0), s(&[1, 3, 5, 7]), "this check");
    }

    #[test]
    fn s3g_c3_a_centred_window_is_clipped_at_both_ends() {
        assert_eq!(moving_sum(&s(&[1, 2, 3, 4]), 1, 1), s(&[3, 6, 9, 7]), "this check");
    }

    #[test]
    fn s3g_c3_nulls_contribute_nothing_and_an_all_null_frame_is_null() {
        let v = [Some(1), None, Some(3)];
        assert_eq!(moving_sum(&v, 1, 1), s(&[1, 4, 3]), "this check");
        assert_eq!(moving_sum(&[None, None], 1, 1), vec![None, None], "this check");
        assert_eq!(moving_sum(&[None, Some(5)], 0, 0), vec![None, Some(5)], "this check");
    }

    #[test]
    fn s3g_c3_empty_input_and_a_zero_frame() {
        assert_eq!(moving_sum(&[], 2, 2), vec![], "this check");
        assert_eq!(moving_sum(&s(&[7, 8]), 0, 0), s(&[7, 8]), "this check");
    }

    #[test]
    fn s3g_c3_a_long_column_is_linear() {
        let v: Vec<Option<i64>> = (0..200_000).map(|i| Some(i % 7)).collect();
        let t = std::time::Instant::now();
        let r = moving_sum(&v, 50_000, 50_000);
        assert_eq!(r.len(), v.len(), "this check");
        assert!(t.elapsed() < std::time::Duration::from_secs(3), "took {:?}: recomputing each frame is quadratic", t.elapsed());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equals summing each frame directly.
        #[test]
        fn s3g_c3_property_equals_the_naive_frame_sum(v in proptest::collection::vec(proptest::option::of(-20i64..20), 0..15), p in 0usize..5, f in 0usize..5) {
            let got = moving_sum(&v, p, f);
            for i in 0..v.len() {
                let frame = &v[i.saturating_sub(p)..(i + f + 1).min(v.len())];
                let want = if frame.iter().all(|x| x.is_none()) { None } else { Some(frame.iter().flatten().sum::<i64>()) };
                prop_assert_eq!(got[i], want, "row {}", i);
            }
            prop_assert_eq!(got.len(), v.len());
        }
    }
}
// @@ challenge 3g-c3 end

// @@ challenge 3g-c4 begin
mod ch_3g_c4 {
    use proptest::prelude::*;

    use bustub::execution::ranks::{dense_rank, rank, row_number};

    #[test]
    fn s3g_c4_the_three_functions_on_data_with_a_tie() {
        let k = [10, 20, 20, 30];
        assert_eq!(row_number(&k), vec![1, 2, 3, 4], "this check");
        assert_eq!(rank(&k), vec![1, 2, 2, 4], "this check");
        assert_eq!(dense_rank(&k), vec![1, 2, 2, 3], "this check");
    }

    #[test]
    fn s3g_c4_a_big_tie_group_makes_a_big_gap() {
        let k = [1, 1, 1, 1, 2];
        assert_eq!(rank(&k), vec![1, 1, 1, 1, 5], "this check");
        assert_eq!(dense_rank(&k), vec![1, 1, 1, 1, 2], "this check");
    }

    #[test]
    fn s3g_c4_without_ties_all_three_agree_and_empty_is_empty() {
        let k = [1, 2, 3];
        assert_eq!(rank(&k), row_number(&k), "this check");
        assert_eq!(dense_rank(&k), row_number(&k), "this check");
        assert_eq!(rank(&[]), Vec::<usize>::new(), "this check");
    }

    #[test]
    fn s3g_c4_all_keys_equal() {
        let k = [5, 5, 5];
        assert_eq!(rank(&k), vec![1, 1, 1], "this check");
        assert_eq!(dense_rank(&k), vec![1, 1, 1], "this check");
        assert_eq!(row_number(&k), vec![1, 2, 3], "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: the definitions, counting smaller keys.
        #[test]
        fn s3g_c4_property_rank_counts_smaller_rows(mut keys in proptest::collection::vec(-4i64..4, 0..15)) {
            keys.sort();
            let r = rank(&keys);
            let d = dense_rank(&keys);
            for (i, k) in keys.iter().enumerate() {
                prop_assert_eq!(r[i], 1 + keys.iter().filter(|x| *x < k).count());
                let mut smaller: Vec<_> = keys.iter().filter(|x| *x < k).collect();
                smaller.dedup();
                prop_assert_eq!(d[i], 1 + smaller.len());
            }
        }
    }
}
// @@ challenge 3g-c4 end

// @@ challenge 3g-c5 begin
mod ch_3g_c5 {
    use proptest::prelude::*;

    use bustub::execution::radix_sort::radix_sort;

    #[test]
    fn s3g_c5_sorts_by_key_keeping_the_order_of_equal_keys() {
        let mut v = vec![(3, 1), (1, 2), (3, 3), (2, 4)];
        radix_sort(&mut v);
        assert_eq!(v, vec![(1, 2), (2, 4), (3, 1), (3, 3)], "this check");
    }

    #[test]
    fn s3g_c5_keys_that_differ_only_in_a_high_byte() {
        let mut v = vec![(0x0100_0000, 0), (0x00FF_FFFF, 1), (0xFF00_0000, 2), (0, 3)];
        radix_sort(&mut v);
        assert_eq!(v.iter().map(|r| r.1).collect::<Vec<_>>(), vec![3, 1, 0, 2], "this check");
    }

    #[test]
    fn s3g_c5_empty_single_and_already_sorted() {
        let mut e: Vec<(u32, u32)> = vec![];
        radix_sort(&mut e);
        assert!(e.is_empty(), "this check: expected `e.is_empty()`");
        let mut one = vec![(5, 5)];
        radix_sort(&mut one);
        assert_eq!(one, vec![(5, 5)], "this check");
        let mut sorted: Vec<(u32, u32)> = (0..100).map(|i| (i, i)).collect();
        let copy = sorted.clone();
        radix_sort(&mut sorted);
        assert_eq!(sorted, copy, "this check");
    }

    #[test]
    fn s3g_c5_equal_keys_and_reversed_input() {
        let mut same: Vec<(u32, u32)> = (0..50).map(|i| (7, i)).collect();
        let copy = same.clone();
        radix_sort(&mut same);
        assert_eq!(same, copy, "equal keys keep their input order");
        let mut rev: Vec<(u32, u32)> = (0..50).rev().map(|i| (i, i)).collect();
        radix_sort(&mut rev);
        assert!(rev.windows(2).all(|w| w[0].0 < w[1].0), "this check: expected `rev.windows(2).all(|w| w[0].0 < w[1].0)`");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equals the standard library's stable sort by key.
        #[test]
        fn s3g_c5_property_equals_a_stable_sort(keys in proptest::collection::vec(prop_oneof![0u32..8, any::<u32>()], 0..60)) {
            let mut rows: Vec<(u32, u32)> = keys.iter().enumerate().map(|(i, &k)| (k, i as u32)).collect();
            let mut want = rows.clone();
            want.sort_by_key(|r| r.0);
            radix_sort(&mut rows);
            prop_assert_eq!(rows, want);
        }
    }
}
// @@ challenge 3g-c5 end
