use solution::*;

#[allow(dead_code)]
mod build {
    use solution::*;
    pub fn t(n: u32) -> Rule {
        Rule::Test(n)
    }
    pub fn not(r: Rule) -> Rule {
        Rule::Not(Box::new(r))
    }
    pub fn all(r: Vec<Rule>) -> Rule {
        Rule::All(r)
    }
    pub fn any(r: Vec<Rule>) -> Rule {
        Rule::Any(r)
    }
    pub fn choose(c: Rule, a: Rule, b: Rule) -> Rule {
        Rule::Choose(Box::new(c), Box::new(a), Box::new(b))
    }
}

/// The device. `Err` says what the program did wrong.
#[allow(dead_code)]
fn device(
    code: &[Instr],
    answers: &mut dyn FnMut() -> bool,
) -> Result<(bool, Vec<u32>), &'static str> {
    let (mut stack, mut asked, mut at) = (Vec::new(), Vec::new(), 0usize);
    while at < code.len() {
        let instr = code[at];
        at += 1;
        let skip = match instr {
            Instr::Ask(n) => {
                asked.push(n);
                stack.push(answers());
                0
            }
            Instr::Push(b) => {
                stack.push(b);
                0
            }
            Instr::Not => {
                let top = stack.last_mut().ok_or("Not on an empty stack")?;
                *top = !*top;
                0
            }
            Instr::Pop => {
                stack.pop().ok_or("Pop on an empty stack")?;
                0
            }
            Instr::Skip(n) => n,
            Instr::SkipIfFalse(n) => {
                if *stack.last().ok_or("SkipIfFalse on an empty stack")? {
                    0
                } else {
                    n
                }
            }
        };
        at = at.checked_add(skip).ok_or("a skip goes past the end")?;
        if at > code.len() {
            return Err("a skip goes past the end");
        }
    }
    match stack.as_slice() {
        [value] => Ok((*value, asked)),
        _ => Err("the stack does not hold exactly one value at the end"),
    }
}

/// Runs the compiled rule, answering the questions with `answers`, over and over.
#[allow(dead_code)]
fn go(rule: &Rule, answers: &[bool]) -> Result<(bool, Vec<u32>), &'static str> {
    let mut next = 0;
    device(&compile(rule), &mut || {
        next += 1;
        answers
            .get((next - 1) % answers.len().max(1))
            .copied()
            .unwrap_or(false)
    })
}

#[test]
fn nested_rules_keep_what_is_below() {
    use build::*;
    let rule = all(vec![
        any(vec![t(1), t(2)]),
        choose(any(vec![]), t(3), not(all(vec![t(4), t(5)]))),
    ]);
    check!(
        "no yes | yes yes",
        go(&rule, &[false, true, true, true]),
        Ok((false, vec![1, 2, 4, 5]))
    );
    check!("no no", go(&rule, &[false, false]), Ok((false, vec![1, 2])));
}

#[test]
fn the_same_test_is_asked_again() {
    use build::*;
    let rule = all(vec![t(1), not(t(1))]);
    check!(
        "yes then no",
        go(&rule, &[true, false]),
        Ok((true, vec![1, 1]))
    );
}

#[test]
fn single_parts() {
    use build::*;
    check!(
        "all of one",
        go(&all(vec![t(1)]), &[false]),
        Ok((false, vec![1]))
    );
    check!(
        "any of one",
        go(&any(vec![t(1)]), &[true]),
        Ok((true, vec![1]))
    );
}

#[allow(dead_code)]
fn size(rule: &Rule) -> usize {
    match rule {
        Rule::Test(_) => 1,
        Rule::Not(r) => 1 + size(r),
        Rule::All(parts) | Rule::Any(parts) => {
            1 + parts.len() + parts.iter().map(size).sum::<usize>()
        }
        Rule::Choose(c, a, b) => 1 + size(c) + size(a) + size(b),
    }
}

#[test]
fn a_hundred_thousand_parts() {
    use build::*;
    let parts: Vec<Rule> = (0..100_000).map(t).collect();
    for (name, rule) in [("all", all(parts.clone())), ("any", any(parts))] {
        let code = compile(&rule);
        check!(name, code.len() <= 4 * size(&rule), true);
        let yes = name == "all";
        check!(
            name,
            device(&code, &mut || yes).map(|(v, asked)| (v, asked.len())),
            Ok((yes, 100_000))
        );
        check!(
            name,
            device(&code, &mut || !yes).map(|(v, asked)| (v, asked.len())),
            Ok((!yes, 1))
        );
    }
}

#[test]
fn choices_inside_choices_do_not_grow_the_program_out_of_proportion() {
    use build::*;
    let mut rule = t(0);
    for i in 1..60 {
        rule = choose(rule, t(i), not(t(i)));
    }
    check!(
        "choose in the test, 59 deep",
        compile(&rule).len() <= 4 * size(&rule),
        true
    );
    let mut rule = t(0);
    for i in 1..16 {
        rule = any(vec![all(vec![rule, t(i)]), t(i + 100)]);
    }
    check!(
        "any of all, 15 deep",
        compile(&rule).len() <= 4 * size(&rule),
        true
    );
}

// Random rules, with random answers, against a direct evaluation.

#[allow(dead_code)]
fn random(rng: &mut anneal_prelude::Rng, depth: usize) -> Rule {
    if depth == 0 || rng.below(4) == 0 {
        return Rule::Test(rng.below(6) as u32);
    }
    let d = depth - 1;
    match rng.below(4) {
        0 => Rule::Not(Box::new(random(rng, d))),
        1 => Rule::All((0..rng.below(4)).map(|_| random(rng, d)).collect()),
        2 => Rule::Any((0..rng.below(4)).map(|_| random(rng, d)).collect()),
        _ => {
            let c = Box::new(random(rng, d));
            let a = Box::new(random(rng, d));
            Rule::Choose(c, a, Box::new(random(rng, d)))
        }
    }
}

#[allow(dead_code)]
fn direct(rule: &Rule, answers: &[bool], asked: &mut Vec<u32>) -> bool {
    match rule {
        Rule::Test(n) => {
            asked.push(*n);
            answers[(asked.len() - 1) % answers.len()]
        }
        Rule::Not(r) => !direct(r, answers, asked),
        Rule::All(parts) => parts.iter().all(|p| direct(p, answers, asked)),
        Rule::Any(parts) => parts.iter().any(|p| direct(p, answers, asked)),
        Rule::Choose(c, a, b) => {
            let side = if direct(c, answers, asked) { a } else { b };
            direct(side, answers, asked)
        }
    }
}

#[test]
fn random_rules() {
    let mut rng = anneal_prelude::Rng::new(6701);
    for _ in 0..3000 {
        let rule = random(&mut rng, 5);
        let answers: Vec<bool> = (0..1 + rng.below(12)).map(|_| rng.bool()).collect();
        let mut asked = Vec::new();
        let value = direct(&rule, &answers, &mut asked);
        let text = format!("{rule:?} answers {answers:?}");
        check!(text.clone(), go(&rule, &answers), Ok((value, asked)));
        check!(text, compile(&rule).len() <= 4 * size(&rule), true);
    }
}

#[test]
fn not_around_a_rule_that_stops_early() {
    use build::*;
    let rule = not(all(vec![t(1), t(2)]));
    check!("stops at 1", go(&rule, &[false, true]), Ok((true, vec![1])));
    let rule = not(any(vec![t(1), t(2)]));
    check!(
        "stops at 1",
        go(&rule, &[true, false]),
        Ok((false, vec![1]))
    );
}

#[test]
fn what_comes_after_an_early_stop_still_runs() {
    use build::*;
    let rule = choose(
        all(vec![any(vec![t(1), t(2)]), t(3)]),
        any(vec![t(4), t(5)]),
        t(6),
    );
    check!(
        "yes, no",
        go(&rule, &[true, false]),
        Ok((true, vec![1, 3, 6]))
    );
    check!("yes", go(&rule, &[true]), Ok((true, vec![1, 3, 4])));
}
