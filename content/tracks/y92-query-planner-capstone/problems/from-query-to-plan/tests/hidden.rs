use solution::*;
use std::cell::Cell;
use anneal_prelude::Rng;

/// A store that counts how many rows have been read from each table.
#[allow(dead_code)]
struct Counting {
    tables: Vec<(usize, Vec<Row>)>,
    indexes: Vec<Vec<Vec<(i64, usize)>>>,
    reads: Vec<Cell<usize>>,
}

#[allow(dead_code)]
impl Counting {
    fn new(tables: Vec<(usize, Vec<Row>)>) -> Counting {
        let indexes = tables
            .iter()
            .map(|(width, rows)| {
                (0..*width)
                    .map(|c| {
                        let mut ix: Vec<(i64, usize)> = rows.iter().enumerate().filter_map(|(i, r)| r[c].map(|v| (v, i))).collect();
                        ix.sort();
                        ix
                    })
                    .collect()
            })
            .collect();
        let reads = tables.iter().map(|_| Cell::new(0)).collect();
        Counting { tables, indexes, reads }
    }
    fn reads(&self) -> Vec<usize> {
        self.reads.iter().map(Cell::get).collect()
    }
}

impl Store for Counting {
    fn width(&self, t: usize) -> usize {
        self.tables[t].0
    }
    fn row_count(&self, t: usize) -> usize {
        self.tables[t].1.len()
    }
    fn row(&self, t: usize, i: usize) -> Row {
        self.reads[t].set(self.reads[t].get() + 1);
        self.tables[t].1[i].clone()
    }
    fn index(&self, t: usize, c: usize) -> &[(i64, usize)] {
        &self.indexes[t][c]
    }
}

/// The catalog that describes the tables of a store. Table `i` is called `t{i}`, column `j` is `c{j}`.
#[allow(dead_code)]
fn catalog_for(tables: &[(usize, Vec<Row>)], indexed: &[Vec<usize>]) -> Catalog {
    let tables = tables
        .iter()
        .zip(indexed)
        .enumerate()
        .map(|(t, ((width, rows), indexed))| Table {
            name: format!("t{t}"),
            column_names: (0..*width).map(|c| format!("c{c}")).collect(),
            rows: rows.len(),
            stats: (0..*width).map(|c| summarise(&rows.iter().map(|r| r[c]).collect::<Vec<_>>())).collect(),
            indexed: indexed.clone(),
        })
        .collect();
    Catalog { tables }
}

#[allow(dead_code)]
fn col(table: usize, column: usize) -> Column {
    Column { table, column }
}

/// The trusted answer: pair every row with every row, then keep what the condition keeps.
#[allow(dead_code)]
fn plain(query: &Query, tables: &[(usize, Vec<Row>)]) -> Vec<Row> {
    let mut rows: Vec<Row> = vec![Vec::new()];
    let mut columns = Vec::new();
    for &t in &query.tables {
        columns.extend((0..tables[t].0).map(|c| col(t, c)));
        let mut next = Vec::new();
        for a in &rows {
            for b in &tables[t].1 {
                let mut row = a.clone();
                row.extend(b.clone());
                next.push(row);
            }
        }
        rows = next;
    }
    rows.retain(|r| holds(&query.condition, r, &columns));
    let mut out: Vec<Row> = rows
        .iter()
        .map(|r| query.output.iter().map(|w| r[columns.iter().position(|c| c == w).unwrap()]).collect())
        .collect();
    out.sort();
    out
}

#[allow(dead_code)]
fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort();
    rows
}

/// A query over some of the tables, with equalities that mostly connect them and a few other conditions.
#[allow(dead_code)]
fn random_query(rng: &mut Rng, available: usize) -> Query {
    let mut ids: Vec<usize> = (0..available).collect();
    rng.shuffle(&mut ids);
    ids.truncate(1 + rng.below(available));
    let mut parts = Vec::new();
    for i in 1..ids.len() {
        if rng.below(5) > 0 {
            let j = rng.below(i);
            parts.push(Cond::Same(col(ids[i], rng.below(2)), col(ids[j], rng.below(2))));
        }
    }
    for _ in 0..rng.below(4) {
        let a = col(*rng.pick(&ids), rng.below(2));
        let b = col(*rng.pick(&ids), rng.below(2));
        let cmp = *rng.pick(&[Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge]);
        parts.push(match rng.below(6) {
            0 => Cond::Any(vec![Cond::Compare(a, cmp, rng.int(0, 3)), Cond::Compare(b, Cmp::Eq, rng.int(0, 3))]),
            1 => Cond::OneOf(a, vec![rng.int(0, 3), rng.int(0, 3)]),
            2 if a != b => Cond::Same(a, b),
            3 => Cond::All(vec![Cond::Compare(a, cmp, rng.int(0, 3)), Cond::All(vec![Cond::Compare(b, Cmp::Le, 2)])]),
            4 if rng.below(5) == 0 => Cond::Any(vec![]),
            _ => Cond::Compare(a, cmp, rng.int(0, 3)),
        });
    }
    rng.shuffle(&mut parts);
    let output = (0..1 + rng.below(3)).map(|_| col(*rng.pick(&ids), rng.below(2))).collect();
    Query { tables: ids, condition: Cond::All(parts), output }
}

#[allow(dead_code)]
type Data = Vec<(usize, Vec<Row>)>;

#[allow(dead_code)]
fn random_data(rng: &mut Rng, most: usize) -> (Data, Vec<Vec<usize>>) {
    let tables = (0..4)
        .map(|_| {
            let n = rng.below(most + 1);
            (2, (0..n).map(|_| (0..2).map(|_| (rng.below(7) > 0).then(|| rng.int(0, 3))).collect()).collect())
        })
        .collect();
    let indexed = (0..4).map(|_| (0..2).filter(|_| rng.bool()).collect()).collect();
    (tables, indexed)
}

#[test]
fn random_queries_give_the_plain_answer() {
    let mut rng = Rng::new(9901);
    for case in 0..500 {
        let (tables, indexed) = random_data(&mut rng, 7);
        let catalog = catalog_for(&tables, &indexed);
        let query = random_query(&mut rng, 4);
        let input = format!("case {case}: {query:?} over {tables:?}");
        let plan = optimize(&query, &catalog);
        check!(format!("{input}: it is a valid query"), plan.as_ref().err(), None);
        let plan = plan.unwrap();
        check!(format!("{input}: the top"), plan.node.clone(), Node::Project(query.output.clone()));
        check!(format!("{input}: rows of {plan:?}"), sorted(run(&plan, &Counting::new(tables.clone()))), plain(&query, &tables));
        let below = &plan.inputs[0];
        let cost = below.cost + below.rows * 0.0025;
        check!(format!("{input}: the projection costs one check per row"), (plan.rows == below.rows, (plan.cost - cost).abs() < 1e-9 * cost.max(1.0)), (true, true));
        check!(format!("{input}: order"), plan.order, below.order.filter(|c| query.output.contains(c)));
    }
}

/// A cost the planner must not exceed: join the tables in the order written, taking the cheapest step each time.
#[allow(dead_code)]
fn in_the_written_order(query: &Query, catalog: &Catalog) -> f64 {
    let mut parts = Vec::new();
    conjuncts(&query.condition, &mut parts);
    let own = |t: usize| -> Vec<Cond> { parts.iter().filter(|p| tables_of(p) == 1 << t).cloned().collect() };
    let cheapest = |plans: Vec<Plan>| plans.into_iter().min_by(|a, b| a.cost.total_cmp(&b.cost)).unwrap();
    let mut plan = cheapest(leaf_plans(catalog, query.tables[0], &own(query.tables[0])));
    let mut seen: u64 = 1 << query.tables[0];
    for &t in &query.tables[1..] {
        let next = cheapest(leaf_plans(catalog, t, &own(t)));
        let now = seen | (1 << t);
        let on: Vec<Cond> = parts.iter().filter(|p| { let m = tables_of(p); m & !now == 0 && m & seen != 0 && m & (1 << t) != 0 }).cloned().collect();
        let mut rows = plan.rows * next.rows;
        for c in &on {
            rows *= selectivity(c, catalog);
        }
        plan = cheapest(join_plans(&plan, &next, &on, rows));
        seen = now;
    }
    plan.cost + plan.rows * 0.0025
}

#[test]
fn the_chosen_plan_is_never_dearer_than_the_written_order() {
    let mut rng = Rng::new(9902);
    let mut better = 0;
    for case in 0..150 {
        let (tables, indexed) = random_data(&mut rng, 300);
        let catalog = catalog_for(&tables, &indexed);
        let mut query = random_query(&mut rng, 4);
        // a connected query, so that the written order needs no pairing without a condition when it is a chain
        let mut parts: Vec<Cond> = (1..query.tables.len()).map(|i| Cond::Same(col(query.tables[i - 1], rng.below(2)), col(query.tables[i], rng.below(2)))).collect();
        parts.push(Cond::Compare(col(*rng.pick(&query.tables), 1), Cmp::Eq, rng.int(0, 3)));
        query.condition = Cond::All(parts);
        let plan = optimize(&query, &catalog).unwrap();
        let limit = in_the_written_order(&query, &catalog);
        check!(format!("case {case}: {query:?}: cost {} against {limit}", plan.cost), plan.cost <= limit * (1.0 + 1e-9), true);
        better += usize::from(plan.cost < limit * 0.99);
    }
    check!("the search beats the written order at least sometimes", better > 10, true);
}

#[test]
fn a_selective_condition_on_a_large_table_uses_the_index() {
    let tables = vec![(2, (0..20_000).map(|i| vec![Some(i), Some(i % 10)]).collect::<Vec<Row>>())];
    let catalog = catalog_for(&tables, &[vec![0]]);
    let query = Query { tables: vec![0], condition: Cond::Compare(col(0, 0), Cmp::Eq, 17), output: vec![col(0, 1)] };
    let plan = optimize(&query, &catalog).unwrap();
    let text = explain(&plan, &catalog);
    check!(format!("plan:\n{text}"), text.contains("IndexScan t0.c0 [17..17]"), true);
    let store = Counting::new(tables);
    check!("one row read", (run(&plan, &store), store.reads()[0]), (vec![vec![Some(7)]], 1));
    // the index keeps its order through the projection only if the column is still there
    check!("the order column is dropped", plan.order, None);
    let kept = Query { output: vec![col(0, 1), col(0, 0)], ..query };
    check!("the order column is kept", optimize(&kept, &catalog).unwrap().order, Some(col(0, 0)));
}

#[test]
fn every_way_a_query_can_be_wrong() {
    let tables = vec![(2, Vec::new()); 70];
    let catalog = catalog_for(&tables, &vec![vec![]; 70]);
    let fail = |tables: Vec<usize>, condition: Cond, output: Vec<Column>| optimize(&Query { tables, condition, output }, &catalog).err();
    let yes = || Cond::All(vec![]);
    check!("eleven tables", fail((0..11).collect(), yes(), vec![]), Some(PlanError::TooManyTables));
    check!("table 64 has no bit", fail(vec![64], yes(), vec![]), Some(PlanError::UnknownTable(64)));
    check!("table 70 does not exist", fail(vec![70], yes(), vec![]), Some(PlanError::UnknownTable(70)));
    check!("a huge table number", fail(vec![usize::MAX], yes(), vec![]), Some(PlanError::UnknownTable(usize::MAX)));
    check!("repeated", fail(vec![3, 4, 3], yes(), vec![]), Some(PlanError::RepeatedTable(3)));
    check!("a condition on a table not in the query", fail(vec![0], Cond::Compare(col(1, 0), Cmp::Eq, 1), vec![]), Some(PlanError::UnknownColumn(col(1, 0))));
    check!("a column the table lacks", fail(vec![0], Cond::Any(vec![Cond::OneOf(col(0, 2), vec![])]), vec![]), Some(PlanError::UnknownColumn(col(0, 2))));
    check!("the right side of an equality", fail(vec![0], Cond::Same(col(0, 0), col(99, 0)), vec![]), Some(PlanError::UnknownColumn(col(99, 0))));
    check!("an output column", fail(vec![0], yes(), vec![col(0, 1), col(2, 0)]), Some(PlanError::UnknownColumn(col(2, 0))));
    check!("ten tables are fine", fail((0..10).collect(), yes(), vec![col(9, 1)]), None);
}

#[allow(dead_code)]
fn nested(depth: usize) -> Cond {
    let mut c = Cond::Compare(col(0, 0), Cmp::Ge, 0);
    for level in 1..depth {
        c = if level % 2 == 0 { Cond::All(vec![c]) } else { Cond::Any(vec![c]) };
    }
    c
}

#[test]
fn the_depth_limit_is_exact() {
    let catalog = catalog_for(&[(1, vec![vec![Some(1)]])], &[vec![0]]);
    let try_depth = |d| optimize(&Query { tables: vec![0], condition: nested(d), output: vec![col(0, 0)] }, &catalog).map(|_| ()).err();
    check!("depth 64", try_depth(MAX_DEPTH), None);
    check!("depth 65", try_depth(MAX_DEPTH + 1), Some(PlanError::TooDeep));
}

#[test]
fn a_condition_two_hundred_thousand_deep_is_refused_without_crashing() {
    let catalog = catalog_for(&[(1, vec![])], &[vec![]]);
    let query = Query { tables: vec![0], condition: nested(200_000), output: vec![] };
    let got = optimize(&query, &catalog).map(|_| ()).err();
    // dropping a value this deep would itself need a very deep stack
    std::mem::forget(query);
    check!("depth 200 000", got, Some(PlanError::TooDeep));
}

#[test]
fn a_wide_condition_is_fine() {
    let tables = vec![(1, (0..50).map(|i| vec![Some(i)]).collect::<Vec<Row>>())];
    let catalog = catalog_for(&tables, &[vec![0]]);
    let parts = (0..2000).map(|i| Cond::Compare(col(0, 0), Cmp::Ne, i + 10)).collect();
    let query = Query { tables: vec![0], condition: Cond::All(parts), output: vec![col(0, 0)] };
    let plan = optimize(&query, &catalog).unwrap();
    check!("2000 conditions", run(&plan, &Counting::new(tables)).len(), 10);
}

#[test]
fn ten_tables_end_to_end() {
    let tables: Vec<(usize, Vec<Row>)> = (0..10).map(|t| (2, (0..3 + t as i64 % 3).map(|i| vec![Some(i), Some(i)]).collect())).collect();
    let catalog = catalog_for(&tables, &vec![vec![0]; 10]);
    let parts = (1..10).map(|t| Cond::Same(col(t - 1, 1), col(t, 0))).collect();
    let query = Query { tables: (0..10).collect(), condition: Cond::All(parts), output: vec![col(0, 0), col(9, 1)] };
    let plan = optimize(&query, &catalog).unwrap();
    check!("a chain of ten", sorted(run(&plan, &Counting::new(tables))), (0..3).map(|i| vec![Some(i), Some(i)]).collect::<Vec<_>>());
}

#[test]
fn the_naive_plan() {
    let c = Cond::Compare(col(0, 0), Cmp::Eq, 1);
    let query = Query { tables: vec![2, 0, 1], condition: c.clone(), output: vec![] };
    let boxed = |p| Box::new(p);
    let want = Logical::Filter(c, boxed(Logical::Cross(boxed(Logical::Cross(boxed(Logical::Scan(2)), boxed(Logical::Scan(0)))), boxed(Logical::Scan(1)))));
    check!("tables are paired left to right", naive_plan(&query), want);
}

#[test]
fn every_kind_of_line() {
    let mut catalog = catalog_for(&[(2, vec![]), (1, vec![])], &[vec![], vec![]]);
    catalog.tables[0].name = "users".to_string();
    catalog.tables[0].column_names = vec!["id".to_string(), "age".to_string()];
    let (id, age, other) = (col(0, 0), col(0, 1), col(1, 0));
    let leaf = || Plan::new(Node::Scan(0), vec![]);
    let line = |node: Node| {
        let text = explain(&Plan::new(node, vec![]), &catalog);
        text.trim_end_matches(" (rows=0 cost=0.0)\n").to_string()
    };
    let cmp = |c, v| Cond::Compare(age, c, v);
    check!("every comparison", line(Node::Filter(Cond::All(vec![cmp(Cmp::Eq, 1), cmp(Cmp::Ne, 2), cmp(Cmp::Lt, 3), cmp(Cmp::Le, 4), cmp(Cmp::Gt, -5), cmp(Cmp::Ge, 6)]))),
        "Filter users.age = 1 and users.age <> 2 and users.age < 3 and users.age <= 4 and users.age > -5 and users.age >= 6".to_string());
    let mixed = Cond::Any(vec![Cond::All(vec![cmp(Cmp::Gt, 1), cmp(Cmp::Lt, 9)]), Cond::Same(id, other), Cond::OneOf(id, vec![3, 1])]);
    check!("a list inside a list gets brackets", line(Node::Filter(mixed)), "Filter (users.age > 1 and users.age < 9) or users.id = t1.c0 or users.id in (3, 1)".to_string());
    let deep = Cond::All(vec![Cond::All(vec![cmp(Cmp::Gt, 1), cmp(Cmp::Lt, 9)]), Cond::Any(vec![]), Cond::All(vec![])]);
    check!("same kind inside same kind, and empty lists", line(Node::Filter(deep)), "Filter (users.age > 1 and users.age < 9) and false and true".to_string());
    let single = Cond::All(vec![Cond::Any(vec![Cond::All(vec![cmp(Cmp::Gt, 1), cmp(Cmp::Lt, 9)])]), cmp(Cmp::Eq, 0)]);
    check!("a list of one is written as its part", line(Node::Filter(single)), "Filter (users.age > 1 and users.age < 9) and users.age = 0".to_string());
    check!("a list of one on top", line(Node::Filter(Cond::Any(vec![Cond::All(vec![cmp(Cmp::Gt, 1), cmp(Cmp::Lt, 9)])]))), "Filter users.age > 1 and users.age < 9".to_string());
    check!("scan", line(Node::Scan(0)), "Scan users".to_string());
    check!("open ends", line(Node::IndexScan { table: 0, column: 1, low: 5, high: i64::MAX }), "IndexScan users.age [5..inf]".to_string());
    check!("project", line(Node::Project(vec![age, other, age])), "Project users.age, t1.c0, users.age".to_string());
    check!("sort", line(Node::Sort(id)), "Sort users.id".to_string());
    check!("bare loop", line(Node::NestedLoop(None)), "NestedLoop".to_string());
    check!("loop with a condition", line(Node::NestedLoop(Some(Cond::Same(id, other)))), "NestedLoop users.id = t1.c0".to_string());
    check!("merge", line(Node::MergeJoin { left_key: id, right_key: other, rest: None }), "MergeJoin users.id = t1.c0".to_string());
    let tree = Plan::new(Node::NestedLoop(None), vec![Plan::new(Node::Sort(id), vec![leaf()]), leaf()]);
    check!("inputs are indented under their node, first input first", explain(&tree, &catalog),
        "NestedLoop (rows=0 cost=0.0)\n  Sort users.id (rows=0 cost=0.0)\n    Scan users (rows=0 cost=0.0)\n  Scan users (rows=0 cost=0.0)\n".to_string());
}
