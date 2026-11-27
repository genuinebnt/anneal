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
fn one_test() {
    use build::*;
    check!("test 7, yes", go(&t(7), &[true]), Ok((true, vec![7])));
    check!("not test 7", go(&not(t(7)), &[true]), Ok((false, vec![7])));
}

#[test]
fn all_stops_at_the_first_false() {
    use build::*;
    let rule = all(vec![t(1), t(2), t(3)]);
    check!(
        "yes, no, ...",
        go(&rule, &[true, false, true]),
        Ok((false, vec![1, 2]))
    );
    check!("all yes", go(&rule, &[true]), Ok((true, vec![1, 2, 3])));
}

#[test]
fn any_stops_at_the_first_true() {
    use build::*;
    let rule = any(vec![t(1), t(2), t(3)]);
    check!(
        "no, yes, ...",
        go(&rule, &[false, true, false]),
        Ok((true, vec![1, 2]))
    );
    check!("all no", go(&rule, &[false]), Ok((false, vec![1, 2, 3])));
}

#[test]
fn choose_asks_one_side() {
    use build::*;
    let rule = choose(t(1), t(2), t(3));
    check!("yes", go(&rule, &[true, false]), Ok((false, vec![1, 2])));
    check!("no", go(&rule, &[false, true]), Ok((true, vec![1, 3])));
}

#[test]
fn no_parts() {
    use build::*;
    check!("all of nothing", go(&all(vec![]), &[]), Ok((true, vec![])));
    check!("any of nothing", go(&any(vec![]), &[]), Ok((false, vec![])));
}
