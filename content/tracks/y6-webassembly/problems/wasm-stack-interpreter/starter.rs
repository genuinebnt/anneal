/// One function: how many parameters and results it has, how many extra (zero-initialised) locals, and its instruction
/// bytes, which end with the `end` opcode 0x0B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub params: usize,
    pub results: usize,
    pub locals: usize,
    pub code: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    Unreachable,
    DivideByZero,
    IntegerOverflow,
    OutOfFuel,
    /// More than 100 active calls.
    StackOverflow,
    /// A call (or the entry point) named a function that does not exist.
    BadFunction(usize),
    /// Unknown opcode, bad immediate, stack underflow, bad local or label index, unbalanced blocks, wrong argument count.
    Malformed,
}

/// Reads an unsigned LEB128 immediate at `*pc` and moves `*pc` past it. (Provided.)
fn read_u32(code: &[u8], pc: &mut usize) -> Result<u32, Trap> {
    let mut result: u32 = 0;
    for i in 0..5 {
        let b = *code.get(*pc).ok_or(Trap::Malformed)?;
        *pc += 1;
        result |= ((b & 0x7F) as u32).checked_shl(7 * i).ok_or(Trap::Malformed)?;
        if b & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(Trap::Malformed)
}

/// Reads a signed LEB128 immediate (an `i32.const` operand) at `*pc` and moves `*pc` past it. (Provided.)
fn read_i32(code: &[u8], pc: &mut usize) -> Result<i32, Trap> {
    let mut result: i64 = 0;
    let mut shift = 0;
    loop {
        let b = *code.get(*pc).ok_or(Trap::Malformed)?;
        *pc += 1;
        result |= ((b & 0x7F) as i64) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            if b & 0x40 != 0 {
                result |= -1i64 << shift;
            }
            return Ok(result as i32);
        }
        if shift >= 35 {
            return Err(Trap::Malformed);
        }
    }
}

/// Runs function `entry` with `args` and returns its results. Every instruction executed costs one unit of `fuel`;
/// running out is `Trap::OutOfFuel`.
pub fn run(funcs: &[Func], entry: usize, args: &[i32], fuel: u64) -> Result<Vec<i32>, Trap> {
    todo!()
}
