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

/// How many states the statement asks for, apart from `Match`.
#[allow(dead_code)]
fn size(ast: &Ast) -> usize {
    match ast {
        Ast::Empty => 0,
        Ast::Class(_) | Ast::Any | Ast::Start | Ast::End => 1,
        Ast::Cat(items) => items.iter().map(size).sum(),
        Ast::Alt(branches) => branches.iter().map(size).sum::<usize>() + branches.len() - 1,
        Ast::Star(a) | Ast::Plus(a) | Ast::Opt(a) => 1 + size(a),
    }
}

/// Follows the machine from `at` in every possible way, visiting each (state, position) once, and returns the
/// positions at which it reaches `Match`.
#[allow(dead_code)]
fn accepted(nfa: &Nfa, text: &[char], at: usize) -> std::collections::BTreeSet<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut work = vec![(nfa.start, at)];
    let mut out = std::collections::BTreeSet::new();
    while let Some((id, p)) = work.pop() {
        if !seen.insert((id, p)) {
            continue;
        }
        match &nfa.states[id] {
            State::Match => {
                out.insert(p);
            }
            State::Char(class, next) => {
                if text.get(p).is_some_and(|&c| class.matches(c)) {
                    work.push((*next, p + 1));
                }
            }
            State::Any(next) => {
                if p < text.len() {
                    work.push((*next, p + 1));
                }
            }
            State::Split(a, b) => work.extend([(*a, p), (*b, p)]),
            State::Start(next) => {
                if p == 0 {
                    work.push((*next, p));
                }
            }
            State::End(next) => {
                if p == text.len() {
                    work.push((*next, p));
                }
            }
        }
    }
    out
}

#[allow(dead_code)]
fn built(pattern: &str) -> Nfa {
    compile(&parse(pattern).expect("a valid pattern"))
}

/// Which of the texts the built machine accepts as a whole.
#[allow(dead_code)]
fn whole(pattern: &str, texts: &[&str]) -> Vec<bool> {
    let nfa = built(pattern);
    texts.iter().map(|t| {
        let t: Vec<char> = t.chars().collect();
        accepted(&nfa, &t, 0).contains(&t.len())
    }).collect()
}

#[allow(dead_code)]
fn well_formed(nfa: &Nfa) -> bool {
    let n = nfa.states.len();
    nfa.start < n
        && nfa.states[MATCH] == State::Match
        && nfa.states.iter().filter(|s| **s == State::Match).count() == 1
        && nfa.states.iter().all(|s| match s {
            State::Char(_, a) | State::Any(a) | State::Start(a) | State::End(a) => *a < n,
            State::Split(a, b) => *a < n && *b < n,
            State::Match => true,
        })
}

#[test]
fn loops_around_nothing() {
    check!("()*", whole("()*", &["", "a"]), vec![true, false]);
    check!("(a*)*", whole("(a*)*", &["", "aaa", "b"]), vec![true, true, false]);
    check!("(|a)+b", whole("(|a)+b", &["b", "aab", "a"]), vec![true, true, false]);
    check!("()+", whole("()+", &[""]), vec![true]);
    for pattern in ["()*", "(a*)*", "(|a)+b", "()+", "(^)*", "(()|())*"] {
        let nfa = built(pattern);
        check!(format!("{pattern}: size and links"), (nfa.states.len(), well_formed(&nfa)), (size(&parse(pattern).unwrap()) + 1, true));
    }
}

#[test]
fn a_loop_goes_back_to_its_own_start() {
    check!("(ab)*c", whole("(ab)*c", &["c", "ababc", "abac", "abbc"]), vec![true, true, false, false]);
    check!("(a(bc)*d)*", whole("(a(bc)*d)*", &["adabcd", "abcbcdad", "abc", "bcd"]), vec![true, true, false, false]);
}

#[test]
fn random_patterns_accept_what_their_tree_means() {
    let mut rng = Rng::new(1603);
    for _ in 0..2500 {
        let pattern = random_pattern(&mut rng, 3);
        let tree = parse(&pattern).unwrap();
        let nfa = compile(&tree);
        check!(format!("{pattern:?}: links and the single Match"), well_formed(&nfa), true);
        check!(format!("{pattern:?}: number of states"), nfa.states.len(), size(&tree) + 1);
        for _ in 0..6 {
            let text = random_text(&mut rng);
            let chars: Vec<char> = text.chars().collect();
            let at = rng.below(chars.len() + 1);
            check!(format!("{pattern:?} on {text:?} from {at}"), accepted(&nfa, &chars, at), ends(&tree, &chars, at));
        }
    }
}

#[test]
fn branches_that_are_empty_add_no_states_of_their_own() {
    check!("|||", built("|||").states.len(), 4);
    check!("(|a|)b", whole("(|a|)b", &["b", "ab", "aab"]), vec![true, true, false]);
}

#[test]
fn anchors_under_repetition() {
    check!("(^a|b$)*", whole("(^a|b$)*", &["a", "b", "ab", "ba", "aa"]), vec![true, true, true, false, false]);
}

#[test]
fn size_stays_in_step_with_long_patterns() {
    let pattern = "(a|b)*c?".repeat(5_000);
    check!("40 000 characters of pattern", built(&pattern).states.len(), 6 * 5_000 + 1);
}

#[test]
fn very_wide_patterns_do_not_overflow_the_stack() {
    check!("300 000 letters", built(&"a".repeat(300_000)).states.len(), 300_001);
    check!("100 000 branches", built(&"a|".repeat(100_000)).states.len(), 200_001);
}

#[test]
fn the_deepest_pattern_the_reader_allows() {
    let mut pattern = "a".to_string();
    for _ in 0..MAX_OPEN_GROUPS {
        pattern = format!("(b|c{pattern}+?*)");
    }
    let tree = parse(&pattern).unwrap();
    check!("100 groups deep", compile(&tree).states.len(), size(&tree) + 1);
}
