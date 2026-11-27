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

struct Loop {
    /// Variables that existed when the loop began: they survive leaving it.
    kept: usize,
    exits: Vec<usize>,
}

#[derive(Default)]
struct Compiler<'a> {
    code: Vec<Instr>,
    /// The variables that are alive, in stack order: a variable's index here is its slot.
    live: Vec<&'a str>,
    /// Where the current block's own variables begin.
    block: usize,
    loops: Vec<Loop>,
}

impl<'a> Compiler<'a> {
    fn slot(&self, name: &str) -> Result<usize, CompileError> {
        let found = self.live.iter().rposition(|n| *n == name);
        found.ok_or_else(|| CompileError::Unknown(name.to_string()))
    }

    fn expr(&mut self, e: &Expr) -> Result<(), CompileError> {
        match e {
            Expr::Number(n) => self.code.push(Instr::Push(*n)),
            Expr::Name(name) => {
                let slot = self.slot(name)?;
                self.code.push(Instr::Copy(slot));
            }
            Expr::Add(a, b) | Expr::Sub(a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                self.code.push(if matches!(e, Expr::Add(..)) {
                    Instr::Add
                } else {
                    Instr::Sub
                });
            }
        }
        Ok(())
    }

    /// Compiles the statements as one block, which removes its variables at the end.
    fn block(&mut self, list: &'a [Stmt]) -> Result<(), CompileError> {
        let outer = self.block;
        self.block = self.live.len();
        for s in list {
            self.stmt(s)?;
        }
        if self.block == 0 {
            while self.live.pop().is_some() {
                self.code.push(Instr::Pop);
            }
        }
        self.block = outer;
        Ok(())
    }

    fn stmt(&mut self, s: &'a Stmt) -> Result<(), CompileError> {
        match s {
            Stmt::Let(name, value) => {
                self.expr(value)?; // before the new name exists
                if self.live[self.block..].contains(&name.as_str()) {
                    return Err(CompileError::Twice(name.clone()));
                }
                self.live.push(name); // the value is already in the right slot
            }
            Stmt::Assign(name, value) => {
                self.expr(value)?;
                let slot = self.slot(name)?;
                self.code.push(Instr::Store(slot));
            }
            Stmt::Print(value) => {
                self.expr(value)?;
                self.code.push(Instr::Print);
            }
            Stmt::Block(list) => self.block(list)?,
            Stmt::Loop(list) => {
                let start = self.code.len();
                self.loops.push(Loop {
                    kept: self.live.len(),
                    exits: Vec::new(),
                });
                self.block(list)?;
                self.code.push(Instr::Jump(start));
                let finished = self.loops.pop().expect("pushed above");
                let end = self.code.len();
                for at in finished.exits {
                    self.code[at] = Instr::Jump(end);
                }
            }
            Stmt::LeaveIf(test, levels) => {
                self.expr(test)?;
                let target = self.loops.len().checked_sub(*levels);
                let target = target
                    .filter(|_| *levels > 0)
                    .ok_or(CompileError::NoSuchLoop)?;
                let stay = self.code.len();
                self.code.push(Instr::JumpIfZero(0));
                // Remove what the loops being left have made. The compiler still needs those names.
                let extra = self.live.len() - self.loops[target].kept;
                self.code.extend(std::iter::repeat_n(Instr::Pop, extra));
                let exit = self.code.len();
                self.code.push(Instr::Jump(0));
                self.loops[target].exits.push(exit);
                self.code[stay] = Instr::JumpIfZero(self.code.len());
            }
        }
        Ok(())
    }
}

/// Turns a program, which is a block, into instructions that leave the stack empty.
pub fn compile(program: &[Stmt]) -> Result<Vec<Instr>, CompileError> {
    let mut compiler = Compiler::default();
    compiler.block(program)?;
    Ok(compiler.code)
}
