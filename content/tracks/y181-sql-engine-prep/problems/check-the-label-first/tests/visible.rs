use solution::Formula as F;
use solution::*;

#[allow(dead_code)]
fn table() -> Vec<(String, Kind)> {
    vec![("items".to_string(), Kind::Number), ("city".to_string(), Kind::Words), ("fragile".to_string(), Kind::YesNo)]
}

#[allow(dead_code)]
fn field(name: &str) -> Box<F> {
    Box::new(F::Field(name.to_string()))
}

#[test]
fn fields_become_positions() {
    let got = check(&F::Add(field("items"), Box::new(F::Number(1))), &table());
    let want = Checked::Add(Box::new(Checked::Field(0)), Box::new(Checked::Number(1)));
    check!("items + 1", got, Ok((want, Some(Kind::Number))));
}

#[test]
fn a_wrong_kind() {
    let got = check(&F::Add(field("items"), field("city")), &table());
    check!("items + city", got.map(|r| r.1), Err(Fault::WrongKind { needed: Kind::Number, found: Kind::Words }));
}

#[test]
fn an_unknown_field() {
    let got = check(&F::Join(vec![F::Field("town".to_string())]), &table());
    check!("join(town)", got.map(|r| r.1), Err(Fault::NoSuchField("town".to_string())));
}

#[test]
fn blank_fits_anywhere() {
    let sum = F::Add(Box::new(F::Blank), field("items"));
    check!("blank + items", check(&sum, &table()).map(|r| r.1), Ok(Some(Kind::Number)));
    let pick = F::Pick(field("fragile"), Box::new(F::Blank), field("city"));
    check!("pick(fragile, blank, city)", check(&pick, &table()).map(|r| r.1), Ok(Some(Kind::Words)));
    check!("blank alone", check(&F::Blank, &[]), Ok((Checked::Blank, None)));
}

#[test]
fn the_first_mistake_wins() {
    let sum = F::Add(field("city"), field("nowhere"));
    check!("city is judged before the second operand is looked at", check(&sum, &table()).map(|r| r.1), Err(Fault::WrongKind { needed: Kind::Number, found: Kind::Words }));
}
