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

#[test]
fn no_brackets_when_the_tighter_form_is_inside() {
    let f = Or(vec![And(vec![v('a'), not(v('b'))]), v('c')]);
    check!("a&!b|c", to_text(&f), "a&!b|c".to_string());
}

#[test]
fn brackets_when_the_looser_form_is_inside() {
    let f = And(vec![v('a'), Or(vec![v('b'), v('c')])]);
    check!("a&(b|c)", to_text(&f), "a&(b|c)".to_string());
    check!("!(a&b)", to_text(&not(And(vec![v('a'), v('b')]))), "!(a&b)".to_string());
}

#[test]
fn a_list_inside_its_own_kind_keeps_its_brackets() {
    let f = And(vec![v('a'), And(vec![v('b'), v('c')])]);
    check!("a&(b&c)", to_text(&f), "a&(b&c)".to_string());
    let f = Or(vec![Or(vec![v('a'), v('b')]), v('c')]);
    check!("(a|b)|c", to_text(&f), "(a|b)|c".to_string());
}

#[test]
fn single_forms() {
    check!("a letter", to_text(&v('b')), "b".to_string());
    check!("two nots", to_text(&not(not(v('a')))), "!!a".to_string());
}

#[test]
fn samples_respect_the_height_and_repeat() {
    check!("height 1 is a letter", (0..50).all(|s| matches!(sample(s, 1), Var('a'..='c'))), true);
    check!("same seed, same formula", (0..50).all(|s| sample(s, 5) == sample(s, 5)), true);
}
