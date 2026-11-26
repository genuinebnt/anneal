use solution::*;

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
fn letters_in_a_row() {
    check!("abc", whole("abc", &["abc", "ab", "abcd", "acb"]), vec![true, false, false, false]);
    check!("abc has 3 states and Match", built("abc").states.len(), 4);
}

#[test]
fn the_empty_pattern_is_just_match() {
    let nfa = built("");
    check!("states", nfa.states.clone(), vec![State::Match]);
    check!("start", nfa.start, MATCH);
}

#[test]
fn branches() {
    check!("ab|c|", whole("ab|c|", &["ab", "c", "", "a", "abc"]), vec![true, true, true, false, false]);
    check!("one state per letter and two splits", built("ab|c|").states.len(), 6);
}

#[test]
fn repetitions() {
    check!("ab*", whole("ab*", &["a", "abbb", "", "abab"]), vec![true, true, false, false]);
    check!("ab+", whole("ab+", &["a", "ab", "abbb"]), vec![false, true, true]);
    check!("ab?", whole("ab?", &["a", "ab", "abb"]), vec![true, true, false]);
    check!("(ab)+ has a, b, one split and Match", built("(ab)+").states.len(), 4);
}

#[test]
fn anchors_become_states() {
    check!("^a$", whole("^a$", &["a"]), vec![true]);
    check!("a^", whole("a^", &["a"]), vec![false]);
    check!("$a", whole("$a", &["a"]), vec![false]);
}

#[test]
fn links_stay_inside() {
    check!("a(b|c*)+d?", well_formed(&built("a(b|c*)+d?")), true);
}
