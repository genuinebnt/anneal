use anneal_prelude::Rng;
use solution::Formula as F;
use solution::*;

const KINDS: [Kind; 3] = [Kind::Number, Kind::Words, Kind::YesNo];

fn table() -> Vec<(String, Kind)> {
    // two fields of each kind
    (0..6).map(|i| (format!("f{i}"), KINDS[i % 3])).collect()
}

fn b<T>(x: T) -> Box<T> {
    Box::new(x)
}

fn kind_of(formula: F) -> Result<Option<Kind>, Fault> {
    check(&formula, &table()).map(|r| r.1)
}

fn wrong(needed: Kind, found: Kind) -> Result<Option<Kind>, Fault> {
    Err(Fault::WrongKind { needed, found })
}

#[test]
fn two_of_one_kind() {
    check!("same(1, \"1\")", kind_of(F::Same(b(F::Number(1)), b(F::Words("1".into())))), wrong(Kind::Number, Kind::Words));
    check!("same(blank, yes)", kind_of(F::Same(b(F::Blank), b(F::YesNo(true)))), Ok(Some(Kind::YesNo)));
    check!("same(blank, blank)", kind_of(F::Same(b(F::Blank), b(F::Blank))), Ok(Some(Kind::YesNo)));
}

#[test]
fn choices() {
    let pick = |t: F, x: F, y: F| kind_of(F::Pick(b(t), b(x), b(y)));
    check!("the test is a number", pick(F::Number(1), F::Blank, F::Blank), wrong(Kind::YesNo, Kind::Number));
    check!("a blank test", pick(F::Blank, F::Number(1), F::Blank), Ok(Some(Kind::Number)));
    check!("two blanks", pick(F::YesNo(true), F::Blank, F::Blank), Ok(None));
    check!("the branches differ", pick(F::YesNo(true), F::Words(String::new()), F::Number(2)), wrong(Kind::Words, Kind::Number));
    let blank_first = F::Pick(b(F::YesNo(true)), b(F::Blank), b(F::Words("a".into())));
    check!("a blank branch takes the other's kind", kind_of(F::Add(b(blank_first), b(F::Number(1)))), wrong(Kind::Number, Kind::Words));
}

#[test]
fn joining() {
    check!("nothing to join", check(&F::Join(vec![]), &[]), Ok((Checked::Join(vec![]), Some(Kind::Words))));
    let parts = vec![F::Words("a".into()), F::Number(3), F::Field("gone".into())];
    check!("the second part is the first mistake", kind_of(F::Join(parts)), wrong(Kind::Words, Kind::Number));
}

#[test]
fn inside_comes_before_outside() {
    let inner = F::Add(b(F::YesNo(false)), b(F::Number(1)));
    // as an operand of `join`, the whole sum would also be wrong, but its inside is checked first
    check!("join(no + 1)", kind_of(F::Join(vec![inner])), wrong(Kind::Number, Kind::YesNo));
    let late = F::Add(b(F::Field("gone".into())), b(F::Words("x".into())));
    check!("gone + \"x\"", kind_of(late), Err(Fault::NoSuchField("gone".into())));
    let early = F::Same(b(F::Add(b(F::Words("x".into())), b(F::Field("gone".into())))), b(F::Blank));
    check!("(\"x\" + gone) is judged left to right", kind_of(early), wrong(Kind::Number, Kind::Words));
}

#[test]
fn field_names_match_exactly() {
    check!("F0 is not f0", kind_of(F::Field("F0".into())), Err(Fault::NoSuchField("F0".into())));
    check!("no fields at all", check(&F::Field("f0".into()), &[]), Err(Fault::NoSuchField("f0".into())));
}

/// A correct formula whose kind is `want`, or fits anything. Returns what `check` must give for it.
fn good(rng: &mut Rng, want: Kind, depth: u32) -> (F, Checked, Option<Kind>) {
    let leaf = depth == 0 || rng.below(3) == 0;
    if leaf {
        return match rng.below(3) {
            0 => (F::Blank, Checked::Blank, None),
            1 => {
                let at = KINDS.iter().position(|&k| k == want).unwrap() + 3 * rng.below(2);
                (F::Field(format!("f{at}")), Checked::Field(at), Some(want))
            }
            _ => match want {
                Kind::Number => (F::Number(7), Checked::Number(7), Some(want)),
                Kind::Words => (F::Words("w".into()), Checked::Words("w".into()), Some(want)),
                Kind::YesNo => (F::YesNo(true), Checked::YesNo(true), Some(want)),
            },
        };
    }
    let d = depth - 1;
    if rng.below(2) == 0 {
        let t = good(rng, Kind::YesNo, d);
        let x = good(rng, want, d);
        let y = good(rng, want, d);
        return (F::Pick(b(t.0), b(x.0), b(y.0)), Checked::Pick(b(t.1), b(x.1), b(y.1)), x.2.or(y.2));
    }
    match want {
        Kind::Number => {
            let (x, y) = (good(rng, want, d), good(rng, want, d));
            (F::Add(b(x.0), b(y.0)), Checked::Add(b(x.1), b(y.1)), Some(want))
        }
        Kind::Words => {
            let parts: Vec<_> = (0..rng.below(4)).map(|_| good(rng, want, d)).collect();
            let (f, c): (Vec<F>, Vec<Checked>) = parts.into_iter().map(|p| (p.0, p.1)).unzip();
            (F::Join(f), Checked::Join(c), Some(want))
        }
        Kind::YesNo => {
            let of = *rng.pick(&KINDS);
            let (x, y) = (good(rng, of, d), good(rng, of, d));
            (F::Same(b(x.0), b(y.0)), Checked::Same(b(x.1), b(y.1)), Some(want))
        }
    }
}

#[test]
fn random_correct_formulas() {
    let mut rng = Rng::new(18111);
    for _ in 0..600 {
        let want = *rng.pick(&KINDS);
        let (formula, checked, kind) = good(&mut rng, want, 4);
        check!(format!("{formula:?}"), check(&formula, &table()), Ok((checked, kind)));
    }
}

#[test]
fn one_mistake_hidden_in_a_correct_formula() {
    let mut rng = Rng::new(18112);
    for _ in 0..600 {
        let (needed, found) = (Kind::Number, *rng.pick(&[Kind::Words, Kind::YesNo]));
        let bad_field = F::Field(format!("f{}", if found == Kind::Words { 1 } else { 2 }));
        let (mut formula, expected) = if rng.bool() {
            (F::Add(b(good(&mut rng, needed, 2).0), b(bad_field)), Fault::WrongKind { needed, found })
        } else {
            (F::Field("lost".into()), Fault::NoSuchField("lost".into()))
        };
        // wrap it: everything to its left is correct, everything to its right is nonsense that must not be reached
        for _ in 0..rng.below(4) {
            let nonsense = b(F::Field("later".into()));
            formula = match rng.below(3) {
                0 => F::Same(b(formula), nonsense),
                1 => F::Pick(b(good(&mut rng, Kind::YesNo, 2).0), b(formula), nonsense),
                _ => F::Join(vec![good(&mut rng, Kind::Words, 2).0, formula, *nonsense]),
            };
        }
        check!(format!("{formula:?}"), check(&formula, &table()).map(|r| r.1), Err(expected));
    }
}

#[test]
fn a_deep_formula() {
    let mut formula = F::Number(1);
    for _ in 0..99 {
        formula = F::Add(b(formula), b(F::Blank));
    }
    check!("100 deep", kind_of(formula), Ok(Some(Kind::Number)));
}
