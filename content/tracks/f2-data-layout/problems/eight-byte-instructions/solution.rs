/// One of the interpreter's 256 registers.
pub type Reg = u8;

/// What the front end emits. Arithmetic wraps; `Lt` sets `dst` to 1 or 0; `BrIf` jumps when `cond` isn't 0;
/// `BrTable` jumps to `targets[index]`, or to `default` when `index` is negative or out of range.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Const {
        dst: Reg,
        value: i64,
    },
    Add {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    Sub {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    Mul {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    Lt {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    Jump {
        target: u32,
    },
    BrIf {
        cond: Reg,
        target: u32,
    },
    BrTable {
        index: Reg,
        targets: Vec<u32>,
        default: u32,
    },
    Ret {
        src: Reg,
    },
}

/// What the interpreter runs: 8 bytes. A tag byte and at most 7 bytes of operands laid out around it;
/// anything bigger (an `i64` constant, a jump table) lives in a side pool and is named by a `u32` index,
/// as Cranelift's `InstructionData` does with its constant and jump-table pools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inst {
    Const { dst: Reg, k: u32 },
    Add { dst: Reg, a: Reg, b: Reg },
    Sub { dst: Reg, a: Reg, b: Reg },
    Mul { dst: Reg, a: Reg, b: Reg },
    Lt { dst: Reg, a: Reg, b: Reg },
    Jump { target: u32 },
    BrIf { cond: Reg, target: u32 },
    BrTable { index: Reg, table: u32 },
    Ret { src: Reg },
}

/// A function body: instructions indexed from 0, plus the pools they point into.
pub struct Program {
    insts: Vec<Inst>,
    consts: Vec<i64>,
    tables: Vec<Box<[u32]>>,
}

impl Program {
    pub fn new() -> Program {
        Program {
            insts: Vec::new(),
            consts: Vec::new(),
            tables: Vec::new(),
        }
    }

    /// Appends `op` and returns its index.
    pub fn push(&mut self, op: Op) -> u32 {
        let inst = match op {
            Op::Const { dst, value } => {
                self.consts.push(value);
                Inst::Const {
                    dst,
                    k: self.consts.len() as u32 - 1,
                }
            }
            Op::Add { dst, a, b } => Inst::Add { dst, a, b },
            Op::Sub { dst, a, b } => Inst::Sub { dst, a, b },
            Op::Mul { dst, a, b } => Inst::Mul { dst, a, b },
            Op::Lt { dst, a, b } => Inst::Lt { dst, a, b },
            Op::Jump { target } => Inst::Jump { target },
            Op::BrIf { cond, target } => Inst::BrIf { cond, target },
            Op::BrTable {
                index,
                targets,
                default,
            } => {
                // Default first, then the targets: one slice per table in a side pool.
                let mut table = Vec::with_capacity(targets.len() + 1);
                table.push(default);
                table.extend(targets);
                self.tables.push(table.into_boxed_slice());
                Inst::BrTable {
                    index,
                    table: self.tables.len() as u32 - 1,
                }
            }
            Op::Ret { src } => Inst::Ret { src },
        };
        self.insts.push(inst);
        self.insts.len() as u32 - 1
    }

    pub fn len(&self) -> usize {
        self.insts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.insts.is_empty()
    }

    /// Runs from instruction 0 with every register 0 until a `Ret`, and returns its value. `None` if
    /// control runs off the end or more than `fuel` instructions execute.
    pub fn run(&self, fuel: u64) -> Option<i64> {
        let mut regs = [0i64; 256];
        let mut pc = 0usize;
        for _ in 0..fuel {
            let inst = *self.insts.get(pc)?;
            pc += 1;
            match inst {
                Inst::Const { dst, k } => regs[dst as usize] = self.consts[k as usize],
                Inst::Add { dst, a, b } => {
                    regs[dst as usize] = regs[a as usize].wrapping_add(regs[b as usize])
                }
                Inst::Sub { dst, a, b } => {
                    regs[dst as usize] = regs[a as usize].wrapping_sub(regs[b as usize])
                }
                Inst::Mul { dst, a, b } => {
                    regs[dst as usize] = regs[a as usize].wrapping_mul(regs[b as usize])
                }
                Inst::Lt { dst, a, b } => {
                    regs[dst as usize] = (regs[a as usize] < regs[b as usize]) as i64
                }
                Inst::Jump { target } => pc = target as usize,
                Inst::BrIf { cond, target } => {
                    if regs[cond as usize] != 0 {
                        pc = target as usize;
                    }
                }
                Inst::BrTable { index, table } => {
                    let table = &self.tables[table as usize];
                    let i = regs[index as usize];
                    // Targets start at 1; anything negative or past the end takes the default at 0.
                    pc = usize::try_from(i)
                        .ok()
                        .and_then(|i| table.get(i + 1))
                        .copied()
                        .unwrap_or(table[0]) as usize;
                }
                Inst::Ret { src } => return Some(regs[src as usize]),
            }
        }
        None
    }
}
