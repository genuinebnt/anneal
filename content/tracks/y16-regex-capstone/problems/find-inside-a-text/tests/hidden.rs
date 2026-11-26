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
fn find(pattern: &str, text: &str) -> Option<(usize, usize)> {
    Regex::new(pattern).expect("a valid pattern").find(text)
}

#[test]
fn a_match_can_stop_accepting_and_accept_again() {
    check!("ab(cd)? and longer", find("a(bcd)?", "abcx"), Some((0, 1)));
    check!("abcd", find("a(bcd)?", "abcd"), Some((0, 4)));
    check!("(abc)*", find("(abc)+", "abcabcab"), Some((0, 6)));
}

#[test]
fn a_longer_match_that_fails_late_falls_back() {
    check!("ab|abcdef on abcdex", find("ab|abcdef", "abcdex"), Some((0, 2)));
    check!("an earlier start that finishes late wins", find("abcde|c", "abcde"), Some((0, 5)));
    check!("an earlier start that never finishes does not", find("abcdx|c", "abcde"), Some((2, 3)));
}

#[test]
fn random_searches_agree_with_the_tree() {
    let mut rng = Rng::new(1605);
    let (mut inside, mut none) = (0, 0);
    for _ in 0..3000 {
        let pattern = random_pattern(&mut rng, 3);
        let tree = parse(&pattern).unwrap();
        let re = Regex::new(&pattern).unwrap();
        for _ in 0..8 {
            let text = random_text(&mut rng);
            let want = slow_find(&tree, &text);
            inside += u32::from(want.is_some_and(|(s, _)| s > 0));
            none += u32::from(want.is_none());
            check!(format!("find {pattern:?} in {text:?}"), re.find(&text), want);
            check!(format!("is_match {pattern:?} on {text:?}"), re.is_match(&text), slow_is_match(&tree, &text));
        }
    }
    check!(format!("the comparison is not trivial: {inside} later starts, {none} misses"), inside > 300 && none > 1000, true);
}

#[test]
fn newlines_are_ordinary_characters() {
    check!("$ is only the very end", find("a$", "a\nb a\n"), None);
    check!("^ is only the very start", find("^b", "a\nb"), None);
    check!("dot reads a newline", find("a.b", "xa\nb"), Some((1, 4)));
}

#[test]
fn anchors_decided_by_the_real_text() {
    check!("a start anchor in the second branch", find("zz|^a", "ba zz"), Some((3, 5)));
    check!("an empty match only at the end", find("$|zz", "abc"), Some((3, 3)));
    check!("both anchors need an empty text", (find("^$", "a"), find("^$", "")), (None, Some((0, 0))));
}

#[test]
fn one_regex_answers_many_times() {
    let re = Regex::new("b+c").unwrap();
    let got: Vec<_> = ["abbc", "bc", "cb", "abbc"].iter().map(|t| re.find(t)).collect();
    check!("four searches", got, vec![Some((1, 4)), Some((0, 2)), None, Some((1, 4))]);
}

#[test]
fn attempts_that_die_at_once_cost_little() {
    let text = "b".repeat(200_000);
    check!("a+c in 200 000 b", find("a+c", &text), None);
    check!("^c in 200 000 b", find("^c", &text), None);
}

#[test]
fn the_pattern_that_blows_up_other_engines() {
    let a = "a".repeat(1000);
    check!("(a*)*b in a^1000", find("(a*)*b", &a), None);
    check!("(a*)*b in a^1000 b", find("(a*)*b", &(a + "b")), Some((0, 1001)));
}
