#[derive(Debug, Clone)]
pub enum Expr {
    Number(i64),
    Name(String),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// Makes a new variable.
    Let(String, Expr),
    /// Gives an existing variable a new value.
    Assign(String, Expr),
    Print(Expr),
    Block(Vec<Stmt>),
    /// Runs its statements again and again, until a `Leave` gets out.
    Loop(Vec<Stmt>),
    /// If the value is not 0, leaves this many loops, counted from the innermost.
    LeaveIf(Expr, usize),
}

/// One instruction of a machine whose only memory is a stack. Slot 0 is the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Push(i64),
    /// Pushes a copy of the value in a slot.
    Copy(usize),
    /// Pops the top and writes it into a slot.
    Store(usize),
    Add,
    Sub,
    /// Pops the top and records it.
    Print,
    Pop,
    /// Goes on at the instruction with this index.
    Jump(usize),
    /// Pops the top, and goes on at this index if it was 0.
    JumpIfZero(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    Unknown(String),
    Twice(String),
    NoSuchLoop,
}

/// Turns a program, which is a block, into instructions that leave the stack empty.
pub fn compile(program: &[Stmt]) -> Result<Vec<Instr>, CompileError> {
    todo!()
}
