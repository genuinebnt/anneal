use solution::*;
use std::collections::BTreeMap;

#[allow(dead_code)]
fn unit(list: &[(&str, i32)]) -> Result<BTreeMap<String, i32>, UnitError> {
    Ok(list.iter().map(|&(n, p)| (n.to_string(), p)).collect())
}

#[test]
fn more_unexpected_tokens() {
    for (text, want) in [
        (". m", UnitError::Unexpected(0)),
        ("/ m", UnitError::Unexpected(0)),
        ("m . . s", UnitError::Unexpected(4)),
        ("m .", UnitError::UnexpectedEnd),
        ("m^2^3", UnitError::Unexpected(3)),
        ("m^", UnitError::UnexpectedEnd),
        ("(m", UnitError::UnexpectedEnd),
        ("(m ^ )", UnitError::Unexpected(5)),
        ("()", UnitError::Unexpected(1)),
        ("^2", UnitError::Unexpected(0)),
        ("m 1 2", UnitError::Unexpected(4)),
        ("10 m", UnitError::Unexpected(0)),
    ] {
        check!(text, parse(text), Err(want));
    }
}

#[test]
fn splitting_comes_first() {
    check!(") é", parse(") é"), Err(UnitError::BadChar(2)));
    check!("m - s", parse("m - s"), Err(UnitError::BadChar(2)));
    check!("m^-", parse("m^-"), Err(UnitError::BadChar(2)));
    check!("a written number too large, then a bad character", parse("m^99999999999 ?"), Err(UnitError::TooBig));
    check!("the other order", parse("? m^99999999999"), Err(UnitError::BadChar(0)));
}

#[test]
fn the_ends_of_the_range() {
    check!("i32::MIN", parse("m^-2147483648"), unit(&[("m", i32::MIN)]));
    check!("i32::MAX", parse("m^2147483647"), unit(&[("m", i32::MAX)]));
    check!("one too many by writing", parse("m^2147483648"), Err(UnitError::TooBig));
    check!("by multiplying", parse("m^2147483647 m"), Err(UnitError::TooBig));
    check!("by raising", parse("(m^65536)^65536"), Err(UnitError::TooBig));
    check!("by dividing", parse("1 / m^-2147483648"), Err(UnitError::TooBig));
    check!("overflow comes before a later mistake", parse("(m^65536)^65536 )"), Err(UnitError::TooBig));
    check!("a mistake before the overflow", parse(") (m^65536)^65536"), Err(UnitError::Unexpected(0)));
}

#[test]
fn brackets_have_a_limit() {
    let nested = |n: usize| format!("{}m{}", "(".repeat(n), ")".repeat(n));
    check!("40 open", parse(&nested(MAX_OPEN)), unit(&[("m", 1)]));
    check!("41 open", parse(&nested(MAX_OPEN + 1)), Err(UnitError::TooDeep(40)));
    check!("200000 open", parse(&nested(200_000)), Err(UnitError::TooDeep(40)));
    check!("200000 never closed", parse(&"(".repeat(200_000)), Err(UnitError::TooDeep(40)));
    check!("many groups side by side", parse(&"(m)".repeat(5_000)), unit(&[("m", 5_000)]));
}

#[test]
fn long_flat_chains() {
    check!("200000 factors", parse(&"m ".repeat(200_000)), unit(&[("m", 200_000)]));
    check!("100000 divisions", parse(&format!("m{}", "/m".repeat(100_000))), unit(&[("m", -99_999)]));
}

#[derive(Debug)]
enum Tree {
    Name(&'static str),
    Mul(Box<Tree>, Box<Tree>),
    Div(Box<Tree>, Box<Tree>),
    Pow(Box<Tree>, i32),
}

fn random(rng: &mut anneal_prelude::Rng, depth: usize) -> Tree {
    let pick = if depth == 0 { 0 } else { rng.below(5) };
    match pick {
        0 | 1 => Tree::Name(*rng.pick(&["a", "b", "c"])),
        2 => Tree::Mul(Box::new(random(rng, depth - 1)), Box::new(random(rng, depth - 1))),
        3 => Tree::Div(Box::new(random(rng, depth - 1)), Box::new(random(rng, depth - 1))),
        _ => Tree::Pow(Box::new(random(rng, depth - 1)), rng.int(-3, 3) as i32),
    }
}

/// Writes the tree with only the brackets the reader needs. 0: ratio, 1: product, 2: factor.
fn print(rng: &mut anneal_prelude::Rng, t: &Tree, needs: usize) -> String {
    let (text, strength) = match t {
        Tree::Name(n) => (n.to_string(), 3),
        Tree::Div(a, b) => (format!("{} / {}", print(rng, a, 0), print(rng, b, 1)), 0),
        Tree::Mul(a, b) => {
            let join = *rng.pick(&[" ", ".", " . "]);
            (format!("{}{join}{}", print(rng, a, 1), print(rng, b, 1)), 1)
        }
        Tree::Pow(a, n) => (format!("{}^{n}", print(rng, a, 3)), 2),
    };
    if strength < needs {
        format!("({text})")
    } else {
        text
    }
}

fn value(t: &Tree) -> BTreeMap<String, i32> {
    let add = |a: &Tree, b: &Tree, sign: i32| {
        let mut left = value(a);
        for (n, p) in value(b) {
            *left.entry(n).or_insert(0) += sign * p;
        }
        left
    };
    let mut out = match t {
        Tree::Name(n) => BTreeMap::from([(n.to_string(), 1)]),
        Tree::Mul(a, b) => add(a, b, 1),
        Tree::Div(a, b) => add(a, b, -1),
        Tree::Pow(a, n) => value(a).into_iter().map(|(k, p)| (k, p * n)).collect(),
    };
    out.retain(|_, p| *p != 0);
    out
}

#[test]
fn matches_trees_written_out_as_text() {
    let mut rng = anneal_prelude::Rng::new(5107);
    for _ in 0..600 {
        let depth = rng.below(5);
        let tree = random(&mut rng, depth);
        let text = print(&mut rng, &tree, 0);
        check!(text.clone(), parse(&text), Ok(value(&tree)));
    }
}

#[test]
fn spaces_are_optional() {
    check!("J/(kg.K)^2m", parse("J/(kg.K)^2m"), unit(&[("J", 1), ("K", -2), ("kg", -2), ("m", -1)]));
    check!("   m   ", parse("   m   "), unit(&[("m", 1)]));
}

#[test]
fn names_are_whole_runs_of_letters() {
    check!("mm m Mm", parse("mm m Mm"), unit(&[("Mm", 1), ("m", 1), ("mm", 1)]));
    check!("m1m", parse("m1m"), unit(&[("m", 2)]));
}
