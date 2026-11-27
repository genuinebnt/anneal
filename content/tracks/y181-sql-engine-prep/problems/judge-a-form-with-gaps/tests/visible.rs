use solution::*;

#[allow(dead_code)]
fn not(rule: Rule) -> Rule {
    Rule::Not(Box::new(rule))
}

#[test]
fn a_given_answer() {
    check!("18 at least 18", judge(&Rule::AtLeast(0, 18), &[Some(18)]), Verdict::Yes);
    check!("17 at least 18", judge(&Rule::AtLeast(0, 18), &[Some(17)]), Verdict::No);
}

#[test]
fn a_missing_answer_is_not_a_no() {
    check!("blank", judge(&Rule::AtLeast(0, 18), &[None]), Verdict::Unknown);
    check!("not on the form", judge(&Rule::AtLeast(7, 0), &[Some(1)]), Verdict::Unknown);
    check!("its opposite", judge(&not(Rule::AtLeast(0, 18)), &[None]), Verdict::Unknown);
}

#[test]
fn one_side_can_settle_it() {
    let answers = [None, Some(3)];
    let low = Rule::AtLeast(1, 10);
    let gap = Rule::AtLeast(0, 10);
    check!("all: one no", judge(&Rule::All(vec![gap.clone(), low.clone()]), &answers), Verdict::No);
    check!("any: one yes", judge(&Rule::Any(vec![gap.clone(), not(low.clone())]), &answers), Verdict::Yes);
    check!("any: no yes yet", judge(&Rule::Any(vec![gap, low]), &answers), Verdict::Unknown);
}

#[test]
fn asking_whether_it_is_missing() {
    check!("blank", judge(&Rule::Missing(0), &[None]), Verdict::Yes);
    check!("given", judge(&Rule::Missing(0), &[Some(0)]), Verdict::No);
}

#[test]
fn two_lists() {
    let forms = vec![vec![Some(30)], vec![None], vec![Some(3)], vec![Some(18)]];
    check!("accept, send back", sort_forms(&Rule::AtLeast(0, 18), &forms), (vec![0, 3], vec![1]));
}
