#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instr {
    Push(i64),
    /// Pushes a copy of this routine's slot.
    Load(usize),
    /// Pops the top and writes it into this routine's slot.
    Store(usize),
    Pop,
    Add,
    Sub,
    Mul,
    /// Goes on at this index of this routine.
    Jump(usize),
    /// Pops the top, and goes on at this index if it was 0.
    JumpIfZero(usize),
    /// Calls the routine with this index.
    Call(usize),
    /// Pops the result and leaves this routine.
    Return,
    /// Stops everything with this code.
    Fail(u32),
}

#[derive(Debug, Clone)]
pub struct Routine {
    pub name: String,
    /// How many values a call takes.
    pub takes: usize,
    /// Each instruction with the source line it came from.
    pub code: Vec<(Instr, u32)>,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Routines that may be active at once.
    pub calls: usize,
    /// Values on the stack at once.
    pub values: usize,
    /// Instructions run in all.
    pub steps: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Failed(u32),
    Underflow,
    BadSlot,
    BadAddress,
    BadRoutine,
    Overflow,
    TooManyCalls,
    TooManyValues,
    OutOfSteps,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    pub cause: Cause,
    /// The active routines, innermost first, each with the line it had reached.
    pub trace: Vec<(String, u32)>,
}

/// Calls routine `entry` with `arguments` and returns its result.
pub fn run(
    routines: &[Routine],
    entry: usize,
    arguments: &[i64],
    limits: Limits,
) -> Result<i64, Crash> {
    todo!()
}
