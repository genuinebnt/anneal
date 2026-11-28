use anneal_prelude::Rng;
use solution::*;

/// Every position where a match of `ast` that begins at `at` can end. Slow, and plainly right.
#[allow(dead_code)]
fn ends(ast: &Ast, text: &[char], at: usize) -> std::collections::BTreeSet<usize> {
    use std::collections::BTreeSet;
    let one = |ok: bool| ok.then_some(at + 1).into_iter().collect::<BTreeSet<_>>();
    let nothing = |ok: bool| ok.then_some(at).into_iter().collect::<BTreeSet<_>>();
    // All positions reachable from `from` by any number of further rounds of `inner`.
    let rounds = |inner: &Ast, from: BTreeSet<usize>| {
        let mut all = from.clone();
        let mut work: Vec<usize> = from.into_iter().collect();
        while let Some(p) = work.pop() {
            for q in ends(inner, text, p) {
                if all.insert(q) {
                    work.push(q);
                }
            }
        }
        all
    };
    match ast {
        Ast::Empty => nothing(true),
        Ast::Class(c) => one(text.get(at).is_some_and(|&x| c.matches(x))),
        Ast::Any => one(at < text.len()),
        Ast::Start => nothing(at == 0),
        Ast::End => nothing(at == text.len()),
        Ast::Cat(items) => items.iter().fold(nothing(true), |here, item| {
            here.into_iter().flat_map(|p| ends(item, text, p)).collect()
        }),
        Ast::Alt(branches) => branches.iter().flat_map(|b| ends(b, text, at)).collect(),
        Ast::Star(inner) => rounds(inner, nothing(true)),
        Ast::Plus(inner) => rounds(inner, ends(inner, text, at)),
        Ast::Opt(inner) => {
            let mut all = ends(inner, text, at);
            all.insert(at);
            all
        }
    }
}

#[allow(dead_code)]
fn slow_is_match(ast: &Ast, text: &str) -> bool {
    let t: Vec<char> = text.chars().collect();
    ends(ast, &t, 0).contains(&t.len())
}

#[allow(dead_code)]
fn slow_find(ast: &Ast, text: &str) -> Option<(usize, usize)> {
    let t: Vec<char> = text.chars().collect();
    (0..=t.len()).find_map(|s| ends(ast, &t, s).last().map(|&e| (s, e)))
}

/// Random pattern text. Every piece it builds can safely be joined or repeated.
#[allow(dead_code)]
fn random_pattern(rng: &mut Rng, depth: u32) -> String {
    if depth == 0 || rng.below(5) == 0 {
        let atom = *rng.pick(&["a", "a", "b", "b", "c", ".", "[ab]", "[^a]", "[a-b]", r"\w", "^", "$", "()"]);
        return if rng.below(3) == 0 {
            format!("{atom}{}", rng.pick(&["*", "+", "?"]))
        } else {
            atom.to_string()
        };
    }
    let a = random_pattern(rng, depth - 1);
    let b = random_pattern(rng, depth - 1);
    match rng.below(7) {
        0 | 1 => format!("{a}{b}"),
        2 => format!("({a}|{b})"),
        3 => format!("({a})*"),
        4 => format!("({a})+"),
        5 => format!("({a})?"),
        _ => format!("{a}|{b}"),
    }
}

#[allow(dead_code)]
fn random_text(rng: &mut Rng) -> String {
    let n = rng.below(7);
    rng.string(n, "aabbc\n_")
}

#[allow(dead_code)]
const PLENTY: u64 = 2_000_000;

#[allow(dead_code)]
fn bm(pattern: &str, text: &str, limit: u64) -> Result<bool, StepLimit> {
    backtrack_is_match(&parse(pattern).expect("a valid pattern"), text, limit)
}

#[allow(dead_code)]
fn bf(pattern: &str, text: &str, limit: u64) -> Result<Option<(usize, usize)>, StepLimit> {
    backtrack_find(&parse(pattern).expect("a valid pattern"), text, limit)
}

#[test]
fn giving_up_is_never_an_answer() {
    check!("no steps at all", bm("a", "a", 0), Err(StepLimit));
    check!("find with no steps", bf("a", "a", 0), Err(StepLimit));
    let long = "a".repeat(40);
    for pattern in ["(a*)*b", "(a|a)*b", "(a|aa)*c", "(a+a+)+b"] {
        check!(format!("is_match {pattern}"), bm(pattern, &long, 500_000), Err(StepLimit));
        check!(format!("find {pattern}"), bf(pattern, &long, 500_000), Err(StepLimit));
    }
}

#[test]
fn small_limits_never_lie() {
    let mut rng = Rng::new(1616);
    for _ in 0..3000 {
        let pattern = random_pattern(&mut rng, 3);
        let tree = parse(&pattern).unwrap();
        let text = random_text(&mut rng);
        let limit = rng.below(40) as u64;
        if let Ok(got) = backtrack_is_match(&tree, &text, limit) {
            check!(format!("is_match {pattern:?} on {text:?} within {limit}"), got, slow_is_match(&tree, &text));
        }
        if let Ok(got) = backtrack_find(&tree, &text, limit) {
            check!(format!("find {pattern:?} in {text:?} within {limit}"), got, slow_find(&tree, &text));
        }
    }
}

#[test]
fn anchors_use_the_real_text() {
    check!("^b in ab", bf("^b", "ab", PLENTY), Ok(None));
    check!("a$ in aaba", bf("a$", "aaba", PLENTY), Ok(Some((3, 4))));
    check!("a(^|$)", bm("a(^|$)", "a", PLENTY), Ok(true));
}

#[test]
fn an_empty_round_is_refused_but_other_ways_are_still_tried() {
    check!("the round's second choice reads", bm("(()|a)*b", "aaab", PLENTY), Ok(true));
    check!("find sees the longest", bf("(()|a)+", "aaab", PLENTY), Ok(Some((0, 3))));
}

#[test]
fn a_generous_limit_is_enough_for_ordinary_work() {
    check!("60 letters, a few steps each", bm("(a|b)*c", &("ab".repeat(29) + "c"), 2_000), Ok(true));
    check!("a plain word", bf("abc", &("x".repeat(57) + "abc"), 2_000), Ok(Some((57, 60))));
}

#[test]
fn is_match_stops_at_the_first_way() {
    let a = "a".repeat(40);
    check!("(a|aa)* on a^40: the first way works", bm("(a|aa)*", &a, 5_000), Ok(true));
    check!("(a*)* on a^40", bm("(a*)*", &a, 5_000), Ok(true));
    // Stopping the first loop early leaves the second loop an impossible job.
    check!("a*((a|a)*b)? on a^40", bm("a*((a|a)*b)?", &a, 5_000), Ok(true));
}

#[test]
fn the_limit_covers_every_start_together() {
    // Each start alone is cheap; all sixty of them are not.
    let text = "a".repeat(60);
    check!("a*a*b over 60 starts", bf("a*a*b", &text, 20_000), Err(StepLimit));
    check!("with room to finish", bf("a*a*b", &text, 5_000_000), Ok(None));
}

#[test]
fn random_patterns_agree_with_the_tree() {
    let mut rng = Rng::new(1606);
    let (mut compared, mut yes) = (0, 0);
    for _ in 0..2000 {
        let pattern = random_pattern(&mut rng, 3);
        let tree = parse(&pattern).unwrap();
        for _ in 0..6 {
            let text = random_text(&mut rng);
            if let Ok(got) = backtrack_is_match(&tree, &text, 200_000) {
                compared += 1;
                let want = slow_is_match(&tree, &text);
                yes += u32::from(want);
                check!(format!("is_match {pattern:?} on {text:?}"), got, want);
            }
            if let Ok(got) = backtrack_find(&tree, &text, 200_000) {
                compared += 1;
                check!(format!("find {pattern:?} in {text:?}"), got, slow_find(&tree, &text));
            }
        }
    }
    check!(format!("you may give up only rarely: {compared} of 24000 answered, {yes} matches"), compared > 23_000 && yes > 500, true);
}

#[test]
fn the_question_mark_tries_the_item_first() {
    // Taking the `x` leads straight to `bb*`. Skipping it first sends `(b*)*c` over thirty b's, which never ends in time.
    let text = format!("x{}", "b".repeat(30));
    check!("x?(x(b*)*c|bb*) on x and 30 b's", bm("x?(x(b*)*c|bb*)", &text, 1_000_000), Ok(true));
}
