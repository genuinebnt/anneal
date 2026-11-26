use solution::*;
use Formula::{And, Not, Or, Var};

#[allow(dead_code)]
fn v(c: char) -> Formula {
    Var(c)
}
#[allow(dead_code)]
fn not(f: Formula) -> Formula {
    Not(Box::new(f))
}

/// The reader of the grammar in the statement.
#[allow(dead_code)]
fn read(text: &str) -> Option<Formula> {
    fn list(s: &[u8], at: &mut usize, level: usize) -> Option<Formula> {
        if level == 2 {
            return unary(s, at);
        }
        let mut parts = vec![list(s, at, level + 1)?];
        while s.get(*at) == Some(&[b'|', b'&'][level]) {
            *at += 1;
            parts.push(list(s, at, level + 1)?);
        }
        Some(match (parts.len(), level) {
            (1, _) => parts.pop()?,
            (_, 0) => Or(parts),
            _ => And(parts),
        })
    }
    fn unary(s: &[u8], at: &mut usize) -> Option<Formula> {
        let c = *s.get(*at)?;
        *at += 1;
        match c {
            b'!' => Some(not(unary(s, at)?)),
            b'a'..=b'c' => Some(v(c as char)),
            b'(' => {
                let inner = list(s, at, 0)?;
                (s.get(*at) == Some(&b')')).then(|| *at += 1)?;
                Some(inner)
            }
            _ => None,
        }
    }
    let mut at = 0;
    let f = list(text.as_bytes(), &mut at, 0)?;
    (at == text.len()).then_some(f)
}

#[allow(dead_code)]
fn height(f: &Formula) -> usize {
    match f {
        Var(_) => 1,
        Not(inner) => 1 + height(inner),
        And(parts) | Or(parts) => 1 + parts.iter().map(height).max().unwrap_or(0),
    }
}

#[allow(dead_code)]
fn well_formed(f: &Formula) -> bool {
    match f {
        Var(c) => ('a'..='c').contains(c),
        Not(inner) => well_formed(inner),
        And(parts) | Or(parts) => (2..=4).contains(&parts.len()) && parts.iter().all(well_formed),
    }
}

use anneal_prelude::Rng;
use std::collections::HashSet;

#[allow(dead_code)]
fn random(rng: &mut Rng, height: usize) -> Formula {
    match rng.below(if height <= 1 { 1 } else { 4 }) {
        0 => v(*rng.pick(&['a', 'b', 'c'])),
        1 => not(random(rng, height - 1)),
        k => {
            let n = rng.int(2, 3);
            let parts = (0..n).map(|_| random(rng, height - 1)).collect();
            if k == 2 {
                And(parts)
            } else {
                Or(parts)
            }
        }
    }
}

/// The text with one matched pair of brackets removed, for each pair.
#[allow(dead_code)]
fn with_a_pair_removed(text: &str) -> Vec<String> {
    let mut open = Vec::new();
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        if c == '(' {
            open.push(i);
        } else if c == ')' {
            if let Some(o) = open.pop() {
                out.push(format!("{}{}{}", &text[..o], &text[o + 1..i], &text[i + 1..]));
            }
        }
    }
    out
}

#[test]
fn random_trees_read_back_and_no_bracket_can_go() {
    let mut rng = Rng::new(1508);
    for _ in 0..2000 {
        let f = random(&mut rng, 5);
        let text = to_text(&f);
        check!(format!("{f:?} printed as {text:?} reads back"), read(&text), Some(f.clone()));
        for fewer in with_a_pair_removed(&text) {
            check!(format!("{text:?} could be {fewer:?}"), read(&fewer) == Some(f.clone()), false);
        }
    }
}

#[test]
fn every_sample_is_well_formed_and_not_too_tall() {
    for max in 1..=6 {
        for seed in 0..300 {
            let f = sample(seed, max);
            check!(format!("sample({seed}, {max}) = {f:?}"), (well_formed(&f), height(&f) <= max), (true, true));
        }
    }
    check!("the largest seed", well_formed(&sample(u64::MAX, 4)), true);
}

#[allow(dead_code)]
fn kind(f: &Formula) -> &'static str {
    match f {
        Var(_) => "letter",
        Not(_) => "not",
        And(_) => "and",
        Or(_) => "or",
    }
}

#[allow(dead_code)]
fn pairs(f: &Formula, seen: &mut HashSet<(&'static str, &'static str)>) {
    let children: Vec<&Formula> = match f {
        Var(_) => vec![],
        Not(inner) => vec![inner],
        And(parts) | Or(parts) => parts.iter().collect(),
    };
    for child in children {
        seen.insert((kind(f), kind(child)));
        pairs(child, seen);
    }
}

#[test]
fn many_seeds_explore_the_shapes() {
    let all: Vec<Formula> = (0..2000).map(|s| sample(s, 4)).collect();
    let distinct: HashSet<&Formula> = all.iter().collect();
    check!(format!("{} different formulas", distinct.len()), distinct.len() >= 500, true);
    let tops: HashSet<&str> = all.iter().map(kind).collect();
    check!(format!("forms at the top: {tops:?}"), tops.len(), 4);
    let mut seen = HashSet::new();
    all.iter().for_each(|f| pairs(f, &mut seen));
    for outer in ["not", "and", "or"] {
        for inner in ["letter", "not", "and", "or"] {
            check!(format!("{inner} directly inside {outer}"), seen.contains(&(outer, inner)), true);
        }
    }
}

#[test]
fn all_three_letters_and_all_list_lengths_appear() {
    fn walk(f: &Formula, letters: &mut HashSet<char>, lengths: &mut HashSet<usize>) {
        match f {
            Var(c) => {
                letters.insert(*c);
            }
            Not(inner) => walk(inner, letters, lengths),
            And(parts) | Or(parts) => {
                lengths.insert(parts.len());
                parts.iter().for_each(|p| walk(p, letters, lengths));
            }
        }
    }
    let (mut letters, mut lengths) = (HashSet::new(), HashSet::new());
    (0..2000).for_each(|s| walk(&sample(s, 4), &mut letters, &mut lengths));
    check!("letters and lengths", (letters.len(), lengths.len()), (3, 3));
}

#[test]
fn nots_around_each_form() {
    check!("!(a|b)&!c", to_text(&And(vec![not(Or(vec![v('a'), v('b')])), not(v('c'))])), "!(a|b)&!c".to_string());
}

#[test]
fn a_not_of_a_letter_needs_nothing() {
    check!("!a|!b", to_text(&Or(vec![not(v('a')), not(v('b'))])), "!a|!b".to_string());
}

#[test]
fn deep_towers_of_not() {
    let mut f = v('a');
    for _ in 0..200 {
        f = not(f);
    }
    check!("200 nots", to_text(&f), "!".repeat(200) + "a");
}

#[test]
fn the_same_arguments_give_the_same_formula_at_every_height() {
    for max in 1..=7 {
        let first: Vec<Formula> = (0..60).map(|s| sample(s, max)).collect();
        let again: Vec<Formula> = (0..60).map(|s| sample(s, max)).collect();
        check!(format!("height limit {max}"), first == again, true);
    }
}
