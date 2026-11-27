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

struct Frame {
    routine: usize,
    /// The index of the next instruction.
    next: usize,
    /// Where this routine's slot 0 is on the stack.
    base: usize,
    line: u32,
}

struct Machine<'a> {
    routines: &'a [Routine],
    limits: Limits,
    stack: Vec<i64>,
    frames: Vec<Frame>,
}

impl Machine<'_> {
    fn push(&mut self, value: i64) -> Result<(), Cause> {
        if self.stack.len() >= self.limits.values {
            return Err(Cause::TooManyValues);
        }
        self.stack.push(value);
        Ok(())
    }

    /// Takes the top value, but never one that belongs to a caller.
    fn pop(&mut self) -> Result<i64, Cause> {
        let base = self.frames.last().expect("a routine is running").base;
        if self.stack.len() <= base {
            return Err(Cause::Underflow);
        }
        self.stack.pop().ok_or(Cause::Underflow)
    }

    fn slot(&self, i: usize) -> Result<usize, Cause> {
        let base = self.frames.last().expect("a routine is running").base;
        base.checked_add(i)
            .filter(|&at| at < self.stack.len())
            .ok_or(Cause::BadSlot)
    }

    /// Starts a routine whose arguments are already on top of the stack.
    fn enter(&mut self, routine: usize) -> Result<(), Cause> {
        let Some(wanted) = self.routines.get(routine).map(|r| r.takes) else {
            return Err(Cause::BadRoutine);
        };
        let floor = self.frames.last().map_or(0, |f| f.base);
        if self.stack.len() - floor < wanted {
            return Err(Cause::Underflow);
        }
        if self.frames.len() >= self.limits.calls {
            return Err(Cause::TooManyCalls);
        }
        let base = self.stack.len() - wanted;
        self.frames.push(Frame {
            routine,
            next: 0,
            base,
            line: 0,
        });
        Ok(())
    }

    fn run(&mut self) -> Result<i64, Cause> {
        let mut steps = 0u64;
        loop {
            let frame = self.frames.last_mut().expect("a routine is running");
            let code = &self.routines[frame.routine].code;
            let Some(&(instr, line)) = code.get(frame.next) else {
                return Err(Cause::BadAddress);
            };
            frame.line = line;
            frame.next += 1;
            steps += 1;
            if steps > self.limits.steps {
                return Err(Cause::OutOfSteps);
            }
            match instr {
                Instr::Push(v) => self.push(v)?,
                Instr::Load(i) => {
                    let value = self.stack[self.slot(i)?];
                    self.push(value)?;
                }
                Instr::Store(i) => {
                    let value = self.pop()?;
                    let at = self.slot(i)?;
                    self.stack[at] = value;
                }
                Instr::Pop => {
                    self.pop()?;
                }
                Instr::Add | Instr::Sub | Instr::Mul => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = match instr {
                        Instr::Add => a.checked_add(b),
                        Instr::Sub => a.checked_sub(b),
                        _ => a.checked_mul(b),
                    };
                    self.push(result.ok_or(Cause::Overflow)?)?;
                }
                Instr::Jump(to) => self.jump(to),
                Instr::JumpIfZero(to) => {
                    if self.pop()? == 0 {
                        self.jump(to);
                    }
                }
                Instr::Call(routine) => self.enter(routine)?,
                Instr::Return => {
                    let result = self.pop()?;
                    let frame = self.frames.pop().expect("a routine is running");
                    if self.frames.is_empty() {
                        return Ok(result);
                    }
                    self.stack.truncate(frame.base);
                    self.push(result)?;
                }
                Instr::Fail(code) => return Err(Cause::Failed(code)),
            }
        }
    }

    fn jump(&mut self, to: usize) {
        self.frames.last_mut().expect("a routine is running").next = to;
    }

    fn trace(&self) -> Vec<(String, u32)> {
        let frames = self.frames.iter().rev();
        frames
            .map(|f| {
                let r = &self.routines[f.routine];
                let line = r.code.get(f.next).map_or(f.line, |c| c.1);
                (r.name.clone(), line)
            })
            .collect()
    }
}

/// Calls routine `entry` with `arguments` and returns its result.
pub fn run(
    routines: &[Routine],
    entry: usize,
    arguments: &[i64],
    limits: Limits,
) -> Result<i64, Crash> {
    let mut machine = Machine {
        routines,
        limits,
        stack: Vec::new(),
        frames: Vec::new(),
    };
    let mut result = Ok(0);
    for &a in arguments {
        result = result.and_then(|_| machine.push(a).map(|_| 0));
    }
    result
        .and_then(|_| machine.enter(entry))
        .and_then(|_| machine.run())
        .map_err(|cause| Crash {
            cause,
            trace: machine.trace(),
        })
}
