/// A rule over yes-or-no tests. Each test is a slow question to a device.
#[derive(Debug, Clone)]
pub enum Rule {
    Test(u32),
    Not(Box<Rule>),
    /// True if every part is true. With no parts, true.
    All(Vec<Rule>),
    /// True if some part is true. With no parts, false.
    Any(Vec<Rule>),
    /// The second rule if the first is true, else the third.
    Choose(Box<Rule>, Box<Rule>, Box<Rule>),
}

/// One instruction of a machine with a stack of booleans. There is no way backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    /// Asks the device and pushes the answer.
    Ask(u32),
    Push(bool),
    /// Replaces the top by its opposite.
    Not,
    Pop,
    /// Skips the next `n` instructions.
    Skip(usize),
    /// Skips the next `n` instructions if the top is false. The top stays.
    SkipIfFalse(usize),
}

/// Appends a skip whose length is not known yet, and returns where it is.
fn hole(code: &mut Vec<Instr>, instr: Instr) -> usize {
    code.push(instr);
    code.len() - 1
}

/// Makes the skip at `at` land just after the last instruction so far.
fn land(code: &mut [Instr], at: usize) {
    let length = code.len() - at - 1;
    match &mut code[at] {
        Instr::Skip(n) | Instr::SkipIfFalse(n) => *n = length,
        _ => unreachable!("only skips are patched"),
    }
}

fn emit(rule: &Rule, code: &mut Vec<Instr>) {
    match rule {
        Rule::Test(n) => code.push(Instr::Ask(*n)),
        Rule::Not(inner) => {
            emit(inner, code);
            code.push(Instr::Not);
        }
        Rule::All(parts) => {
            let Some((last, rest)) = parts.split_last() else {
                code.push(Instr::Push(true));
                return;
            };
            let mut exits = Vec::new();
            for part in rest {
                emit(part, code);
                // A false answer is the answer of the whole rule.
                exits.push(hole(code, Instr::SkipIfFalse(0)));
                code.push(Instr::Pop);
            }
            emit(last, code);
            exits.into_iter().for_each(|at| land(code, at));
        }
        Rule::Any(parts) => {
            let Some((last, rest)) = parts.split_last() else {
                code.push(Instr::Push(false));
                return;
            };
            let mut exits = Vec::new();
            for part in rest {
                emit(part, code);
                let to_next = hole(code, Instr::SkipIfFalse(0));
                land(code, to_next);
                code.push(Instr::Pop);
            }
            emit(last, code);
            exits.into_iter().for_each(|at| land(code, at));
        }
        Rule::Choose(test, yes, no) => {
            emit(test, code);
            let to_no = hole(code, Instr::SkipIfFalse(0));
            code.push(Instr::Pop);
            emit(yes, code);
            let to_end = hole(code, Instr::Skip(0));
            land(code, to_no);
            code.push(Instr::Pop);
            emit(no, code);
            land(code, to_end);
        }
    }
}

/// Turns a rule into instructions that leave its value as the only thing on the stack.
pub fn compile(rule: &Rule) -> Vec<Instr> {
    let mut code = Vec::new();
    emit(rule, &mut code);
    code
}
