use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn col(table: usize) -> Column {
    Column { table, column: 0 }
}
#[allow(dead_code)]
fn boxed(p: Logical) -> Box<Logical> {
    Box::new(p)
}

/// The leaves under a node, as bits.
#[allow(dead_code)]
fn under(p: &Logical) -> u64 {
    match p {
        Logical::Scan(t) => 1 << t,
        Logical::Filter(_, x) => under(x),
        Logical::Cross(a, b) | Logical::Join(_, a, b) => under(a) | under(b),
    }
}

/// The shape of the tree without any condition.
#[allow(dead_code)]
fn bare(p: &Logical) -> Logical {
    match p {
        Logical::Scan(t) => Logical::Scan(*t),
        Logical::Filter(_, x) => bare(x),
        Logical::Cross(a, b) | Logical::Join(_, a, b) => Logical::Cross(boxed(bare(a)), boxed(bare(b))),
    }
}

#[allow(dead_code)]
fn flat(c: &Cond, out: &mut Vec<String>) {
    match c {
        Cond::All(parts) => parts.iter().for_each(|p| flat(p, out)),
        other => out.push(format!("{other:?}")),
    }
}

#[allow(dead_code)]
fn every_part(p: &Logical, out: &mut Vec<String>) {
    match p {
        Logical::Scan(_) => {}
        Logical::Filter(c, x) => {
            flat(c, out);
            every_part(x, out);
        }
        Logical::Cross(a, b) => {
            every_part(a, out);
            every_part(b, out);
        }
        Logical::Join(c, a, b) => {
            flat(c, out);
            every_part(a, out);
            every_part(b, out);
        }
    }
}

/// The parts of one node's condition, or what is wrong with how it is written.
#[allow(dead_code)]
fn node_parts(c: &Cond) -> Result<Vec<Cond>, String> {
    match c {
        Cond::All(parts) if parts.len() < 2 => Err(format!("a list of {} parts should not be written as All", parts.len())),
        Cond::All(parts) if parts.iter().any(|p| matches!(p, Cond::All(_))) => Err("an All inside an All".to_string()),
        Cond::All(parts) => Ok(parts.clone()),
        other => Ok(vec![other.clone()]),
    }
}

/// Whether the conditions are as low as they can go and written in the required way. A part that reads no table
/// may be on any table.
#[allow(dead_code)]
fn lowest(p: &Logical) -> Result<(), String> {
    match p {
        Logical::Scan(_) => Ok(()),
        Logical::Filter(c, x) => {
            if !matches!(**x, Logical::Scan(_)) {
                return Err("a Filter may only sit directly on a Scan".to_string());
            }
            for part in node_parts(c)? {
                let needs = tables_of(&part);
                if needs & !under(x) != 0 {
                    return Err(format!("{part:?} is on the wrong table"));
                }
            }
            Ok(())
        }
        Logical::Cross(a, b) => lowest(a).and_then(|()| lowest(b)),
        Logical::Join(c, a, b) => {
            for part in node_parts(c)? {
                let needs = tables_of(&part);
                if needs & !(under(a) | under(b)) != 0 {
                    return Err(format!("{part:?} is below a table it needs"));
                }
                if needs & !under(a) == 0 || needs & !under(b) == 0 {
                    return Err(format!("{part:?} can go lower"));
                }
            }
            lowest(a).and_then(|()| lowest(b))
        }
    }
}

#[allow(dead_code)]
fn holds_for(c: &Cond, values: &[i64]) -> bool {
    match c {
        Cond::Compare(col, cmp, v) => {
            let x = values[col.table];
            match cmp {
                Cmp::Eq => x == *v,
                Cmp::Ne => x != *v,
                Cmp::Lt => x < *v,
                Cmp::Le => x <= *v,
                Cmp::Gt => x > *v,
                Cmp::Ge => x >= *v,
            }
        }
        Cond::Same(a, b) => values[a.table] == values[b.table],
        Cond::OneOf(col, list) => list.contains(&values[col.table]),
        Cond::All(parts) => parts.iter().all(|p| holds_for(p, values)),
        Cond::Any(parts) => parts.iter().any(|p| holds_for(p, values)),
    }
}

/// The meaning of a plan when every table has the rows 0, 1 and 2: which combinations survive.
#[allow(dead_code)]
fn meaning(p: &Logical, tables: usize) -> Vec<Vec<i64>> {
    match p {
        Logical::Scan(t) => (0..3)
            .map(|v| {
                let mut row = vec![-1; tables];
                row[*t] = v;
                row
            })
            .collect(),
        Logical::Filter(c, x) => meaning(x, tables).into_iter().filter(|r| holds_for(c, r)).collect(),
        Logical::Cross(a, b) | Logical::Join(_, a, b) => {
            let right = meaning(b, tables);
            let mut out = Vec::new();
            for l in meaning(a, tables) {
                for r in &right {
                    let row: Vec<i64> = l.iter().zip(r).map(|(x, y)| *x.max(y)).collect();
                    if !matches!(p, Logical::Join(c, ..) if !holds_for(c, &row)) {
                        out.push(row);
                    }
                }
            }
            out
        }
    }
}

#[allow(dead_code)]
fn random_cond(rng: &mut Rng, ids: &[usize], depth: usize) -> Cond {
    match rng.below(if depth == 0 { 2 } else { 4 }) {
        0 => Cond::Compare(col(*rng.pick(ids)), *rng.pick(&[Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Ge]), rng.int(0, 2)),
        1 => Cond::Same(col(*rng.pick(ids)), col(*rng.pick(ids))),
        2 => {
            let n = rng.below(3);
            Cond::Any((0..n).map(|_| random_cond(rng, ids, depth - 1)).collect())
        }
        _ => {
            let n = rng.below(4);
            Cond::All((0..n).map(|_| random_cond(rng, ids, depth - 1)).collect())
        }
    }
}

/// A tree over `ids` whose conditions are legal where they stand: each reads only tables below it.
#[allow(dead_code)]
fn random_plan(rng: &mut Rng, ids: &[usize]) -> Logical {
    let mut plan = if ids.len() == 1 {
        Logical::Scan(ids[0])
    } else {
        let cut = 1 + rng.below(ids.len() - 1);
        let (a, b) = (random_plan(rng, &ids[..cut]), random_plan(rng, &ids[cut..]));
        if rng.below(3) == 0 {
            Logical::Join(random_cond(rng, ids, 2), boxed(a), boxed(b))
        } else {
            Logical::Cross(boxed(a), boxed(b))
        }
    };
    while rng.below(3) == 0 {
        plan = Logical::Filter(random_cond(rng, ids, 2), boxed(plan));
    }
    plan
}

/// The same plan with the parts of every node in one fixed order.
#[allow(dead_code)]
fn in_one_order(p: &Logical) -> Logical {
    let tidy = |c: &Cond| match c {
        Cond::All(parts) => {
            let mut parts = parts.clone();
            parts.sort_by_key(|p| format!("{p:?}"));
            Cond::All(parts)
        }
        other => other.clone(),
    };
    match p {
        Logical::Scan(t) => Logical::Scan(*t),
        Logical::Filter(c, x) => Logical::Filter(tidy(c), boxed(in_one_order(x))),
        Logical::Cross(a, b) => Logical::Cross(boxed(in_one_order(a)), boxed(in_one_order(b))),
        Logical::Join(c, a, b) => Logical::Join(tidy(c), boxed(in_one_order(a)), boxed(in_one_order(b))),
    }
}

#[test]
fn random_plans_keep_their_meaning_and_nothing_can_go_lower() {
    let mut rng = Rng::new(9301);
    for _ in 0..700 {
        let n = 1 + rng.below(4);
        let mut ids: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut ids);
        let plan = random_plan(&mut rng, &ids);
        let pushed = push_down(&plan);
        let input = format!("push_down({plan:?}) = {pushed:?}");
        check!(format!("{input}: same tree of tables"), bare(&pushed), bare(&plan));
        let (mut before, mut after) = (Vec::new(), Vec::new());
        every_part(&plan, &mut before);
        every_part(&pushed, &mut after);
        before.sort();
        after.sort();
        check!(format!("{input}: the same parts, none lost or repeated"), after, before);
        check!(format!("{input}: placement"), lowest(&pushed), Ok(()));
        check!(format!("{input}: the same combinations survive"), meaning(&pushed, n), meaning(&plan, n));
        let again = push_down(&pushed);
        check!(format!("{input}: running it again changes only the order inside a node"), in_one_order(&again), in_one_order(&pushed));
    }
}

#[test]
fn a_condition_that_reads_no_table_ends_up_on_some_table() {
    let never = Cond::Any(vec![]);
    let plan = Logical::Filter(
        never.clone(),
        boxed(Logical::Cross(boxed(Logical::Cross(boxed(Logical::Scan(2)), boxed(Logical::Scan(0)))), boxed(Logical::Scan(1)))),
    );
    let pushed = push_down(&plan);
    let mut parts = Vec::new();
    every_part(&pushed, &mut parts);
    check!(format!("any of nothing: {pushed:?}"), (parts, lowest(&pushed)), (vec![format!("{never:?}")], Ok(())));
    let always = Logical::Filter(Cond::All(vec![Cond::All(vec![])]), boxed(Logical::Scan(0)));
    check!("all of nothing disappears", push_down(&always), Logical::Scan(0));
}

#[test]
fn an_or_is_never_taken_apart() {
    let either = Cond::Any(vec![Cond::Compare(col(0), Cmp::Eq, 1), Cond::Compare(col(1), Cmp::Eq, 1)]);
    let plan = Logical::Filter(either.clone(), boxed(Logical::Cross(boxed(Logical::Scan(0)), boxed(Logical::Scan(1)))));
    check!("t0 = 1 or t1 = 1", push_down(&plan), Logical::Join(either, boxed(Logical::Scan(0)), boxed(Logical::Scan(1))));
}

#[test]
fn table_sixty_three_has_a_bit_of_its_own() {
    check!("t63", tables_of(&Cond::Compare(col(63), Cmp::Eq, 0)), 1u64 << 63);
    let plan = Logical::Filter(Cond::Same(col(63), col(31)), boxed(Logical::Cross(boxed(Logical::Scan(31)), boxed(Logical::Scan(63)))));
    let want = Logical::Join(Cond::Same(col(63), col(31)), boxed(Logical::Scan(31)), boxed(Logical::Scan(63)));
    check!("tables 31 and 63", push_down(&plan), want);
}

#[test]
fn five_hundred_stacked_filters() {
    let mut plan = Logical::Scan(0);
    for v in 0..500 {
        plan = Logical::Filter(Cond::Compare(col(0), Cmp::Ne, v), boxed(plan));
    }
    let pushed = push_down(&plan);
    let mut parts = Vec::new();
    every_part(&pushed, &mut parts);
    check!("500 parts under one Filter", (parts.len(), lowest(&pushed)), (500, Ok(())));
}

#[test]
fn conditions_on_old_joins_move_too() {
    let low = Cond::Compare(col(0), Cmp::Lt, 5);
    let on = Cond::Same(col(2), col(0));
    let old = Logical::Join(low.clone(), boxed(Logical::Scan(0)), boxed(Logical::Scan(1)));
    let plan = Logical::Filter(on.clone(), boxed(Logical::Cross(boxed(old), boxed(Logical::Scan(2)))));
    let want = Logical::Join(
        on,
        boxed(Logical::Cross(boxed(Logical::Filter(low, boxed(Logical::Scan(0)))), boxed(Logical::Scan(1)))),
        boxed(Logical::Scan(2)),
    );
    check!(format!("{plan:?}"), push_down(&plan), want);
}

#[test]
fn a_bushy_tree() {
    let pair = |a, b| Logical::Cross(boxed(Logical::Scan(a)), boxed(Logical::Scan(b)));
    let plan = Logical::Filter(
        Cond::All(vec![Cond::Same(col(3), col(2)), Cond::Same(col(0), col(1)), Cond::Same(col(1), col(3))]),
        boxed(Logical::Cross(boxed(pair(0, 1)), boxed(pair(2, 3)))),
    );
    let side = |on, a, b| Logical::Join(on, boxed(Logical::Scan(a)), boxed(Logical::Scan(b)));
    let want = Logical::Join(
        Cond::Same(col(1), col(3)),
        boxed(side(Cond::Same(col(0), col(1)), 0, 1)),
        boxed(side(Cond::Same(col(3), col(2)), 2, 3)),
    );
    check!("two pairs and one condition between them", push_down(&plan), want);
}

#[test]
fn a_condition_over_three_tables_waits_for_all_three() {
    let three = Cond::Any(vec![Cond::Same(col(0), col(1)), Cond::Same(col(1), col(2))]);
    let tree = |a, b| Logical::Cross(boxed(Logical::Cross(boxed(a), boxed(b))), boxed(Logical::Scan(2)));
    let plan = Logical::Filter(three.clone(), boxed(tree(Logical::Scan(0), Logical::Scan(1))));
    let want = Logical::Join(three, boxed(Logical::Cross(boxed(Logical::Scan(0)), boxed(Logical::Scan(1)))), boxed(Logical::Scan(2)));
    check!("t0 = t1 or t1 = t2", push_down(&plan), want);
}
