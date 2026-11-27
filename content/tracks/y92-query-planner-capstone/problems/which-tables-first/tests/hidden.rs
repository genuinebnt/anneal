use solution::*;
use anneal_prelude::Rng;
use std::collections::HashMap;

#[allow(dead_code)]
fn col(table: usize, column: usize) -> Column {
    Column { table, column }
}

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// A made-up join operator. It has up to three ways to join two plans. Their costs and orders depend on both
/// inputs' orders, on which side each table is, on the conditions and on `seed`, so cheap guesses go wrong.
#[allow(dead_code)]
fn made_up(seed: u64, a: &Plan, b: &Plan, on: &[Cond], rows: f64) -> Vec<Plan> {
    let order_code = |o: Option<Column>| o.map_or(0, |c| 1 + c.table as u64 * 3 + c.column as u64);
    let mix = |x: u64| {
        let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let base = mix(mix(mix(seed ^ plan_tables(a)) ^ plan_tables(b)) ^ ((order_code(a.order) << 8) | order_code(b.order)));
    let ways = 1 + (mix(base ^ 99) % 3) as usize;
    (0..ways)
        .map(|k| {
            let r = mix(base ^ (k as u64 + 1));
            let order = match k {
                0 => a.order,
                1 => None,
                _ => Some(col(0, (r % 2) as usize)),
            };
            // the made-up plan only has to say which tables it reads, which keeps the tests quick
            let tables = plan_tables(a) | plan_tables(b);
            let scans = (0..64).filter(|t| tables >> t & 1 == 1).map(|t| Plan::new(Node::Scan(t), vec![])).collect();
            let mut plan = Plan::new(Node::NestedLoop(None), scans);
            plan.rows = rows;
            plan.order = order;
            plan.cost = a.cost + b.cost + (r % 1000) as f64 + rows.min(1e6) * 0.001 + on.len() as f64;
            plan
        })
        .collect()
}

/// A graph whose conditions are numbered, so that a join can say which ones it was given.
#[allow(dead_code)]
fn random_graph(rng: &mut Rng, n: usize) -> JoinGraph {
    let mut leaf_rows = Vec::new();
    let leaves = (0..n)
        .map(|t| {
            let rows = rng.int(1, 200) as f64;
            leaf_rows.push(rows);
            let mut set = PlanSet::new();
            for k in 0..1 + rng.below(2) {
                let mut plan = Plan::new(Node::Scan(t), vec![]);
                (plan.rows, plan.cost) = (rows, rng.int(1, 500) as f64);
                // only two orders exist, so that few plans are worth keeping and the tests stay quick
                plan.order = (k == 1).then(|| col(0, t % 2));
                set.insert(plan);
            }
            set
        })
        .collect();
    let mut edges: Vec<Edge> = Vec::new();
    let add = |bits: u64, edges: &mut Vec<Edge>, s: f64| {
        let id = edges.len() as i64;
        edges.push(Edge { leaves: bits, condition: Cond::Compare(col(0, 0), Cmp::Eq, id), selectivity: s });
    };
    if n > 1 {
        let style = rng.below(5);
        for i in 1..n {
            // style 0 leaves some tables unconnected
            if style == 0 && rng.bool() {
                continue;
            }
            let j = rng.below(i);
            add((1 << i) | (1 << j), &mut edges, 1.0 / rng.int(1, 100) as f64);
        }
        for _ in 0..rng.below(3) {
            let (i, j) = (rng.below(n), rng.below(n));
            if i != j {
                add((1 << i) | (1 << j), &mut edges, 1.0 / rng.int(1, 20) as f64);
            }
        }
        if style == 1 && n > 2 {
            // one condition over three tables
            let mut ids: Vec<usize> = (0..n).collect();
            rng.shuffle(&mut ids);
            if rng.bool() {
                edges.clear();
            }
            add((1 << ids[0]) | (1 << ids[1]) | (1 << ids[2]), &mut edges, 0.5);
        }
    }
    JoinGraph { leaves, leaf_rows, edges }
}

#[allow(dead_code)]
fn crossing(graph: &JoinGraph, a: u64, b: u64) -> Vec<Cond> {
    graph
        .edges
        .iter()
        .filter(|e| e.leaves & !(a | b) == 0 && e.leaves & !a != 0 && e.leaves & !b != 0)
        .map(|e| e.condition.clone())
        .collect()
}

#[allow(dead_code)]
fn own_rows(graph: &JoinGraph, set: u64) -> f64 {
    let mut rows = 1.0;
    for (i, r) in graph.leaf_rows.iter().enumerate() {
        if set >> i & 1 == 1 {
            rows *= r;
        }
    }
    for e in &graph.edges {
        if e.leaves & set == e.leaves {
            rows *= e.selectivity;
        }
    }
    rows
}

/// Every plan of every tree for a set of tables, with nothing thrown away.
#[allow(dead_code)]
fn every_plan(graph: &JoinGraph, set: u64, cross: bool, seed: u64) -> Vec<Plan> {
    if set.count_ones() == 1 {
        return graph.leaves[set.trailing_zeros() as usize].plans().to_vec();
    }
    let mut out = Vec::new();
    for a in 1..set {
        if a & set != a {
            continue;
        }
        let b = set - a;
        let on = crossing(graph, a, b);
        if on.is_empty() && !cross {
            continue;
        }
        let right = every_plan(graph, b, cross, seed);
        for x in every_plan(graph, a, cross, seed) {
            for y in &right {
                out.extend(made_up(seed, &x, y, &on, own_rows(graph, set)));
            }
        }
    }
    out
}

/// The cheapest cost for each output order.
#[allow(dead_code)]
fn by_order(plans: &[Plan]) -> Vec<(Option<Column>, f64)> {
    let mut best: HashMap<Option<Column>, f64> = HashMap::new();
    for p in plans {
        let c = best.entry(p.order).or_insert(f64::INFINITY);
        *c = c.min(p.cost);
    }
    let mut out: Vec<_> = best.into_iter().collect();
    out.sort_by_key(|e| e.0);
    out
}

#[test]
fn against_every_tree_on_small_graphs() {
    let mut rng = Rng::new(9801);
    for case in 0..250 {
        let n = 1 + rng.below(4);
        let graph = random_graph(&mut rng, n);
        let seed = rng.next_u64();
        let full = (1u64 << n) - 1;
        let mut wrong: Option<String> = None;
        let mut join = |a: &Plan, b: &Plan, on: &[Cond], rows: f64| {
            let (x, y) = (plan_tables(a), plan_tables(b));
            let mut got: Vec<String> = on.iter().map(|c| format!("{c:?}")).collect();
            let mut want: Vec<String> = crossing(&graph, x, y).iter().map(|c| format!("{c:?}")).collect();
            got.sort();
            want.sort();
            if x & y != 0 || got != want || !close(rows, own_rows(&graph, x | y)) {
                wrong.get_or_insert(format!("join of tables {x:#b} and {y:#b} got conditions {got:?} and {rows} rows; wanted {want:?} and {}", own_rows(&graph, x | y)));
            }
            made_up(seed, a, b, on, rows)
        };
        let found = best_order(&graph, &mut join);
        let input = format!("case {case}: {n} tables, rows {:?}, edges {:?}", graph.leaf_rows, graph.edges.iter().map(|e| (e.leaves, e.selectivity)).collect::<Vec<_>>());
        check!(format!("{input}: what the join was asked"), wrong, None);
        let mut oracle = every_plan(&graph, full, false, seed);
        if oracle.is_empty() {
            oracle = every_plan(&graph, full, true, seed);
        }
        let got = by_order(found.plans());
        let want = by_order(&oracle);
        let same = got.len() == want.len() && got.iter().zip(&want).all(|(g, w)| g.0 == w.0 && close(g.1, w.1));
        check!(format!("{input}: (order, cheapest cost) was {got:?}, over every tree it is {want:?}"), same, true);
        check!(format!("{input}: one plan for each order"), found.plans().len(), want.len());
    }
}

#[test]
fn rows_of_matches_the_definition() {
    let mut rng = Rng::new(9802);
    for case in 0..300 {
        let n = 1 + rng.below(6);
        let graph = random_graph(&mut rng, n);
        for set in 1..(1u64 << n) {
            let (got, want) = (rows_of(&graph, set), own_rows(&graph, set));
            check!(format!("case {case}, set {set:#b}: got {got}, wanted {want}"), close(got, want), true);
        }
    }
}

/// How many times a search that keeps one best plan per order, for each set, calls the join: once for every
/// pair of kept plans in every allowed split.
#[allow(dead_code)]
fn fair_calls(graph: &JoinGraph, seed: u64) -> usize {
    let n = graph.leaves.len();
    let mut calls = 0;
    for cross in [false, true] {
        let mut kept: Vec<Vec<Plan>> = vec![Vec::new(); 1 << n];
        for i in 0..n {
            kept[1 << i] = graph.leaves[i].plans().to_vec();
        }
        for set in 1..(1u64 << n) {
            if set.count_ones() < 2 {
                continue;
            }
            let mut found = PlanSet::new();
            for a in 1..set {
                let b = set ^ a;
                if a & set != a {
                    continue;
                }
                let on = crossing(graph, a, b);
                if on.is_empty() && !cross {
                    continue;
                }
                for x in &kept[a as usize] {
                    for y in &kept[b as usize] {
                        calls += 1;
                        for p in made_up(seed, x, y, &on, own_rows(graph, set)) {
                            found.insert(p);
                        }
                    }
                }
            }
            kept[set as usize] = found.plans().to_vec();
        }
        if !kept[(1 << n) - 1].is_empty() {
            break;
        }
    }
    calls
}

/// Runs the search with a limit on how often it may call the join. Going over fails at once.
#[allow(dead_code)]
fn within_budget(graph: &JoinGraph, seed: u64) -> PlanSet {
    let budget = 2 * fair_calls(graph, seed);
    let mut calls = 0;
    let mut join = |a: &Plan, b: &Plan, on: &[Cond], rows: f64| {
        calls += 1;
        assert!(calls <= budget, "the join was called more than {budget} times for {} tables", graph.leaves.len());
        made_up(seed, a, b, on, rows)
    };
    best_order(graph, &mut join)
}

#[test]
fn seven_tables_within_the_call_budget() {
    let mut rng = Rng::new(9803);
    for _ in 0..8 {
        let graph = random_graph(&mut rng, 7);
        let found = within_budget(&graph, rng.next_u64());
        check!("a plan over all seven", found.cheapest().map(plan_tables), Some(127));
    }
}

#[allow(dead_code)]
fn shaped(n: usize, pairs: &[(usize, usize)]) -> JoinGraph {
    let mut rng = Rng::new(n as u64 + pairs.len() as u64);
    let mut graph = random_graph(&mut rng, n);
    graph.edges = pairs
        .iter()
        .map(|&(a, b)| Edge { leaves: (1 << a) | (1 << b), condition: Cond::Same(col(a, 0), col(b, 0)), selectivity: 0.05 })
        .collect();
    graph
}

#[test]
fn ten_tables_in_a_chain_and_a_star() {
    let chain: Vec<(usize, usize)> = (1..MAX_TABLES).map(|i| (i - 1, i)).collect();
    let star: Vec<(usize, usize)> = (1..MAX_TABLES).map(|i| (0, i)).collect();
    for pairs in [chain, star] {
        let graph = shaped(MAX_TABLES, &pairs);
        let found = within_budget(&graph, 7);
        check!("all ten tables", found.cheapest().map(plan_tables), Some((1u64 << MAX_TABLES) - 1));
    }
}

#[test]
fn eight_tables_with_no_conditions_at_all() {
    let graph = shaped(8, &[]);
    let found = within_budget(&graph, 11);
    check!("all eight", found.cheapest().map(plan_tables), Some(255));
}

#[test]
fn no_tables_and_too_many_tables() {
    let empty = JoinGraph { leaves: vec![], leaf_rows: vec![], edges: vec![] };
    check!("no tables", within_budget(&empty, 1).plans().len(), 0);
    let mut rng = Rng::new(9804);
    for n in [MAX_TABLES + 1, 40, 64] {
        let graph = random_graph(&mut rng, n.min(40));
        let mut join = |_: &Plan, _: &Plan, _: &[Cond], _: f64| -> Vec<Plan> { panic!("too many tables: the join must not be called") };
        check!(format!("{} tables", graph.leaves.len()), best_order(&graph, &mut join).plans().len(), 0);
    }
}

#[test]
fn two_pairs_joined_to_each_other() {
    // two big tables, each cut down by a small one: the cheapest tree joins the two pairs, not one table at a time
    let leaf = |t: usize, rows: f64| {
        let mut plan = Plan::new(Node::Scan(t), vec![]);
        plan.rows = rows;
        let mut set = PlanSet::new();
        set.insert(plan);
        set
    };
    let edge = |a: usize, b: usize, s: f64| Edge { leaves: (1 << a) | (1 << b), condition: Cond::Same(col(a, 0), col(b, 0)), selectivity: s };
    let sizes = [10.0, 10000.0, 10000.0, 10.0];
    let graph = JoinGraph {
        leaves: (0..4).map(|i| leaf(i, sizes[i])).collect(),
        leaf_rows: sizes.to_vec(),
        edges: vec![edge(0, 1, 0.0001), edge(1, 2, 0.01), edge(2, 3, 0.0001)],
    };
    // a join costs its output rows, whatever the method
    let mut join = |a: &Plan, b: &Plan, _: &[Cond], rows: f64| {
        let mut plan = Plan::new(Node::NestedLoop(None), vec![a.clone(), b.clone()]);
        plan.rows = rows;
        plan.cost = a.cost + b.cost + rows;
        vec![plan]
    };
    let best = best_order(&graph, &mut join);
    let cost = best.cheapest().map(|p| p.cost.round());
    check!("10 rows, 10 rows, then 1 row", cost, Some(21.0));
}

#[test]
fn a_condition_over_three_tables_needs_the_second_search() {
    let mut rng = Rng::new(9805);
    let mut graph = random_graph(&mut rng, 3);
    graph.edges = vec![Edge { leaves: 7, condition: Cond::All(vec![]), selectivity: 0.5 }];
    let found = within_budget(&graph, 3);
    check!("a plan over all three", found.cheapest().map(plan_tables), Some(7));
}
