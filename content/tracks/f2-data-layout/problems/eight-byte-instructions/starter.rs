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

/// What the interpreter runs, one per op. For now, the op itself.
pub type Inst = Op;

/// A function body: instructions indexed from 0.
pub struct Program {
    insts: Vec<Inst>,
}

impl Program {
    pub fn new() -> Program {
        Program { insts: Vec::new() }
    }

    /// Appends `op` and returns its index.
    pub fn push(&mut self, op: Op) -> u32 {
        self.insts.push(op);
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
            let inst = self.insts.get(pc)?;
            pc += 1;
            match inst {
                Op::Const { dst, value } => regs[*dst as usize] = *value,
                Op::Add { dst, a, b } => {
                    regs[*dst as usize] = regs[*a as usize].wrapping_add(regs[*b as usize])
                }
                Op::Sub { dst, a, b } => {
                    regs[*dst as usize] = regs[*a as usize].wrapping_sub(regs[*b as usize])
                }
                Op::Mul { dst, a, b } => {
                    regs[*dst as usize] = regs[*a as usize].wrapping_mul(regs[*b as usize])
                }
                Op::Lt { dst, a, b } => {
                    regs[*dst as usize] = (regs[*a as usize] < regs[*b as usize]) as i64
                }
                Op::Jump { target } => pc = *target as usize,
                Op::BrIf { cond, target } => {
                    if regs[*cond as usize] != 0 {
                        pc = *target as usize;
                    }
                }
                Op::BrTable {
                    index,
                    targets,
                    default,
                } => {
                    let i = regs[*index as usize];
                    pc = usize::try_from(i)
                        .ok()
                        .and_then(|i| targets.get(i))
                        .copied()
                        .unwrap_or(*default) as usize;
                }
                Op::Ret { src } => return Some(regs[*src as usize]),
            }
        }
        None
    }
}
