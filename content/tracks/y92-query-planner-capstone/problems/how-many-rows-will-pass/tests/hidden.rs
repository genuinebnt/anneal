use solution::*;
use anneal_prelude::Rng;
use std::collections::HashMap;

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[allow(dead_code)]
fn table(columns: Vec<Vec<Option<i64>>>) -> Table {
    Table {
        name: "t".to_string(),
        column_names: (0..columns.len()).map(|i| format!("c{i}")).collect(),
        rows: columns[0].len(),
        stats: columns.iter().map(|c| summarise(c)).collect(),
        indexed: vec![],
    }
}

#[allow(dead_code)]
fn random_catalog(rng: &mut Rng) -> Catalog {
    let tables = (0..2)
        .map(|_| {
            let n = rng.below(80);
            let columns = (0..3)
                .map(|c| {
                    let span = [4, 30, 500][c];
                    (0..n).map(|_| (rng.below(8) > 0).then(|| rng.int(0, span))).collect()
                })
                .collect();
            table(columns)
        })
        .collect();
    Catalog { tables }
}

#[allow(dead_code)]
fn random_column(rng: &mut Rng) -> Column {
    Column { table: rng.below(2), column: rng.below(3) }
}

#[allow(dead_code)]
fn random_value(rng: &mut Rng) -> i64 {
    match rng.below(12) {
        0 => i64::MIN,
        1 => i64::MAX,
        _ => rng.int(-2, 40),
    }
}

#[allow(dead_code)]
fn random_cond(rng: &mut Rng, depth: usize) -> Cond {
    let cmps = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge];
    match rng.below(if depth == 0 { 3 } else { 6 }) {
        1 => {
            let n = rng.below(5);
            Cond::OneOf(random_column(rng), (0..n).map(|_| rng.int(0, 6)).collect())
        }
        2 => {
            let a = random_column(rng);
            Cond::Same(a, Column { table: a.table, column: (a.column + 1) % 3 })
        }
        3 => {
            let n = rng.below(4);
            Cond::Any((0..n).map(|_| random_cond(rng, depth - 1)).collect())
        }
        4 | 5 => {
            let n = rng.below(5);
            Cond::All((0..n).map(|_| random_cond(rng, depth - 1)).collect())
        }
        _ => {
            // few columns, so bounds on the same one meet often
            let column = Column { table: 0, column: rng.below(2) };
            Cond::Compare(column, *rng.pick(&cmps), random_value(rng))
        }
    }
}

/// The rules again, written independently.
#[allow(dead_code)]
fn model(cond: &Cond, catalog: &Catalog) -> f64 {
    let stats = |c: &Column| catalog.tables[c.table].stats[c.column].clone();
    let rows = |c: &Column| catalog.tables[c.table].rows as f64;
    let share = |c: &Column, n: f64| if rows(c) == 0.0 { 0.0 } else { (n / rows(c)).clamp(0.0, 1.0) };
    match cond {
        Cond::Compare(c, Cmp::Eq, v) => share(c, stats(c).equal_to(*v)),
        Cond::Compare(c, Cmp::Ne, v) => {
            let s = stats(c);
            share(c, (s.rows - s.missing) as f64 - s.equal_to(*v))
        }
        Cond::Compare(c, cmp, v) => {
            let (low, high) = model_range(*cmp, *v);
            share(c, if low > high { 0.0 } else { stats(c).between(low as i64, high as i64) })
        }
        Cond::Same(a, b) => {
            let (x, y) = (stats(a).distinct, stats(b).distinct);
            if x.min(y) == 0 { 0.0 } else { 1.0 / x.max(y) as f64 }
        }
        Cond::OneOf(c, values) => {
            let mut seen = Vec::new();
            let mut sum = 0.0;
            for v in values {
                if !seen.contains(v) {
                    seen.push(*v);
                }
            }
            seen.sort();
            for v in seen {
                sum += share(c, stats(c).equal_to(v));
            }
            if sum > 1.0 { 1.0 } else { sum }
        }
        Cond::Any(parts) => {
            let mut none = 1.0;
            for p in parts {
                none *= 1.0 - model(p, catalog);
            }
            1.0 - none
        }
        Cond::All(parts) => {
            let mut flat = Vec::new();
            let mut stack: Vec<&Cond> = parts.iter().collect();
            while let Some(p) = stack.pop() {
                match p {
                    Cond::All(inner) => stack.extend(inner),
                    other => flat.push(other),
                }
            }
            let mut ranges: HashMap<Column, (i128, i128)> = HashMap::new();
            let mut kept = 1.0;
            for p in flat {
                match p {
                    Cond::Compare(c, cmp, v) if !matches!(cmp, Cmp::Eq | Cmp::Ne) => {
                        let (low, high) = model_range(*cmp, *v);
                        let r = ranges.entry(*c).or_insert((i64::MIN as i128, i64::MAX as i128));
                        r.0 = r.0.max(low);
                        r.1 = r.1.min(high);
                    }
                    other => kept *= model(other, catalog),
                }
            }
            for (c, (low, high)) in ranges {
                kept *= share(&c, if low > high { 0.0 } else { stats(&c).between(low as i64, high as i64) });
            }
            kept
        }
    }
}

/// Bounds in a wider type, so nothing can overflow.
#[allow(dead_code)]
fn model_range(cmp: Cmp, v: i64) -> (i128, i128) {
    let (min, max, v) = (i64::MIN as i128, i64::MAX as i128, v as i128);
    let (low, high) = match cmp {
        Cmp::Lt => (min, v - 1),
        Cmp::Le => (min, v),
        Cmp::Gt => (v + 1, max),
        Cmp::Ge => (v, max),
        _ => (v, v),
    };
    if low > max || high < min { (1, 0) } else { (low, high) }
}

#[test]
fn ranges_at_the_ends() {
    for (cmp, v, allowed) in [
        (Cmp::Lt, i64::MIN, false),
        (Cmp::Gt, i64::MAX, false),
        (Cmp::Le, i64::MIN, true),
        (Cmp::Ge, i64::MAX, true),
    ] {
        let (low, high) = range_of(cmp, v).unwrap();
        check!(format!("{cmp:?} {v} allows some value"), low <= high, allowed);
    }
    check!("x = MIN", range_of(Cmp::Eq, i64::MIN), Some((i64::MIN, i64::MIN)));
    check!("x > 0", range_of(Cmp::Gt, 0), Some((1, i64::MAX)));
    check!("x <= 0", range_of(Cmp::Le, 0), Some((i64::MIN, 0)));
    check!("x >= 7", range_of(Cmp::Ge, 7), Some((7, i64::MAX)));
}

#[test]
fn impossible_bounds_keep_nothing_even_beside_other_bounds() {
    let c = Catalog { tables: vec![table(vec![(0..50).map(Some).collect()])] };
    let col = Column { table: 0, column: 0 };
    for (name, parts) in [
        ("x > MAX and x < 10", vec![Cond::Compare(col, Cmp::Gt, i64::MAX), Cond::Compare(col, Cmp::Lt, 10)]),
        ("x < MIN and x >= 0", vec![Cond::Compare(col, Cmp::Lt, i64::MIN), Cond::Compare(col, Cmp::Ge, 0)]),
        ("x > 20 and x < 10", vec![Cond::Compare(col, Cmp::Gt, 20), Cond::Compare(col, Cmp::Lt, 10)]),
    ] {
        check!(name, selectivity(&Cond::All(parts), &c), 0.0);
    }
}

#[test]
fn bounds_hidden_in_nested_lists_are_still_one_range() {
    let c = Catalog { tables: vec![table(vec![(0..1000).map(Some).collect()])] };
    let col = Column { table: 0, column: 0 };
    let nested = Cond::All(vec![
        Cond::All(vec![Cond::Compare(col, Cmp::Gt, 100)]),
        Cond::All(vec![Cond::All(vec![Cond::Compare(col, Cmp::Lt, 200)]), Cond::All(vec![])]),
    ]);
    let want = c.tables[0].stats[0].between(101, 199) / 1000.0;
    check!("nested x > 100 and x < 200", close(selectivity(&nested, &c), want), true);
}

#[test]
fn an_equality_is_not_merged_into_the_range() {
    let c = Catalog { tables: vec![table(vec![(0..100).map(|i| Some(i % 20)).collect()])] };
    let col = Column { table: 0, column: 0 };
    let parts = vec![Cond::Compare(col, Cmp::Eq, 3), Cond::Compare(col, Cmp::Lt, 10)];
    let want = selectivity(&parts[0], &c) * selectivity(&parts[1], &c);
    check!("x = 3 and x < 10 multiply", close(selectivity(&Cond::All(parts), &c), want), true);
}

#[test]
fn empty_lists_and_empty_tables() {
    let c = Catalog { tables: vec![table(vec![vec![], vec![]])] };
    let col = Column { table: 0, column: 0 };
    check!("all of nothing", selectivity(&Cond::All(vec![]), &c), 1.0);
    check!("any of nothing", selectivity(&Cond::Any(vec![]), &c), 0.0);
    check!("empty list of values", selectivity(&Cond::OneOf(col, vec![]), &c), 0.0);
    for cmp in [Cmp::Eq, Cmp::Ne, Cmp::Gt] {
        check!(format!("empty table, {cmp:?}"), selectivity(&Cond::Compare(col, cmp, 1), &c), 0.0);
    }
    check!("empty table, column = column", selectivity(&Cond::Same(col, Column { table: 0, column: 1 }), &c), 0.0);
}

#[test]
fn a_list_of_values_never_passes_one() {
    let c = Catalog { tables: vec![table(vec![vec![Some(1); 10]])] };
    let col = Column { table: 0, column: 0 };
    check!("x in (1, 1, 1)", selectivity(&Cond::OneOf(col, vec![1, 1, 1]), &c), 1.0);
}

#[test]
fn random_conditions_follow_the_rules() {
    let mut rng = Rng::new(9201);
    for case in 0..1500 {
        let catalog = random_catalog(&mut rng);
        let cond = random_cond(&mut rng, 3);
        let got = selectivity(&cond, &catalog);
        let want = model(&cond, &catalog);
        let sizes: Vec<usize> = catalog.tables.iter().map(|t| t.rows).collect();
        check!(format!("case {case}, tables of {sizes:?} rows: {cond:?} gave {got}, wanted {want}"), close(got, want), true);
    }
}

#[test]
fn the_error_factor_counts_at_least_one_row() {
    check!("both below one row", q_error(0.0, 0.2), 1.0);
    check!("estimate 0, actual 50", q_error(0.0, 50.0), 50.0);
    check!("perfect", q_error(7.0, 7.0), 1.0);
}

#[test]
fn many_bounds_on_one_column() {
    let c = Catalog { tables: vec![table(vec![(0..1000).map(Some).collect()])] };
    let col = Column { table: 0, column: 0 };
    let parts: Vec<Cond> = (0..20_000).map(|i| Cond::Compare(col, Cmp::Ge, i % 500)).collect();
    let want = c.tables[0].stats[0].between(499, i64::MAX) / 1000.0;
    check!("20 000 lower bounds are one range", close(selectivity(&Cond::All(parts), &c), want), true);
}

#[test]
fn independent_columns_are_estimated_well_and_linked_ones_are_not() {
    let mut rng = Rng::new(9202);
    let a: Vec<i64> = (0..4000).map(|_| rng.int(0, 19)).collect();
    let free: Vec<i64> = (0..4000).map(|_| rng.int(0, 19)).collect();
    let c = Catalog { tables: vec![table(vec![
        a.iter().map(|&v| Some(v)).collect(),
        free.iter().map(|&v| Some(v)).collect(),
        a.iter().map(|&v| Some(v)).collect(),
    ])] };
    let eq = |column, v| Cond::Compare(Column { table: 0, column }, Cmp::Eq, v);
    let truth_free = (0..4000).filter(|&i| a[i] == 3 && free[i] == 3).count() as f64;
    let truth_linked = a.iter().filter(|&&v| v == 3).count() as f64;
    let est_free = 4000.0 * selectivity(&Cond::All(vec![eq(0, 3), eq(1, 3)]), &c);
    let est_linked = 4000.0 * selectivity(&Cond::All(vec![eq(0, 3), eq(2, 3)]), &c);
    check!("unrelated columns: off by less than 2x", q_error(est_free, truth_free) < 2.0, true);
    check!("a copied column: off by more than 10x, because the rule multiplies", q_error(est_linked, truth_linked) > 10.0, true);
}
