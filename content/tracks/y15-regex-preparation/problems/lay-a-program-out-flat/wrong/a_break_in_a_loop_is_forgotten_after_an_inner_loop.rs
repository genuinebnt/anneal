#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Emit(char),
    Seq(Vec<Stmt>),
    If(u8, Box<Stmt>, Box<Stmt>),
    While(u8, Box<Stmt>),
    Break,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Emit(char, usize),
    Branch { flag: u8, yes: usize, no: usize },
    Halt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub ops: Vec<Op>,
    pub start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileError {
    JumpOutsideLoop,
}

/// Where `continue` and `break` go inside the nearest loop.
#[derive(Clone, Copy)]
struct Loop {
    again: usize,
    leave: usize,
}

fn push(ops: &mut Vec<Op>, op: Op) -> usize {
    ops.push(op);
    ops.len() - 1
}

/// Builds `statement` so that it carries on at `next`, and returns where it begins.
fn emit(
    ops: &mut Vec<Op>,
    statement: &Stmt,
    next: usize,
    inside: Option<Loop>,
) -> Result<usize, CompileError> {
    Ok(match statement {
        Stmt::Emit(c) => push(ops, Op::Emit(*c, next)),
        Stmt::Seq(list) => {
            let mut entry = next;
            for s in list.iter().rev() {
                entry = emit(ops, s, entry, inside)?;
            }
            entry
        }
        Stmt::If(flag, then, otherwise) => {
            let yes = emit(ops, then, next, inside)?;
            let no = emit(ops, otherwise, next, inside)?;
            push(
                ops,
                Op::Branch {
                    flag: *flag,
                    yes,
                    no,
                },
            )
        }
        Stmt::While(flag, body) => {
            let test = push(ops, Op::Halt); // filled in below
            let here = Loop {
                again: test,
                leave: next,
            };
            let yes = emit(ops, body, test, Some(here))?;
            ops[test] = Op::Branch {
                flag: *flag,
                yes,
                no: next,
            };
            test
        }
        Stmt::Break => inside.ok_or(CompileError::JumpOutsideLoop)?.leave,
        Stmt::Continue => inside.ok_or(CompileError::JumpOutsideLoop)?.again,
    })
}

pub fn compile(statement: &Stmt) -> Result<Program, CompileError> {
    let mut ops = vec![Op::Halt];
    let outside = Loop { again: 0, leave: 0 };
    let start = emit(&mut ops, statement, 0, Some(outside))?;
    Ok(Program { ops, start })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfSteps;

/// Runs the program, asking `flag` at each branch, for at most `max_steps` operations.
pub fn execute(
    program: &Program,
    flag: &mut dyn FnMut(u8) -> bool,
    max_steps: u64,
) -> Result<String, OutOfSteps> {
    let mut out = String::new();
    let mut at = program.start;
    let mut left = max_steps;
    loop {
        let op = program.ops[at];
        if op == Op::Halt {
            return Ok(out);
        }
        left = left.checked_sub(1).ok_or(OutOfSteps)?;
        at = match op {
            Op::Emit(c, next) => {
                out.push(c);
                next
            }
            Op::Branch { flag: f, yes, no } => {
                if flag(f) {
                    yes
                } else {
                    no
                }
            }
            Op::Halt => return Ok(out),
        };
    }
}
