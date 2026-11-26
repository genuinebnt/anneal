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
fn m(pattern: &str, text: &str) -> bool {
    is_match(&compile(&parse(pattern).expect("a valid pattern")), text)
}

#[test]
fn anchors_are_judged_where_the_machine_stands() {
    check!("an end anchor before the last letter", m("a$a", "aa"), false);
    check!("a start anchor after the first letter", m("a^a", "aa"), false);
    check!("an end anchor reached with the last letter", m("ab$", "ab"), true);
    check!("anchors inside loops", (m("(^a)*", "a"), m("(^a)*", "aa"), m("(a$)*", "a"), m("(a$|b)*", "bba"), m("(a$|b)*", "bab")), (true, false, true, true, false));
    check!("both anchors in the middle of branches", (m("x|^y$|z", "y"), m(".($|.)", "a"), m(".(^|.)", "a")), (true, true, false));
}

#[test]
fn a_text_with_a_wrong_character_early() {
    check!("dies and stays dead", m("abc", "xbcabc"), false);
}

#[test]
fn characters_not_bytes() {
    check!("one dot per character", (m("..", "é€"), m("...", "é€"), m("[é-ü]+", "éü"), m(".$", "€")), (true, false, true, true));
}

#[test]
fn random_patterns_agree_with_the_tree() {
    let mut rng = Rng::new(1604);
    let mut yes = 0;
    for _ in 0..3000 {
        let pattern = random_pattern(&mut rng, 3);
        let tree = parse(&pattern).unwrap();
        let nfa = compile(&tree);
        for _ in 0..8 {
            let text = random_text(&mut rng);
            let want = slow_is_match(&tree, &text);
            yes += u32::from(want);
            check!(format!("{pattern:?} on {text:?}"), is_match(&nfa, &text), want);
        }
    }
    check!(format!("the comparison includes many matches ({yes})"), yes > 1000, true);
}

#[test]
fn patterns_that_blow_up_other_engines() {
    let a = "a".repeat(50_000);
    check!("(a*)*b", m("(a*)*b", &a), false);
    check!("(a|a)*b", m("(a|a)*b", &a), false);
    check!("(a|aa)*c", m("(a|aa)*c", &a), false);
    check!("(a*)*b with the b", m("(a*)*b", &(a.clone() + "b")), true);
    check!("(a+a+)+b", m("(a+a+)+b", &a), false);
}

#[test]
fn many_optional_letters() {
    let pattern = "a?".repeat(40) + &"a".repeat(40);
    check!("a?^40 a^40 on a^40", m(&pattern, &"a".repeat(40)), true);
    check!("on a^39", m(&pattern, &"a".repeat(39)), false);
}

#[test]
fn one_machine_can_be_used_again() {
    let nfa = compile(&parse("a+b").unwrap());
    let got: Vec<bool> = ["ab", "b", "aab", "aab", ""].iter().map(|t| is_match(&nfa, t)).collect();
    check!("five texts in a row", got, vec![true, false, true, true, false]);
}

#[test]
fn a_large_machine_on_a_long_text() {
    let pattern = "(a|b)*c?".repeat(200);
    check!("1 201 states, 2 000 characters", m(&pattern, &"ab".repeat(1000)), true);
    check!("and a failure at the very end", m(&pattern, &("ab".repeat(1000) + "x")), false);
}
