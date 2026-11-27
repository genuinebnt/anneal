//! Tests for module 3j: outer joins (RIGHT and FULL), filters and outer joins in the optimizer, and set operations.

use std::collections::BTreeMap;

use proptest::prelude::*;

use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::exception::{Exception, ExceptionType};
use bustub::common::result_writer::SimpleStreamWriter;

type Cell = Option<i64>;
/// A row of `a(x, y)` or `b(x, z)`.
type R2 = (Cell, Cell);

fn run(db: &BusTubInstance, sql: &str) -> Result<Vec<String>, Exception> {
    let mut out = String::new();
    db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, true, " "), None)?;
    Ok(out.lines().map(|l| l.trim_end().to_string()).collect())
}

fn rows(db: &BusTubInstance, sql: &str) -> Vec<String> {
    run(db, sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"))
}

/// The rows of a query as a bag: the lines, sorted.
fn bag(db: &BusTubInstance, sql: &str) -> Vec<String> {
    let mut r = rows(db, sql);
    r.sort();
    r
}

fn error_kind(db: &BusTubInstance, sql: &str) -> ExceptionType {
    match run(db, sql) {
        Ok(r) => panic!("{sql}: expected an error, got {r:?}"),
        Err(e) => e.kind,
    }
}

fn lit(v: &Cell) -> String {
    v.map_or("null".to_string(), |v| v.to_string())
}

fn show(v: &Cell) -> String {
    v.map_or("integer_null".to_string(), |v| v.to_string())
}

/// `a(x, y)` and `b(x, z)` holding the given rows.
fn db_ab(a: &[R2], b: &[R2]) -> BusTubInstance {
    let db = BusTubInstance::new(128);
    rows(&db, "create table a(x int, y int)");
    rows(&db, "create table b(x int, z int)");
    for (t, data) in [("a", a), ("b", b)] {
        if !data.is_empty() {
            let values: Vec<String> = data.iter().map(|(p, q)| format!("({}, {})", lit(p), lit(q))).collect();
            rows(&db, &format!("insert into {t} values {}", values.join(", ")));
        }
    }
    db
}

/// The same query with the optimizer's rules of this course switched off (BusTub's starter rules only): the answer to compare with.
fn plain(db: &BusTubInstance, sql: &str) -> Vec<String> {
    rows(db, "set force_optimizer_starter_rule = yes");
    let mut r = rows(db, sql);
    rows(db, "set force_optimizer_starter_rule = no");
    r.sort();
    r
}

fn plan(db: &BusTubInstance, sql: &str) -> String {
    rows(db, &format!("explain (optimizer) {sql}")).join("\n")
}

/// The join types in a plan, top to bottom: `["Left", "Inner"]`.
fn join_types(plan: &str) -> Vec<String> {
    plan.split("type=").skip(1).map(|s| s.split([',', ' ']).next().unwrap().to_string()).collect()
}

fn sample() -> (Vec<R2>, Vec<R2>) {
    (
        vec![(Some(1), Some(10)), (Some(2), Some(20)), (Some(3), Some(30)), (None, Some(40)), (Some(2), Some(21))],
        vec![(Some(2), Some(200)), (Some(3), Some(300)), (Some(4), Some(400)), (None, Some(500)), (Some(2), Some(201))],
    )
}

// ---- the join model ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Inner,
    Left,
    Right,
    Full,
}

impl Kind {
    fn sql(self) -> &'static str {
        match self {
            Kind::Inner => "join",
            Kind::Left => "left join",
            Kind::Right => "right join",
            Kind::Full => "full join",
        }
    }
}

/// `a JOIN b ON a.x = b.x` (a NULL key matches nothing), as printed lines `a.x a.y b.x b.z`, sorted.
fn model_join(kind: Kind, a: &[R2], b: &[R2]) -> Vec<String> {
    let line = |l: Option<&R2>, r: Option<&R2>| {
        let (ax, ay) = l.map_or((None, None), |t| *t);
        let (bx, bz) = r.map_or((None, None), |t| *t);
        format!("{} {} {} {}", show(&ax), show(&ay), show(&bx), show(&bz))
    };
    let mut out = vec![];
    let mut right_used = vec![false; b.len()];
    for l in a {
        let mut found = false;
        for (i, r) in b.iter().enumerate() {
            if l.0.is_some() && l.0 == r.0 {
                found = true;
                right_used[i] = true;
                out.push(line(Some(l), Some(r)));
            }
        }
        if !found && matches!(kind, Kind::Left | Kind::Full) {
            out.push(line(Some(l), None));
        }
    }
    if matches!(kind, Kind::Right | Kind::Full) {
        for (i, r) in b.iter().enumerate() {
            if !right_used[i] {
                out.push(line(None, Some(r)));
            }
        }
    }
    out.sort();
    out
}

fn arb_cell() -> impl Strategy<Value = Cell> {
    prop_oneof![3 => (0i64..5).prop_map(Some), 1 => Just(None)]
}

fn arb_rel(max: usize) -> impl Strategy<Value = Vec<R2>> {
    proptest::collection::vec((arb_cell(), arb_cell()), 0..max)
}

// ---- 3j-01 · right and full outer joins ------------------------------------------------------------------------------------

#[test]
fn s3j_01_a_right_join_keeps_every_right_row_and_pads_the_left_with_nulls() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    assert_eq!(bag(&db, "select a.x, a.y, b.x, b.z from a right join b on a.x = b.x"), model_join(Kind::Right, &a, &b), "RIGHT JOIN");
    assert!(
        bag(&db, "select a.x, a.y, b.x, b.z from a right join b on a.x = b.x").contains(&"integer_null integer_null 4 400".to_string()),
        "b's 4 has no partner: a's columns are NULL"
    );
}

#[test]
fn s3j_01_a_full_join_keeps_the_unmatched_rows_of_both_sides() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let got = bag(&db, "select a.x, a.y, b.x, b.z from a full join b on a.x = b.x");
    assert_eq!(got, model_join(Kind::Full, &a, &b), "FULL JOIN");
    assert!(got.contains(&"1 10 integer_null integer_null".to_string()), "a's 1 has no partner");
    assert!(got.contains(&"integer_null integer_null 4 400".to_string()), "b's 4 has no partner");
}

#[test]
fn s3j_01_null_keys_never_match_each_other_but_are_still_kept() {
    let a = vec![(None, Some(1))];
    let b = vec![(None, Some(2))];
    let db = db_ab(&a, &b);
    assert_eq!(
        bag(&db, "select a.y, b.z from a full join b on a.x = b.x"),
        ["1 integer_null", "integer_null 2"],
        "two NULL keys: two unmatched rows, not one match"
    );
    assert_eq!(
        bag(&db, "select a.y, b.z from a right join b on a.x = b.x"),
        ["integer_null 2"],
        "only the right row survives a RIGHT join"
    );
}

#[test]
fn s3j_01_a_row_that_matches_several_times_appears_once_per_match_and_is_not_also_padded() {
    let a = vec![(Some(2), Some(1))];
    let b = vec![(Some(2), Some(10)), (Some(2), Some(20)), (Some(2), Some(30))];
    let db = db_ab(&a, &b);
    assert_eq!(bag(&db, "select a.y, b.z from a full join b on a.x = b.x"), ["1 10", "1 20", "1 30"], "three matches, no padded rows");
    assert_eq!(bag(&db, "select a.y, b.z from b full join a on a.x = b.x"), ["1 10", "1 20", "1 30"], "the same with the sides swapped");
}

#[test]
fn s3j_01_the_condition_may_be_any_predicate_not_only_an_equality() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let got = bag(&db, "select count(*) from a full join b on a.x < b.x");
    let mut matches = 0;
    let (mut lu, mut ru) = (vec![false; a.len()], vec![false; b.len()]);
    for (i, l) in a.iter().enumerate() {
        for (j, r) in b.iter().enumerate() {
            if matches!((l.0, r.0), (Some(x), Some(y)) if x < y) {
                matches += 1;
                lu[i] = true;
                ru[j] = true;
            }
        }
    }
    let want = matches + lu.iter().filter(|u| !**u).count() + ru.iter().filter(|u| !**u).count();
    assert_eq!(got, [want.to_string()], "pairs that satisfy a.x < b.x, plus every row that has none");
    let on_extra = bag(&db, "select a.y, b.z from a right join b on a.x = b.x and b.z > 250");
    assert_eq!(
        on_extra.len(),
        5,
        "b has five rows; a condition in ON can only make a row unmatched, never remove it from a RIGHT join: {on_extra:?}"
    );
}

#[test]
fn s3j_01_empty_sides() {
    let (a, b) = sample();
    let empty: Vec<R2> = vec![];
    let db = db_ab(&empty, &b);
    assert_eq!(bag(&db, "select a.x, b.z from a full join b on a.x = b.x").len(), b.len(), "an empty left side: every right row, padded");
    assert_eq!(bag(&db, "select a.x, b.z from a left join b on a.x = b.x").len(), 0, "and a LEFT join of nothing is nothing");
    let db = db_ab(&a, &empty);
    assert_eq!(bag(&db, "select a.x, b.z from a right join b on a.x = b.x").len(), 0, "an empty right side: a RIGHT join is empty");
    assert_eq!(bag(&db, "select a.x, b.z from a full join b on a.x = b.x").len(), a.len(), "a FULL join keeps the left rows");
    let db = db_ab(&empty, &empty);
    assert_eq!(bag(&db, "select a.x, b.z from a full join b on a.x = b.x").len(), 0, "nothing joined with nothing");
}

#[test]
fn s3j_01_sides_bigger_than_a_batch() {
    let a: Vec<R2> = (0..70).map(|i| (Some(i % 30), Some(i))).collect();
    let b: Vec<R2> = (20..90).map(|i| (Some(i % 40), Some(i))).collect();
    let db = db_ab(&a, &b);
    for kind in [Kind::Right, Kind::Full] {
        let sql = format!("select a.x, a.y, b.x, b.z from a {} b on a.x = b.x", kind.sql());
        assert_eq!(bag(&db, &sql), model_join(kind, &a, &b), "{kind:?}");
    }
}

#[test]
fn s3j_01_a_second_execution_of_the_same_join_gives_the_same_rows() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let sql = "select a.y, b.z from a full join b on a.x = b.x";
    assert_eq!(bag(&db, sql), bag(&db, sql), "the executor can be started again (as the inner side of another join is)");
    // a non-equality condition makes the outer loop start the FULL join again for every row of its left side
    let full = model_join(Kind::Full, &a, &b).len();
    let outer_rows = b.iter().filter(|r| r.0.map_or(false, |x| x < 100)).count();
    let nested =
        rows(&db, "select count(*) from b as bb join (select a.y as p, b.z as q from a full join b on a.x = b.x) as t on bb.x < 100");
    assert_eq!(
        nested,
        [(outer_rows * full).to_string()],
        "a FULL join used as the inner side of another join gives all its rows every time it is started"
    );
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: LEFT, RIGHT and FULL joins on random relations (with NULLs and duplicates) are the model's.
    #[test]
    fn s3j_01_property_outer_joins_are_the_models(a in arb_rel(9), b in arb_rel(9)) {
        let db = db_ab(&a, &b);
        for kind in [Kind::Left, Kind::Right, Kind::Full] {
            let sql = format!("select a.x, a.y, b.x, b.z from a {} b on a.x = b.x", kind.sql());
            prop_assert_eq!(bag(&db, &sql), model_join(kind, &a, &b), "{:?}", kind);
        }
    }
}

// ---- 3j-02 · pushing filters below joins -----------------------------------------------------------------------------------

#[test]
fn s3j_02_filters_on_one_side_of_an_inner_join_move_to_that_side() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let p = plan(&db, "select a.x, b.z from a join b on a.x = b.x where a.y > 15 and b.z < 450");
    assert!(p.contains("SeqScan { table=a, filter="), "the filter on a is applied while scanning a:\n{p}");
    assert!(p.contains("SeqScan { table=b, filter="), "and the one on b while scanning b:\n{p}");
    assert!(!p.contains("Filter {"), "nothing is left to filter above the join:\n{p}");
    let p = plan(&db, "select a.x, b.z from a, b where a.x = b.x and a.y > 15 and b.z < 450");
    assert!(
        p.contains("SeqScan { table=a, filter=") && p.contains("SeqScan { table=b, filter="),
        "the same for a join written with a comma:\n{p}"
    );
}

#[test]
fn s3j_02_a_filter_that_reads_both_sides_stays_above_the_join() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let p = plan(&db, "select a.x from a join b on a.x = b.x where a.y + b.z > 100 and a.y > 15");
    assert!(p.contains("SeqScan { table=a, filter="), "the part about a alone moves:\n{p}");
    assert!(p.contains("Filter {") || p.contains("predicate="), "the part about both stays with the join:\n{p}");
    assert_eq!(
        bag(&db, "select a.x, a.y, b.z from a join b on a.x = b.x where a.y + b.z > 100 and a.y > 15"),
        plain(&db, "select a.x, a.y, b.z from a join b on a.x = b.x where a.y + b.z > 100 and a.y > 15"),
        "and the answer does not change"
    );
}

#[test]
fn s3j_02_a_left_join_takes_filters_on_its_left_side_only() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let p = plan(&db, "select a.x, b.z from a left join b on a.x = b.x where a.y > 15");
    assert!(p.contains("SeqScan { table=a, filter="), "a filter on the preserved side moves below the join:\n{p}");
    assert!(!p.contains("Filter {"), "and leaves nothing above:\n{p}");
    // `b.z IS NULL` finds the rows that had no partner: moved below the join it would find no rows at all
    let sql = "select a.x, a.y from a left join b on a.x = b.x where b.z is null";
    let p = plan(&db, sql);
    assert!(p.contains("Filter { predicate="), "a filter on the padded side must stay above the join:\n{p}");
    assert_eq!(bag(&db, sql), plain(&db, sql), "the rows without a partner");
    assert_eq!(bag(&db, sql), ["1 10", "integer_null 40"], "a's 1 and a's NULL key have no partner in b");
}

#[test]
fn s3j_02_a_right_join_takes_filters_on_its_right_side_only() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let p = plan(&db, "select a.x, b.z from a right join b on a.x = b.x where b.z > 250");
    assert!(p.contains("SeqScan { table=b, filter="), "a filter on the preserved (right) side moves below the join:\n{p}");
    let sql = "select a.y, b.z from a right join b on a.x = b.x where a.y is null";
    assert!(plan(&db, sql).contains("Filter { predicate="), "a filter on the padded (left) side stays");
    assert_eq!(bag(&db, sql), plain(&db, sql), "the right rows without a partner");
}

#[test]
fn s3j_02_a_full_join_takes_no_filter() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let sql = "select a.x, b.x from a full join b on a.x = b.x where a.y is null or b.z is null";
    let p = plan(&db, sql);
    assert!(p.contains("Filter { predicate="), "a filter above a FULL join cannot move:\n{p}");
    assert!(!p.contains("filter="), "no scan received it:\n{p}");
    assert_eq!(bag(&db, sql), plain(&db, sql), "the answer");
}

#[test]
fn s3j_02_a_filter_goes_down_through_several_joins_to_its_table() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    rows(&db, "create table c(x int, w int)");
    rows(&db, "insert into c values (1, 5), (2, 6), (3, 7)");
    let sql = "select a.x, b.z, c.w from a join b on a.x = b.x join c on c.x = a.x where a.y > 15 and c.w < 7 and b.z > 100";
    let p = plan(&db, sql);
    for table in ["a", "b", "c"] {
        assert!(p.contains(&format!("SeqScan {{ table={table}, filter=")), "the filter on {table} reached its scan:\n{p}");
    }
    assert_eq!(bag(&db, sql), plain(&db, sql), "and the answer is the same");
}

#[test]
fn s3j_02_a_filter_that_reads_no_column_stays_put() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    for sql in ["select a.x from a left join b on a.x = b.x where 1 = 2", "select a.x from a join b on a.x = b.x where 1 = 1"] {
        assert_eq!(bag(&db, sql), plain(&db, sql), "{sql}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: whatever the join and the filter, the optimized query returns what the plain plan returns.
    #[test]
    fn s3j_02_property_pushing_a_filter_never_changes_the_answer(
        a in arb_rel(8), b in arb_rel(8), k in 0usize..4, p in 0usize..8, c1 in 0i64..5, c2 in 0i64..5,
    ) {
        let db = db_ab(&a, &b);
        let kind = [Kind::Inner, Kind::Left, Kind::Right, Kind::Full][k];
        let conds = ["a.y > {1}", "b.z < {2}", "a.y is null", "b.z is null", "a.y > {1} and b.z < {2}", "a.y > {1} or b.z < {2}", "a.x = {1}", "a.y + b.z > {2}"];
        let cond = conds[p].replace("{1}", &c1.to_string()).replace("{2}", &c2.to_string());
        let sql = format!("select a.x, a.y, b.x, b.z from a {} b on a.x = b.x where {cond}", kind.sql());
        prop_assert_eq!(bag(&db, &sql), plain(&db, &sql), "{}", sql);
    }
}

// ---- 3j-03 · outer joins that are really inner joins -----------------------------------------------------------------------

#[test]
fn s3j_03_a_left_join_filtered_on_the_right_side_is_an_inner_join() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    for cond in [
        "b.z > 250",
        "b.z is not null",
        "b.z = 300",
        "b.z + 1 > 5",
        "b.z > 250 and a.y > 5",
        "b.z <> 0",
        "b.z > 250 or (b.z < 5 and a.y > 5)",
    ] {
        let sql = format!("select a.x, b.z from a left join b on a.x = b.x where {cond}");
        assert_eq!(
            join_types(&plan(&db, &sql)),
            ["Inner"],
            "{cond}: a row padded with NULLs fails the filter, so the padding is pointless"
        );
        assert_eq!(bag(&db, &sql), plain(&db, &sql), "{cond}: and the answer is the same");
    }
}

#[test]
fn s3j_03_a_filter_that_a_padded_row_can_pass_leaves_the_join_alone() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    for cond in ["b.z is null", "b.z > 250 or a.y > 5", "a.y > 15", "coalesce(b.z, 0) > 250", "b.z is null or b.z > 250"] {
        let sql = format!("select a.x, b.z from a left join b on a.x = b.x where {cond}");
        assert_eq!(join_types(&plan(&db, &sql)), ["Left"], "{cond}: a padded row can pass it");
        assert_eq!(bag(&db, &sql), plain(&db, &sql), "{cond}: the answer");
    }
}

#[test]
fn s3j_03_a_right_join_filtered_on_the_left_side_is_an_inner_join() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let sql = "select a.x, b.z from a right join b on a.x = b.x where a.y > 15";
    assert_eq!(join_types(&plan(&db, sql)), ["Inner"], "the padded rows have a NULL a.y");
    assert_eq!(bag(&db, sql), plain(&db, sql), "the answer");
    let keep = "select a.x, b.z from a right join b on a.x = b.x where b.z > 250";
    assert_eq!(join_types(&plan(&db, keep)), ["Right"], "a filter on the preserved side does not change the join");
}

#[test]
fn s3j_03_a_full_join_loses_the_side_the_filter_rejects() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let cases =
        [("b.z > 250", "Right"), ("a.y > 15", "Left"), ("b.z > 250 and a.y > 15", "Inner"), ("a.y is null or b.z is null", "Outer")];
    for (cond, want) in cases {
        let sql = format!("select a.x, b.z from a full join b on a.x = b.x where {cond}");
        assert_eq!(join_types(&plan(&db, &sql)), [want], "{cond}");
        assert_eq!(bag(&db, &sql), plain(&db, &sql), "{cond}: the answer");
    }
}

#[test]
fn s3j_03_only_the_conditions_that_fail_on_nulls_count() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    // IS NULL, NOT, and an OR with one free side can all be TRUE for a padded row
    for cond in ["not (b.z > 250)", "b.z is null and a.y > 5", "b.z > 250 or b.z is null"] {
        let sql = format!("select a.x, b.z from a left join b on a.x = b.x where {cond}");
        assert_eq!(bag(&db, &sql), plain(&db, &sql), "{cond}");
    }
    assert_eq!(
        join_types(&plan(&db, "select a.x from a left join b on a.x = b.x where b.z is null and a.y > 5")),
        ["Left"],
        "IS NULL keeps the join outer"
    );
}

#[test]
fn s3j_03_after_the_join_is_inner_the_filter_can_move_into_the_scan() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let p = plan(&db, "select a.x, b.z from a left join b on a.x = b.x where b.z > 250 and a.y > 5");
    assert_eq!(join_types(&p), ["Inner"], "the join became inner");
    assert!(
        p.contains("SeqScan { table=b, filter=") && p.contains("SeqScan { table=a, filter="),
        "and both filters reached their scans:\n{p}"
    );
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: simplifying an outer join never changes the answer.
    #[test]
    fn s3j_03_property_simplification_never_changes_the_answer(a in arb_rel(8), b in arb_rel(8), k in 1usize..4, p in 0usize..9, c in 0i64..5) {
        let db = db_ab(&a, &b);
        let kind = [Kind::Inner, Kind::Left, Kind::Right, Kind::Full][k];
        let conds = ["b.z > {c}", "a.y > {c}", "b.z is not null", "a.y is not null", "b.z + a.y > {c}", "b.z > {c} or a.y > {c}", "not (b.z > {c})", "coalesce(a.y, 0) > {c}", "b.z = {c} and a.y <> {c}"];
        let cond = conds[p].replace("{c}", &c.to_string());
        let sql = format!("select a.x, a.y, b.x, b.z from a {} b on a.x = b.x where {cond}", kind.sql());
        prop_assert_eq!(bag(&db, &sql), plain(&db, &sql), "{}", sql);
    }
}

// ---- 3j-04 · UNION ALL ------------------------------------------------------------------------------------------------------

/// The rows of a table as a bag of printed lines `x y`.
fn lines(r: &[R2]) -> Vec<String> {
    let mut v: Vec<String> = r.iter().map(|(p, q)| format!("{} {}", show(p), show(q))).collect();
    v.sort();
    v
}

#[test]
fn s3j_04_union_all_is_both_inputs_with_every_duplicate() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let mut want = lines(&a);
    want.extend(lines(&b));
    want.sort();
    assert_eq!(bag(&db, "select x, y from a union all select x, z from b"), want, "every row of a, then every row of b");
    assert_eq!(rows(&db, "select x, y from a union all select x, y from a").len(), 2 * a.len(), "the same table twice: twice the rows");
}

#[test]
fn s3j_04_the_left_rows_come_before_the_right_rows() {
    let db = db_ab(&[(Some(1), Some(1)), (Some(2), Some(2))], &[(Some(3), Some(3)), (Some(4), Some(4))]);
    assert_eq!(rows(&db, "select x from a union all select x from b"), ["1", "2", "3", "4"], "UNION ALL does not reorder: it concatenates");
}

#[test]
fn s3j_04_a_chain_of_union_all_and_constants() {
    let db = db_ab(&[(Some(1), Some(1))], &[(Some(2), Some(2))]);
    assert_eq!(bag(&db, "select x from a union all select x from b union all select 3"), ["1", "2", "3"], "three parts");
    assert_eq!(bag(&db, "select 1 union all select 2 union all select 1"), ["1", "1", "2"], "constants, duplicates kept");
}

#[test]
fn s3j_04_the_two_sides_must_agree_on_columns() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    assert_eq!(error_kind(&db, "select x, y from a union all select x from b"), ExceptionType::Invalid, "different number of columns");
    assert_eq!(error_kind(&db, "select x from a union all select 'x'"), ExceptionType::MismatchType, "an integer and a string");
    let e = run(&db, "select x, y from a union all select x from b").unwrap_err();
    assert!(e.message.contains("UNION"), "the message names the operator: {e:?}");
    assert!(e.message.contains("2") && e.message.contains("1"), "and the two column counts: {e:?}");
}

#[test]
fn s3j_04_the_result_can_be_used_as_a_table() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    assert_eq!(
        bag(&db, "select t.x, count(*) from (select x from a union all select x from b) as t group by t.x"),
        {
            let mut counts: BTreeMap<Cell, usize> = BTreeMap::new();
            for r in a.iter().chain(&b) {
                *counts.entry(r.0).or_insert(0) += 1;
            }
            let mut v: Vec<String> = counts.iter().map(|(k, n)| format!("{} {n}", show(k))).collect();
            v.sort();
            v
        },
        "a union as a table in FROM, with a group by"
    );
    assert_eq!(bag(&db, "select t.x from (select x from a union all select x from b) as t where t.x > 2"), ["3", "3", "4"], "and filtered");
}

#[test]
fn s3j_04_the_columns_are_named_after_the_left_side() {
    let db = db_ab(&[(Some(1), Some(10))], &[(Some(2), Some(20))]);
    assert_eq!(
        bag(&db, "select t.p from (select x as p from a union all select z from b) as t"),
        ["1", "20"],
        "the left query's alias is the column name"
    );
}

#[test]
fn s3j_04_union_all_feeds_insert_and_a_join() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    rows(&db, "create table c(x int, y int)");
    assert_eq!(
        rows(&db, "insert into c select x, y from a union all select x, z from b"),
        [(a.len() + b.len()).to_string()],
        "INSERT ... SELECT of a union"
    );
    assert_eq!(rows(&db, "select count(*) from c"), [(a.len() + b.len()).to_string()], "all rows arrived");
    assert_eq!(
        bag(&db, "select count(*) from (select x as k from a union all select x from b) as t join a on t.k = a.x").len(),
        1,
        "a union joined with a table"
    );
}

#[test]
fn s3j_04_more_rows_than_a_batch_and_empty_sides() {
    let big: Vec<R2> = (0..75).map(|i| (Some(i), Some(i))).collect();
    let db = db_ab(&big, &big);
    assert_eq!(rows(&db, "select count(*) from (select x from a union all select x from b) as t"), ["150"], "two sides of 75 rows each");
    let empty: Vec<R2> = vec![];
    let db = db_ab(&empty, &big);
    assert_eq!(rows(&db, "select x from a union all select x from b").len(), 75, "an empty left side");
    let db = db_ab(&big, &empty);
    assert_eq!(rows(&db, "select x from a union all select x from b").len(), 75, "an empty right side");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: UNION ALL of two random tables is the concatenation of their rows.
    #[test]
    fn s3j_04_property_union_all_concatenates(a in arb_rel(10), b in arb_rel(10)) {
        let db = db_ab(&a, &b);
        let mut want = lines(&a);
        want.extend(lines(&b));
        want.sort();
        prop_assert_eq!(bag(&db, "select x, y from a union all select x, z from b"), want);
    }
}

// ---- 3j-05 · UNION, INTERSECT and EXCEPT ------------------------------------------------------------------------------------

type Counts = BTreeMap<R2, usize>;

fn counts(r: &[R2]) -> Counts {
    let mut m = Counts::new();
    for row in r {
        *m.entry(*row).or_insert(0) += 1;
    }
    m
}

/// The set operation on bags, as the standard defines it, printed as sorted lines.
fn model_set(op: &str, all: bool, a: &[R2], b: &[R2]) -> Vec<String> {
    let (ca, cb) = (counts(a), counts(b));
    let mut out: Vec<R2> = vec![];
    let keys: std::collections::BTreeSet<R2> = ca.keys().chain(cb.keys()).copied().collect();
    for k in keys {
        let (na, nb) = (ca.get(&k).copied().unwrap_or(0), cb.get(&k).copied().unwrap_or(0));
        let n = match (op, all) {
            ("union", true) => na + nb,
            ("union", false) => usize::from(na + nb > 0),
            ("intersect", true) => na.min(nb),
            ("intersect", false) => usize::from(na > 0 && nb > 0),
            ("except", true) => na.saturating_sub(nb),
            ("except", false) => usize::from(na > 0 && nb == 0),
            _ => unreachable!(),
        };
        out.extend(std::iter::repeat(k).take(n));
    }
    lines(&out)
}

fn set_sql(op: &str, all: bool) -> String {
    format!("select x, y from a {op} {}select x, z from b", if all { "all " } else { "" })
}

#[test]
fn s3j_05_union_without_all_removes_duplicates() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    assert_eq!(bag(&db, &set_sql("union", false)), model_set("union", false, &a, &b), "UNION");
    let twice = bag(&db, "select x, y from a union select x, y from a");
    assert_eq!(twice, model_set("union", false, &a, &a), "a table united with itself is its distinct rows");
}

#[test]
fn s3j_05_two_nulls_are_the_same_row_here() {
    let db = db_ab(&[(None, Some(1)), (None, Some(1))], &[(None, Some(1))]);
    assert_eq!(bag(&db, &set_sql("union", false)), ["integer_null 1"], "UNION: the three rows are one");
    assert_eq!(
        bag(&db, &set_sql("intersect", false)),
        ["integer_null 1"],
        "INTERSECT finds the NULL row on both sides (a WHERE a.x = b.x would not)"
    );
    assert_eq!(bag(&db, &set_sql("except", false)), Vec::<String>::new(), "EXCEPT removes it");
}

#[test]
fn s3j_05_intersect_and_except_without_all() {
    let (a, b) = sample();
    let db =
        db_ab(&[(Some(1), Some(1)), (Some(2), Some(2)), (Some(2), Some(2)), (Some(3), Some(3))], &[(Some(2), Some(2)), (Some(3), Some(9))]);
    assert_eq!(bag(&db, &set_sql("intersect", false)), ["2 2"], "rows on both sides, once");
    assert_eq!(bag(&db, &set_sql("except", false)), ["1 1", "3 3"], "rows of a that are not in b, once each");
    let db = db_ab(&a, &b);
    for op in ["intersect", "except"] {
        assert_eq!(bag(&db, &set_sql(op, false)), model_set(op, false, &a, &b), "{op}");
    }
}

#[test]
fn s3j_05_all_counts_the_rows() {
    let a = vec![(Some(1), Some(1)), (Some(1), Some(1)), (Some(1), Some(1)), (Some(2), Some(2))];
    let b = vec![(Some(1), Some(1)), (Some(1), Some(1)), (Some(3), Some(3))];
    let db = db_ab(&a, &b);
    assert_eq!(bag(&db, &set_sql("intersect", true)), ["1 1", "1 1"], "INTERSECT ALL keeps the smaller count: min(3, 2) = 2");
    assert_eq!(
        bag(&db, &set_sql("except", true)),
        ["1 1", "2 2"],
        "EXCEPT ALL keeps the difference of the counts: 3 - 2 = 1 (and never below 0)"
    );
    assert_eq!(bag(&db, &set_sql("union", true)).len(), 7, "UNION ALL adds them");
}

#[test]
fn s3j_05_the_order_of_the_sides_matters_for_except_only() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let swapped = |op: &str| format!("select x, z from b {op} select x, y from a");
    for op in ["union", "intersect"] {
        assert_eq!(bag(&db, &set_sql(op, false)), bag(&db, &swapped(op)), "{op} is symmetric");
    }
    assert_eq!(bag(&db, &swapped("except")), model_set("except", false, &b, &a), "b EXCEPT a");
    assert_ne!(bag(&db, &set_sql("except", false)), bag(&db, &swapped("except")), "and it is not symmetric");
}

#[test]
fn s3j_05_strings_and_mixed_rows() {
    let db = BusTubInstance::new(64);
    rows(&db, "create table s(name varchar(20), n int)");
    rows(&db, "create table t(name varchar(10), n int)");
    rows(&db, "insert into s values ('ann', 1), ('bob', 2), ('ann', 1)");
    rows(&db, "insert into t values ('ann', 1), ('cy', 3)");
    assert_eq!(
        bag(&db, "select name, n from s union select name, n from t"),
        ["ann 1", "bob 2", "cy 3"],
        "strings of different declared lengths"
    );
    assert_eq!(bag(&db, "select name, n from s intersect select name, n from t"), ["ann 1"], "INTERSECT on strings");
    assert_eq!(
        bag(&db, "select name, n from s except all select name, n from t"),
        ["ann 1", "bob 2"],
        "EXCEPT ALL: one of the two ann rows is cancelled"
    );
}

#[test]
fn s3j_05_a_set_operation_of_set_operations_runs_in_a_subquery() {
    let (a, b) = sample();
    let db = db_ab(&a, &b);
    let sql = "select count(*) from (select x from a union select x from b) as t";
    let mut keys: Vec<Cell> = a.iter().chain(&b).map(|r| r.0).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(rows(&db, sql), [keys.len().to_string()], "the distinct keys of both tables, NULL included once");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: every set operation, with and without ALL, is the bag arithmetic of the standard.
    #[test]
    fn s3j_05_property_set_operations_are_bag_arithmetic(a in arb_rel(10), b in arb_rel(10)) {
        let db = db_ab(&a, &b);
        for op in ["union", "intersect", "except"] {
            for all in [false, true] {
                prop_assert_eq!(bag(&db, &set_sql(op, all)), model_set(op, all, &a, &b), "{} all={}", op, all);
            }
        }
    }
}

// ---- 3j-06 · precedence, parentheses, ORDER BY and LIMIT ------------------------------------------------------------------

fn db_abc(a: &[R2], b: &[R2], c: &[R2]) -> BusTubInstance {
    let db = db_ab(a, b);
    rows(&db, "create table c(x int, w int)");
    if !c.is_empty() {
        let values: Vec<String> = c.iter().map(|(p, q)| format!("({}, {})", lit(p), lit(q))).collect();
        rows(&db, &format!("insert into c values {}", values.join(", ")));
    }
    db
}

fn single(x: &[i64]) -> Vec<R2> {
    x.iter().map(|v| (Some(*v), Some(*v))).collect()
}

#[test]
fn s3j_06_intersect_binds_tighter_than_union() {
    let db = db_abc(&single(&[1, 2]), &single(&[2, 3]), &single(&[3, 4]));
    // a UNION (b INTERSECT c) = {1,2} + {3} ; (a UNION b) INTERSECT c would be {3}
    assert_eq!(bag(&db, "select x from a union select x from b intersect select x from c"), ["1", "2", "3"], "a UNION (b INTERSECT c)");
    assert_eq!(bag(&db, "select x from a intersect select x from b union select x from c"), ["2", "3", "4"], "(a INTERSECT b) UNION c");
}

#[test]
fn s3j_06_union_and_except_go_left_to_right() {
    let db = db_abc(&single(&[1, 2, 3]), &single(&[2]), &single(&[3, 4]));
    assert_eq!(bag(&db, "select x from a except select x from b union select x from c"), ["1", "3", "4"], "(a EXCEPT b) UNION c");
    assert_eq!(bag(&db, "select x from a union select x from c except select x from b"), ["1", "3", "4"], "(a UNION c) EXCEPT b");
    assert_eq!(
        bag(&db, "select x from a except select x from b except select x from c"),
        ["1"],
        "(a EXCEPT b) EXCEPT c, not a EXCEPT (b EXCEPT c)"
    );
}

#[test]
fn s3j_06_parentheses_override_the_precedence() {
    let db = db_abc(&single(&[1, 2]), &single(&[2, 3]), &single(&[3, 4]));
    assert_eq!(
        bag(&db, "select x from a union (select x from b intersect select x from c)"),
        ["1", "2", "3"],
        "parentheses around the second operand"
    );
    assert_eq!(bag(&db, "(select x from a union select x from b) intersect select x from c"), ["3"], "parentheses around the first");
    assert_eq!(bag(&db, "select x from a except (select x from b union select x from c)"), ["1"], "a EXCEPT (b UNION c)");
}

#[test]
fn s3j_06_order_by_and_limit_apply_to_the_whole_result() {
    let db = db_abc(&single(&[5, 1, 3]), &single(&[4, 2, 6]), &[]);
    assert_eq!(
        rows(&db, "select x from a union all select x from b order by x"),
        ["1", "2", "3", "4", "5", "6"],
        "ORDER BY after the last operand sorts the union"
    );
    assert_eq!(rows(&db, "select x from a union all select x from b order by x desc limit 2"), ["6", "5"], "with LIMIT");
    assert_eq!(rows(&db, "select x from a union all select x from b order by x limit 2 offset 3"), ["4", "5"], "and OFFSET");
}

#[test]
fn s3j_06_a_parenthesised_operand_keeps_its_own_order_by_and_limit() {
    let db = db_abc(&single(&[5, 1, 3]), &single(&[4, 2, 6]), &[]);
    assert_eq!(
        rows(&db, "(select x from a order by x limit 1) union all (select x from b order by x desc limit 1)"),
        ["1", "6"],
        "the smallest of a and the largest of b"
    );
    assert_eq!(
        rows(&db, "(select x from a order by x limit 2) union all (select x from b order by x limit 2) order by x desc"),
        ["4", "3", "2", "1"],
        "each side limited, then the whole sorted"
    );
}

#[test]
fn s3j_06_order_by_names_the_columns_of_the_left_side() {
    let db = db_abc(&[(Some(2), Some(20)), (Some(1), Some(10))], &[(Some(3), Some(30))], &[]);
    assert_eq!(rows(&db, "select x as k from a union all select x from b order by k"), ["1", "2", "3"], "an alias of the first select");
    assert!(run(&db, "select x as k from a union all select x from b order by nope").is_err(), "an unknown column is an error");
}

#[test]
fn s3j_06_with_applies_to_every_operand() {
    let db = db_abc(&single(&[1, 2]), &single(&[2, 3]), &[]);
    assert_eq!(bag(&db, "with t as (select x from a) select x from t union select x from b"), ["1", "2", "3"], "a CTE used on the left");
    assert_eq!(bag(&db, "with t as (select x from b) select x from a union select x from t"), ["1", "2", "3"], "and on the right");
}

#[test]
fn s3j_06_distinct_is_accepted_after_the_operator_and_nesting_works() {
    let db = db_abc(&single(&[1, 1, 2]), &single(&[2, 3]), &single(&[9]));
    assert_eq!(bag(&db, "select x from a union distinct select x from b"), ["1", "2", "3"], "UNION DISTINCT is UNION");
    assert_eq!(bag(&db, "((select x from a) union ((select x from b) union (select x from c)))"), ["1", "2", "3", "9"], "parentheses nest");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: a chain of three operands with random operators is evaluated with INTERSECT first and the others left to right.
    #[test]
    fn s3j_06_property_precedence_matches_the_model(a in proptest::collection::vec(0i64..5, 0..6), b in proptest::collection::vec(0i64..5, 0..6), c in proptest::collection::vec(0i64..5, 0..6), o1 in 0usize..3, o2 in 0usize..3, all in any::<bool>()) {
        let db = db_abc(&single(&a), &single(&b), &single(&c));
        let ops = ["union", "intersect", "except"];
        let kw = if all { " all" } else { "" };
        let sql = format!("select x from a {}{kw} select x from b {}{kw} select x from c", ops[o1], ops[o2]);
        let to_rows = |v: &Vec<String>| -> Vec<R2> { v.iter().map(|s| { let n: i64 = s.parse().unwrap(); (Some(n), Some(n)) }).collect() };
        let one = |op: &str, l: &[R2], r: &[R2]| -> Vec<R2> { to_rows(&model_set(op, all, l, r).iter().map(|s| s.split(' ').next().unwrap().to_string()).collect()) };
        let (sa, sb, sc) = (single(&a), single(&b), single(&c));
        let want = if o2 == 1 && o1 != 1 {
            // the second operator binds tighter: a o1 (b INTERSECT c)
            one(ops[o1], &sa, &one("intersect", &sb, &sc))
        } else {
            // left to right (also INTERSECT before another INTERSECT)
            one(ops[o2], &one(ops[o1], &sa, &sb), &sc)
        };
        let mut want: Vec<String> = want.iter().map(|r| show(&r.0)).collect();
        want.sort();
        prop_assert_eq!(bag(&db, &sql), want, "{}", sql);
    }
}

// ---- 3j-07 · boss: joins and sets against their laws ---------------------------------------------------------------------

#[path = "slt/mod.rs"]
mod slt;

#[test]
fn s3j_07_the_joins_and_sets_script() {
    slt::run_slt("sql_joins_and_sets.slt", 128);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 60, failure_persistence: None, ..ProptestConfig::default() })]

    /// Boss property: the identities that relate the joins to each other and to the set operations.
    #[test]
    fn s3j_07_property_the_join_identities(a in arb_rel(8), b in arb_rel(8)) {
        let db = db_ab(&a, &b);
        let q = |kind: &str| bag(&db, &format!("select a.x, a.y, b.x, b.z from a {kind} b on a.x = b.x"));
        let (inner, left, right, full) = (q("join"), q("left join"), q("right join"), q("full join"));
        // a LEFT join is the INNER join plus the left rows that found no partner, padded
        let unmatched_left: Vec<String> = a.iter().filter(|l| !b.iter().any(|r| l.0.is_some() && l.0 == r.0)).map(|l| format!("{} {} integer_null integer_null", show(&l.0), show(&l.1))).collect();
        let mut want = inner.clone();
        want.extend(unmatched_left);
        want.sort();
        prop_assert_eq!(&left, &want, "LEFT = INNER + unmatched left");
        // a RIGHT join is a LEFT join of the swapped tables, with the columns put back
        let swapped = bag(&db, "select a.x, a.y, b.x, b.z from b left join a on a.x = b.x");
        prop_assert_eq!(&right, &swapped, "a RIGHT JOIN b is b LEFT JOIN a");
        // FULL = LEFT union ALL the right rows that found no partner
        let unmatched_right: Vec<String> = right.iter().filter(|l| l.starts_with("integer_null integer_null")).cloned().collect();
        let mut want = left.clone();
        want.extend(unmatched_right);
        want.sort();
        prop_assert_eq!(&full, &want, "FULL = LEFT + unmatched right");
        // the set identities
        let s = |op: &str| bag(&db, &format!("select x, y from a {op} select x, z from b"));
        let (u, ua, i, e) = (s("union"), s("union all"), s("intersect"), s("except"));
        let mut dedup = ua.clone();
        dedup.dedup();
        prop_assert_eq!(&u, &dedup, "UNION = distinct UNION ALL");
        let mut back = i.clone();
        back.extend(e.clone());
        back.sort();
        let mut left_distinct: Vec<String> = lines(&a);
        left_distinct.dedup();
        prop_assert_eq!(back, left_distinct, "(a INTERSECT b) + (a EXCEPT b) = distinct a");
        prop_assert_eq!(s("intersect"), bag(&db, "select x, z from b intersect select x, y from a"), "INTERSECT is symmetric");
    }

    /// Boss property: random filters over random joins, all four kinds, are what the plain plan returns.
    #[test]
    fn s3j_07_property_optimized_equals_plain(a in arb_rel(7), b in arb_rel(7), k in 0usize..4, c in 0i64..5, f in 0usize..6) {
        let db = db_ab(&a, &b);
        let kind = [Kind::Inner, Kind::Left, Kind::Right, Kind::Full][k];
        let conds = ["a.y > {c}", "b.z > {c}", "a.y is null", "b.z is not null", "a.y > {c} and b.z > {c}", "a.y > {c} or b.z is null"];
        let cond = conds[f].replace("{c}", &c.to_string());
        let sql = format!("select a.y, b.z from a {} b on a.x = b.x where {cond} union all select a.y, b.z from a {} b on a.x = b.x where a.x = {c}", kind.sql(), kind.sql());
        prop_assert_eq!(bag(&db, &sql), plain(&db, &sql), "{}", sql);
    }
}

// @@ challenge 3j-c1 begin
mod ch_3j_c1 {

    use super::*;
    use bustub::execution::hash_outer_join::{hash_join, JoinKind, Row};

    type Pair = (Option<i64>, Option<i64>);

    fn model(left: &[Row], right: &[Row], kind: JoinKind) -> Vec<Pair> {
        let mut out = vec![];
        let mut used = vec![false; right.len()];
        for l in left {
            let mut found = false;
            for (i, r) in right.iter().enumerate() {
                if l.0.is_some() && l.0 == r.0 {
                    found = true;
                    used[i] = true;
                    out.push((Some(l.1), Some(r.1)));
                }
            }
            if !found && matches!(kind, JoinKind::Left | JoinKind::Full) {
                out.push((Some(l.1), None));
            }
        }
        if matches!(kind, JoinKind::Right | JoinKind::Full) {
            for (i, r) in right.iter().enumerate() {
                if !used[i] {
                    out.push((None, Some(r.1)));
                }
            }
        }
        out.sort();
        out
    }

    fn sorted(mut v: Vec<Pair>) -> Vec<Pair> {
        v.sort();
        v
    }

    #[test]
    fn s3j_c1_the_worked_example_for_every_kind() {
        let l = [(Some(1), 10), (Some(2), 20), (None, 30)];
        let r = [(Some(2), 200), (Some(3), 300), (None, 400)];
        assert_eq!(sorted(hash_join(&l, &r, JoinKind::Inner)), [(Some(20), Some(200))], "inner");
        assert_eq!(
            sorted(hash_join(&l, &r, JoinKind::Left)),
            sorted(vec![(Some(20), Some(200)), (Some(10), None), (Some(30), None)]),
            "left"
        );
        assert_eq!(
            sorted(hash_join(&l, &r, JoinKind::Right)),
            sorted(vec![(Some(20), Some(200)), (None, Some(300)), (None, Some(400))]),
            "right"
        );
        assert_eq!(hash_join(&l, &r, JoinKind::Full).len(), 5, "full: one pair, two padded left rows, two padded right rows");
    }

    #[test]
    fn s3j_c1_null_keys_match_nothing_but_are_kept() {
        let l = [(None, 1)];
        let r = [(None, 2)];
        assert!(hash_join(&l, &r, JoinKind::Inner).is_empty(), "NULL = NULL is not a match");
        assert_eq!(
            sorted(hash_join(&l, &r, JoinKind::Full)),
            sorted(vec![(Some(1), None), (None, Some(2))]),
            "a FULL join keeps both rows, padded"
        );
    }

    #[test]
    fn s3j_c1_a_row_with_several_partners_appears_once_per_partner() {
        let l = [(Some(1), 1), (Some(1), 2)];
        let r = [(Some(1), 5), (Some(1), 6)];
        for kind in [JoinKind::Inner, JoinKind::Left, JoinKind::Right, JoinKind::Full] {
            assert_eq!(hash_join(&l, &r, kind).len(), 4, "{kind:?}: two left rows times two right rows, nothing padded");
        }
    }

    #[test]
    fn s3j_c1_empty_inputs() {
        let r = [(Some(1), 5)];
        assert!(hash_join(&[], &[], JoinKind::Full).is_empty(), "nothing joined with nothing");
        assert_eq!(hash_join(&[], &r, JoinKind::Full), [(None, Some(5))], "an empty left side: the right rows, padded");
        assert!(hash_join(&[], &r, JoinKind::Left).is_empty(), "a LEFT join of nothing is nothing");
        assert_eq!(hash_join(&r, &[], JoinKind::Left), [(Some(5), None)], "an empty right side: the left rows, padded");
        assert!(hash_join(&r, &[], JoinKind::Inner).is_empty(), "an inner join with nothing is nothing");
    }

    #[test]
    fn s3j_c1_linear_time() {
        let n = 200_000i64;
        let l: Vec<Row> = (0..n).map(|i| (Some(i), i)).collect();
        let r: Vec<Row> = (n / 2..n + n / 2).map(|i| (Some(i), i)).collect();
        let start = std::time::Instant::now();
        let out = hash_join(&l, &r, JoinKind::Full);
        assert_eq!(out.len() as i64, n + n / 2, "half overlap: n/2 pairs, n/2 padded on each side");
        assert!(start.elapsed().as_secs() < 10, "200 000 rows on each side took {:?}: a nested loop cannot do this", start.elapsed());
    }

    fn arb_rows() -> impl Strategy<Value = Vec<Row>> {
        proptest::collection::vec((prop_oneof![3 => (0i64..5).prop_map(Some), 1 => Just(None)], 0i64..1000), 0..12)
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 80, failure_persistence: None, ..ProptestConfig::default() })]

        #[test]
        fn s3j_c1_property_every_kind_equals_the_nested_loop(l in arb_rows(), r in arb_rows()) {
            for kind in [JoinKind::Inner, JoinKind::Left, JoinKind::Right, JoinKind::Full] {
                prop_assert_eq!(sorted(hash_join(&l, &r, kind)), model(&l, &r, kind), "{:?}", kind);
            }
        }

        #[test]
        fn s3j_c1_property_swapping_the_inputs_swaps_left_and_right(l in arb_rows(), r in arb_rows()) {
            let swapped: Vec<Pair> = hash_join(&r, &l, JoinKind::Left).into_iter().map(|(a, b)| (b, a)).collect();
            prop_assert_eq!(sorted(hash_join(&l, &r, JoinKind::Right)), sorted(swapped));
        }
    }
}
// @@ challenge 3j-c1 end

// @@ challenge 3j-c2 begin
mod ch_3j_c2 {

    use super::*;
    use bustub::execution::outer_join_counts::{partner_counts, Row};

    fn model(left: &[Row], right: &[Row]) -> Vec<(i64, usize)> {
        left.iter().map(|l| (l.1, right.iter().filter(|r| l.0.is_some() && l.0 == r.0).count())).collect()
    }

    #[test]
    fn s3j_c2_a_customer_without_orders_has_none() {
        let l = [(Some(1), 10), (Some(2), 20)];
        let r = [(Some(1), 1), (Some(1), 2)];
        assert_eq!(partner_counts(&l, &r), [(10, 2), (20, 0)], "customer 20 has no order: 0, not 1");
    }

    #[test]
    fn s3j_c2_every_customer_is_listed_in_order() {
        let l = [(Some(3), 1), (Some(1), 2), (Some(2), 3), (Some(3), 4)];
        let r = [(Some(3), 9), (Some(1), 8)];
        assert_eq!(partner_counts(&l, &r), [(1, 1), (2, 1), (3, 0), (4, 1)], "one entry per left row, in the left order");
    }

    #[test]
    fn s3j_c2_null_keys_have_no_partners() {
        assert_eq!(partner_counts(&[(None, 30)], &[(None, 1)]), [(30, 0)], "NULL matches nothing, not even a NULL");
    }

    #[test]
    fn s3j_c2_no_orders_at_all() {
        let l = [(Some(1), 1), (Some(2), 2)];
        assert_eq!(partner_counts(&l, &[]), [(1, 0), (2, 0)], "an empty right side");
        assert!(partner_counts(&[], &[(Some(1), 1)]).is_empty(), "an empty left side");
    }

    #[test]
    fn s3j_c2_many_orders_for_one_customer() {
        let l = [(Some(7), 1)];
        let r: Vec<Row> = (0..40).map(|i| (Some(7), i)).collect();
        assert_eq!(partner_counts(&l, &r), [(1, 40)], "forty orders");
    }

    fn arb_rows() -> impl Strategy<Value = Vec<Row>> {
        proptest::collection::vec((prop_oneof![3 => (0i64..4).prop_map(Some), 1 => Just(None)], 0i64..1000), 0..10)
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 80, failure_persistence: None, ..ProptestConfig::default() })]

        #[test]
        fn s3j_c2_property_counts_equal_the_model(l in arb_rows(), r in arb_rows()) {
            prop_assert_eq!(partner_counts(&l, &r), model(&l, &r));
        }

        #[test]
        fn s3j_c2_property_a_new_order_raises_one_count(l in arb_rows(), r in arb_rows(), k in 0i64..4) {
            let before = partner_counts(&l, &r);
            let mut r2 = r.clone();
            r2.push((Some(k), 5000));
            let after = partner_counts(&l, &r2);
            for (i, (b, a)) in before.iter().zip(&after).enumerate() {
                let expected = b.1 + usize::from(l[i].0 == Some(k));
                prop_assert_eq!(a.1, expected, "left row {}", i);
            }
        }
    }
}
// @@ challenge 3j-c2 end

// @@ challenge 3j-c3 begin
mod ch_3j_c3 {

    use super::*;
    use bustub::execution::set_laws::*;

    fn distinct(mut v: Vec<i64>) -> Vec<i64> {
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn s3j_c3_union_of_sorted_input() {
        assert_eq!(union(&[1, 2, 3], &[2, 3, 4]), [1, 2, 3, 4], "sorted inputs");
        assert_eq!(union(&[], &[]), Vec::<i64>::new(), "nothing");
    }

    #[test]
    fn s3j_c3_union_of_unsorted_input() {
        assert_eq!(union(&[1, 2, 1], &[2]), [1, 2], "a duplicate that is not next to its twin");
        assert_eq!(union(&[3, 1], &[2, 3, 1]), [1, 2, 3], "after the concatenation the equal values are far apart");
    }

    #[test]
    fn s3j_c3_the_other_five_operations() {
        let (l, r) = ([1, 1, 1, 2], [1, 1, 3]);
        assert_eq!(union_all(&l, &r).len(), 7, "filters on one side of an inner join move to that side");
        assert_eq!(intersect(&l, &r), [1], "filters on one side of an inner join move to that side");
        assert_eq!(intersect_all(&l, &r), [1, 1], "filters on one side of an inner join move to that side");
        assert_eq!(except(&l, &r), [2], "filters on one side of an inner join move to that side");
        assert_eq!(except_all(&l, &r), [1, 2], "filters on one side of an inner join move to that side");
    }

    #[test]
    fn s3j_c3_union_with_itself_is_the_distinct_rows() {
        assert_eq!(union(&[5, 3, 5, 3, 1], &[5, 3, 5, 3, 1]), [1, 3, 5], "filters on one side of an inner join move to that side");
    }

    #[test]
    fn s3j_c3_results_are_sorted_and_duplicates_free() {
        let u = union(&[9, 7, 9, 8, 7], &[8, 9, 6, 6]);
        assert_eq!(u, [6, 7, 8, 9], "filters on one side of an inner join move to that side");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 120, failure_persistence: None, ..ProptestConfig::default() })]

        #[test]
        fn s3j_c3_property_union_is_the_distinct_union_all(l in proptest::collection::vec(0i64..6, 0..10), r in proptest::collection::vec(0i64..6, 0..10)) {
            prop_assert_eq!(union(&l, &r), distinct(union_all(&l, &r)));
            prop_assert_eq!(union(&l, &r), union(&r, &l));
        }

        #[test]
        fn s3j_c3_property_intersect_and_except_split_the_left(l in proptest::collection::vec(0i64..6, 0..10), r in proptest::collection::vec(0i64..6, 0..10)) {
            let mut both = intersect_all(&l, &r);
            both.extend(except_all(&l, &r));
            both.sort();
            let mut want = l.clone();
            want.sort();
            prop_assert_eq!(both, want);
            let mut s = intersect(&l, &r);
            s.extend(except(&l, &r));
            s.sort();
            prop_assert_eq!(s, distinct(l.clone()));
            prop_assert_eq!(intersect(&l, &r), intersect(&r, &l));
        }
    }
}
// @@ challenge 3j-c3 end

// @@ challenge 3j-c4 begin
mod ch_3j_c4 {

    use super::*;
    use bustub::execution::merge_set_ops::{merge_set_op, Op};
    use std::cell::Cell as Counter;
    use std::rc::Rc;

    fn run(l: &[i64], r: &[i64], op: Op, all: bool) -> Vec<i64> {
        merge_set_op(l.to_vec().into_iter(), r.to_vec().into_iter(), op, all).collect()
    }

    fn model(l: &[i64], r: &[i64], op: Op, all: bool) -> Vec<i64> {
        let mut keys: Vec<i64> = l.iter().chain(r).copied().collect();
        keys.sort();
        keys.dedup();
        let mut out = vec![];
        for k in keys {
            let (na, nb) = (l.iter().filter(|x| **x == k).count(), r.iter().filter(|x| **x == k).count());
            let n = match (op, all) {
                (Op::Union, true) => na + nb,
                (Op::Union, false) => 1,
                (Op::Intersect, true) => na.min(nb),
                (Op::Intersect, false) => usize::from(na > 0 && nb > 0),
                (Op::Except, true) => na.saturating_sub(nb),
                (Op::Except, false) => usize::from(na > 0 && nb == 0),
            };
            out.extend(std::iter::repeat(k).take(n));
        }
        out
    }

    #[test]
    fn s3j_c4_the_worked_examples() {
        assert_eq!(run(&[1, 1, 3], &[1, 2], Op::Union, true), [1, 1, 1, 2, 3], "filters on one side of an inner join move to that side");
        assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Intersect, true), [1, 1], "filters on one side of an inner join move to that side");
        assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Except, true), [1, 2], "filters on one side of an inner join move to that side");
        assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Except, false), [2], "filters on one side of an inner join move to that side");
        assert_eq!(run(&[1, 1, 1, 2], &[1, 1, 3], Op::Union, false), [1, 2, 3], "filters on one side of an inner join move to that side");
    }

    #[test]
    fn s3j_c4_empty_sides() {
        for op in [Op::Union, Op::Intersect, Op::Except] {
            for all in [false, true] {
                assert_eq!(run(&[], &[], op, all), Vec::<i64>::new(), "{op:?} {all}");
                assert_eq!(run(&[1, 2, 2], &[], op, all), model(&[1, 2, 2], &[], op, all), "{op:?} {all}: empty right");
                assert_eq!(run(&[], &[1, 2, 2], op, all), model(&[], &[1, 2, 2], op, all), "{op:?} {all}: empty left");
            }
        }
    }

    #[test]
    fn s3j_c4_it_works_on_infinite_inputs_because_it_is_lazy() {
        let evens = (0i64..).map(|x| x * 2);
        let odds = (0i64..).map(|x| x * 2 + 1);
        let first: Vec<i64> = merge_set_op(evens, odds, Op::Union, true).take(6).collect();
        assert_eq!(first, [0, 1, 2, 3, 4, 5], "the first six of an infinite union, without reading to the end");
        let threes = (0i64..).map(|x| x * 3);
        let twos = (0i64..).map(|x| x * 2);
        let both: Vec<i64> = merge_set_op(threes, twos, Op::Intersect, false).take(4).collect();
        assert_eq!(both, [0, 6, 12, 18], "multiples of both 2 and 3");
    }

    #[test]
    fn s3j_c4_the_first_item_reads_a_bounded_number_of_inputs() {
        let (pulled_l, pulled_r) = (Rc::new(Counter::new(0usize)), Rc::new(Counter::new(0usize)));
        let (a, b) = (pulled_l.clone(), pulled_r.clone());
        let left = (0i64..1_000_000).inspect(move |_| a.set(a.get() + 1));
        let right = (0i64..1_000_000).inspect(move |_| b.set(b.get() + 1));
        let mut it = merge_set_op(left, right, Op::Union, true);
        assert_eq!(it.next(), Some(0), "filters on one side of an inner join move to that side");
        assert!(
            pulled_l.get() <= 3 && pulled_r.get() <= 3,
            "one output item pulled {} and {} input items: it must not read ahead",
            pulled_l.get(),
            pulled_r.get()
        );
    }

    #[test]
    fn s3j_c4_long_runs_are_counted_not_stored() {
        let l = std::iter::repeat(5i64).take(100_000);
        let r = std::iter::repeat(5i64).take(99_990);
        let diff: Vec<i64> = merge_set_op(l, r, Op::Except, true).collect();
        assert_eq!(diff.len(), 10, "100000 - 99990 copies");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 100, failure_persistence: None, ..ProptestConfig::default() })]

        #[test]
        fn s3j_c4_property_all_six_equal_the_counting_model(mut l in proptest::collection::vec(0i64..6, 0..12), mut r in proptest::collection::vec(0i64..6, 0..12)) {
            l.sort();
            r.sort();
            for op in [Op::Union, Op::Intersect, Op::Except] {
                for all in [false, true] {
                    prop_assert_eq!(run(&l, &r, op, all), model(&l, &r, op, all), "{:?} all={}", op, all);
                }
            }
        }
    }
}
// @@ challenge 3j-c4 end

// @@ challenge 3j-c5 begin
mod ch_3j_c5 {

    use super::*;
    use bustub::execution::null_rejection::{eval, rejects_nulls, B, S};

    fn c(i: usize) -> S {
        S::Col(i)
    }
    fn l(v: i64) -> S {
        S::Lit(v)
    }
    fn co(a: S, b: S) -> S {
        S::Coalesce(Box::new(a), Box::new(b))
    }
    fn not(b: B) -> B {
        B::Not(Box::new(b))
    }
    fn and(a: B, b: B) -> B {
        B::And(Box::new(a), Box::new(b))
    }
    fn or(a: B, b: B) -> B {
        B::Or(Box::new(a), Box::new(b))
    }

    /// Can the condition be TRUE for some values of the columns that are not in `nulls`? (Brute force over a small domain.)
    fn can_be_true(b: &B, nulls: &[usize], ncols: usize) -> bool {
        let domain = [None, Some(-1), Some(0), Some(1), Some(2), Some(3), Some(4)];
        let free: Vec<usize> = (0..ncols).filter(|i| !nulls.contains(i)).collect();
        let total = domain.len().pow(free.len() as u32);
        for mut n in 0..total {
            let mut row = vec![None; ncols];
            for &i in &free {
                row[i] = domain[n % domain.len()];
                n /= domain.len();
            }
            if eval(b, &row) == Some(true) {
                return true;
            }
        }
        false
    }

    #[test]
    fn s3j_c5_comparisons_and_null_tests() {
        assert!(rejects_nulls(&B::Gt(c(0), l(3)), &[0]), "NULL > 3 is unknown");
        assert!(!rejects_nulls(&B::Gt(c(0), l(3)), &[1]), "c0 is not NULL: it can be 5");
        assert!(rejects_nulls(&B::Eq(c(0), c(1)), &[1]), "x = NULL is unknown");
        assert!(!rejects_nulls(&B::IsNull(c(0)), &[0]), "NULL IS NULL is true");
        assert!(rejects_nulls(&B::IsNotNull(c(0)), &[0]), "NULL IS NOT NULL is false");
    }

    #[test]
    fn s3j_c5_not_and_or() {
        assert!(rejects_nulls(&not(B::Gt(c(0), l(3))), &[0]), "NOT unknown is unknown");
        assert!(rejects_nulls(&not(B::IsNull(c(0))), &[0]), "NOT (c0 IS NULL) is c0 IS NOT NULL");
        assert!(!rejects_nulls(&not(B::IsNotNull(c(0))), &[0]), "NOT (c0 IS NOT NULL) is c0 IS NULL: true");
        let p = or(B::Gt(c(0), l(3)), B::Gt(c(1), l(3)));
        assert!(!rejects_nulls(&p, &[0]), "OR: the other side can be true");
        assert!(rejects_nulls(&p, &[0, 1]), "OR: both sides unknown");
        assert!(rejects_nulls(&and(B::Gt(c(0), l(3)), B::Gt(c(1), l(3))), &[0]), "AND: one unknown side is enough");
        assert!(rejects_nulls(&not(and(B::Gt(c(0), l(3)), B::Gt(c(1), l(3)))), &[0, 1]), "NOT (unknown AND unknown)");
        assert!(
            !rejects_nulls(&not(and(B::Gt(c(0), l(3)), B::Gt(c(1), l(3)))), &[0]),
            "NOT (unknown AND c1 > 3): when c1 <= 3 the AND is false and the NOT true"
        );
    }

    #[test]
    fn s3j_c5_coalesce_and_constants() {
        assert!(!rejects_nulls(&B::Gt(co(c(0), l(5)), l(3)), &[0]), "COALESCE(NULL, 5) > 3 is true");
        assert!(rejects_nulls(&B::Gt(co(c(0), l(1)), l(3)), &[0]), "COALESCE(NULL, 1) > 3 is false");
        assert!(!rejects_nulls(&B::Gt(co(c(0), c(1)), l(3)), &[0]), "the second column can be big");
        assert!(rejects_nulls(&B::Gt(co(c(0), c(1)), l(3)), &[0, 1]), "both NULL");
        assert!(rejects_nulls(&B::Eq(l(1), l(2)), &[]), "a constant condition that is false rejects every row");
        assert!(!rejects_nulls(&B::Eq(l(2), l(2)), &[]), "and a true one rejects none");
    }

    fn arb_s(ncols: usize) -> impl Strategy<Value = S> {
        let leaf = prop_oneof![(0..ncols).prop_map(S::Col), (0i64..3).prop_map(S::Lit)];
        leaf.prop_recursive(2, 6, 2, |inner| (inner.clone(), inner).prop_map(|(a, b)| S::Coalesce(Box::new(a), Box::new(b))))
    }

    fn arb_b(ncols: usize) -> impl Strategy<Value = B> {
        let leaf = prop_oneof![
            (arb_s(ncols), arb_s(ncols)).prop_map(|(a, b)| B::Gt(a, b)),
            (arb_s(ncols), arb_s(ncols)).prop_map(|(a, b)| B::Eq(a, b)),
            arb_s(ncols).prop_map(B::IsNull),
            arb_s(ncols).prop_map(B::IsNotNull),
        ];
        leaf.prop_recursive(3, 12, 2, |inner| {
            prop_oneof![
                (inner.clone(), inner.clone()).prop_map(|(a, b)| B::And(Box::new(a), Box::new(b))),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| B::Or(Box::new(a), Box::new(b))),
                inner.prop_map(|a| B::Not(Box::new(a))),
            ]
        })
    }

    fn columns(b: &B, out: &mut Vec<usize>) {
        fn s(x: &S, out: &mut Vec<usize>) {
            match x {
                S::Col(i) => out.push(*i),
                S::Lit(_) => {}
                S::Coalesce(a, b) => {
                    s(a, out);
                    s(b, out);
                }
            }
        }
        match b {
            B::Gt(x, y) | B::Eq(x, y) => {
                s(x, out);
                s(y, out);
            }
            B::IsNull(x) | B::IsNotNull(x) => s(x, out),
            B::And(x, y) | B::Or(x, y) => {
                columns(x, out);
                columns(y, out);
            }
            B::Not(x) => columns(x, out),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 300, max_global_rejects: 100_000, failure_persistence: None, ..ProptestConfig::default() })]

        #[test]
        fn s3j_c5_property_a_yes_is_always_right(cond in arb_b(3), mask in 0u8..8) {
            let nulls: Vec<usize> = (0..3).filter(|i| mask & (1 << i) != 0).collect();
            if rejects_nulls(&cond, &nulls) {
                prop_assert!(!can_be_true(&cond, &nulls, 3), "{:?} with NULL columns {:?} can be true", cond, nulls);
            }
        }

        #[test]
        fn s3j_c5_property_it_finds_every_rejection_when_no_column_repeats(cond in arb_b(3), mask in 0u8..8) {
            let mut cols = vec![];
            columns(&cond, &mut cols);
            let mut uniq = cols.clone();
            uniq.sort();
            uniq.dedup();
            prop_assume!(uniq.len() == cols.len());
            let nulls: Vec<usize> = (0..3).filter(|i| mask & (1 << i) != 0).collect();
            prop_assert_eq!(rejects_nulls(&cond, &nulls), !can_be_true(&cond, &nulls, 3), "{:?} with NULL columns {:?}", cond, nulls);
        }
    }
}
// @@ challenge 3j-c5 end
