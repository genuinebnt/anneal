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

pub fn compile(statement: &Stmt) -> Result<Program, CompileError> {
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfSteps;

/// Runs the program, asking `flag` at each branch, for at most `max_steps` operations.
pub fn execute(
    program: &Program,
    flag: &mut dyn FnMut(u8) -> bool,
    max_steps: u64,
) -> Result<String, OutOfSteps> {
    todo!()
}
