use anneal_prelude::Rng;
use solution::*;

fn not(rule: Rule) -> Rule {
    Rule::Not(Box::new(rule))
}

#[test]
fn two_gaps_are_not_known_to_be_equal() {
    check!("two blanks", judge(&Rule::Same(0, 1), &[None, None]), Verdict::Unknown);
    check!("the same blank", judge(&Rule::Same(0, 0), &[None]), Verdict::Unknown);
    check!("one blank", judge(&Rule::Same(0, 1), &[Some(1), None]), Verdict::Unknown);
    check!("both given", judge(&Rule::Same(0, 1), &[Some(1), Some(1)]), Verdict::Yes);
    check!("both given, different", judge(&Rule::Same(0, 1), &[Some(1), Some(2)]), Verdict::No);
}

#[test]
fn nothing_to_combine() {
    check!("all of nothing", judge(&Rule::All(vec![]), &[]), Verdict::Yes);
    check!("any of nothing", judge(&Rule::Any(vec![]), &[]), Verdict::No);
}

#[test]
fn the_unknown_part_may_come_on_either_side() {
    let gap = Rule::AtLeast(9, 0);
    let yes = Rule::All(vec![]);
    let no = Rule::Any(vec![]);
    for parts in [vec![gap.clone(), no.clone()], vec![no.clone(), gap.clone()]] {
        check!(format!("All({parts:?})"), judge(&Rule::All(parts.clone()), &[]), Verdict::No);
    }
    for parts in [vec![gap.clone(), yes.clone()], vec![yes.clone(), gap.clone()]] {
        check!(format!("Any({parts:?})"), judge(&Rule::Any(parts.clone()), &[]), Verdict::Yes);
    }
}

#[test]
fn a_rule_is_not_reasoned_about() {
    let either = Rule::Any(vec![Rule::AtLeast(0, 5), not(Rule::AtLeast(0, 5))]);
    check!("at least 5, or not", judge(&either, &[None]), Verdict::Unknown);
}

#[test]
fn refused_forms_are_in_neither_list() {
    let forms = vec![vec![Some(1)], vec![], vec![Some(0)]];
    check!("lists", sort_forms(&not(Rule::AtLeast(0, 1)), &forms), (vec![2], vec![1]));
    check!("no forms", sort_forms(&Rule::All(vec![]), &[]), (vec![], vec![]));
}

/// A random rule with its verdict worked out as a number: no 0, unknown 1, yes 2. Then "all" is the smallest, "any" is
/// the largest, and "not" counts from the other end.
fn rule(rng: &mut Rng, answers: &[Option<i64>], depth: u32) -> (Rule, u8) {
    let get = |q: usize| if q < answers.len() { answers[q] } else { None };
    let kind = rng.below(if depth == 0 { 3 } else { 6 });
    match kind {
        0 => {
            let (q, n) = (rng.below(4), rng.int(-2, 2));
            (Rule::AtLeast(q, n), get(q).map_or(1, |a| if a >= n { 2 } else { 0 }))
        }
        1 => {
            let (a, b) = (rng.below(4), rng.below(4));
            let worth = match (get(a), get(b)) {
                (Some(x), Some(y)) => 2 * u8::from(x == y),
                _ => 1,
            };
            (Rule::Same(a, b), worth)
        }
        2 => {
            let q = rng.below(4);
            (Rule::Missing(q), 2 * u8::from(get(q).is_none()))
        }
        3 => {
            let (of, worth) = rule(rng, answers, depth - 1);
            (not(of), 2 - worth)
        }
        _ => {
            let parts: Vec<(Rule, u8)> = (0..rng.below(4)).map(|_| rule(rng, answers, depth - 1)).collect();
            let worths = parts.iter().map(|p| p.1);
            let worth = if kind == 4 { worths.min().unwrap_or(2) } else { worths.max().unwrap_or(0) };
            let parts = parts.into_iter().map(|p| p.0).collect();
            (if kind == 4 { Rule::All(parts) } else { Rule::Any(parts) }, worth)
        }
    }
}

#[test]
fn random_rules() {
    let mut rng = Rng::new(18141);
    for _ in 0..3000 {
        let len = rng.below(4);
        let answers: Vec<Option<i64>> = (0..len).map(|_| (rng.below(3) != 0).then(|| rng.int(-2, 2))).collect();
        let (rule, worth) = rule(&mut rng, &answers, 4);
        let want = [Verdict::No, Verdict::Unknown, Verdict::Yes][usize::from(worth)];
        check!(format!("{rule:?} on {answers:?}"), judge(&rule, &answers), want);
    }
}

#[test]
fn a_deep_rule() {
    let mut deep = Rule::AtLeast(0, 0);
    for _ in 0..99 {
        deep = not(deep);
    }
    check!("99 times not", judge(&deep, &[Some(1)]), Verdict::No);
}

#[test]
fn many_forms() {
    let forms: Vec<Vec<Option<i64>>> = (0..50_000).map(|i| vec![(i % 3 != 0).then_some(i % 5)]).collect();
    let (accept, back) = sort_forms(&Rule::AtLeast(0, 1), &forms);
    let want_back: Vec<usize> = (0..50_000).filter(|i| i % 3 == 0).collect();
    check!("sent back", back, want_back);
    check!("accepted", accept.len(), (0..50_000).filter(|i| i % 3 != 0 && i % 5 >= 1).count());
}
