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

use std::collections::HashMap;

/// For each `block`/`loop`/`if` pc: where its `end` is and where its `else` is. For each `else` pc: where its `end` is.
type Jumps = HashMap<usize, (usize, Option<usize>)>;

/// The pc after the instruction at `pc` (skipping its immediates), or `Malformed` for an unknown opcode.
fn next_pc(code: &[u8], pc: usize) -> Result<usize, Trap> {
    let op = code[pc];
    let mut q = pc + 1;
    match op {
        0x00 | 0x01 | 0x05 | 0x0B | 0x0F | 0x1A | 0x1B | 0x45..=0x4F | 0x6A..=0x6D | 0x6F | 0x71..=0x76 => {}
        0x02..=0x04 => q += 1,
        0x0C | 0x0D | 0x10 | 0x20..=0x22 => {
            read_u32(code, &mut q)?;
        }
        0x41 => {
            read_i32(code, &mut q)?;
        }
        _ => return Err(Trap::Malformed),
    }
    if q > code.len() {
        return Err(Trap::Malformed);
    }
    Ok(q)
}

fn analyze(code: &[u8]) -> Result<Jumps, Trap> {
    let mut jumps = Jumps::new();
    let mut open: Vec<(usize, Option<usize>)> = Vec::new(); // (block pc, else pc)
    let mut pc = 0;
    let mut closed = false;
    while pc < code.len() {
        if closed {
            return Err(Trap::Malformed); // bytes after the function's own end
        }
        match code[pc] {
            0x02..=0x04 => open.push((pc, None)),
            0x05 => {
                let top = open.last_mut().ok_or(Trap::Malformed)?;
                if code[top.0] != 0x04 || top.1.is_some() {
                    return Err(Trap::Malformed);
                }
                top.1 = Some(pc);
            }
            0x0B => match open.pop() {
                Some((start, els)) => {
                    jumps.insert(start, (pc, els));
                    if let Some(e) = els {
                        jumps.insert(e, (pc, None));
                    }
                }
                None => closed = true,
            },
            _ => {}
        }
        pc = next_pc(code, pc)?;
    }
    if !closed || !open.is_empty() {
        return Err(Trap::Malformed);
    }
    Ok(jumps)
}

struct Label {
    is_loop: bool,
    cont: usize,
    height: usize,
    arity: usize,
}

fn block_arity(code: &[u8], pc: usize) -> Result<usize, Trap> {
    match code.get(pc + 1) {
        Some(0x40) => Ok(0),
        Some(0x7F) => Ok(1),
        _ => Err(Trap::Malformed),
    }
}

fn pop(stack: &mut Vec<i32>) -> Result<i32, Trap> {
    stack.pop().ok_or(Trap::Malformed)
}

/// Branch to label `depth`: keep the label's arity values, drop the rest of that block's stack, and continue at its target.
fn branch(labels: &mut Vec<Label>, stack: &mut Vec<i32>, depth: usize, pc: &mut usize) -> Result<(), Trap> {
    if depth >= labels.len() {
        return Err(Trap::Malformed);
    }
    let li = labels.len() - 1 - depth;
    let (is_loop, cont, height, arity) = {
        let l = &labels[li];
        (l.is_loop, l.cont, l.height, l.arity)
    };
    if stack.len() < height + arity {
        return Err(Trap::Malformed);
    }
    let keep = stack.split_off(stack.len() - arity);
    stack.truncate(height);
    stack.extend(keep);
    labels.truncate(if is_loop { li + 1 } else { li });
    *pc = cont;
    Ok(())
}

fn binop(op: u8, a: i32, b: i32) -> Result<i32, Trap> {
    Ok(match op {
        0x46 => (a == b) as i32,
        0x47 => (a != b) as i32,
        0x48 => (a < b) as i32,
        0x49 => ((a as u32) < (b as u32)) as i32,
        0x4A => (a > b) as i32,
        0x4B => ((a as u32) > (b as u32)) as i32,
        0x4C => (a <= b) as i32,
        0x4D => ((a as u32) <= (b as u32)) as i32,
        0x4E => (a >= b) as i32,
        0x4F => ((a as u32) >= (b as u32)) as i32,
        0x6A => a.wrapping_add(b),
        0x6B => a.wrapping_sub(b),
        0x6C => a.wrapping_mul(b),
        0x6D => {
            if b == 0 {
                return Err(Trap::DivideByZero);
            }
            if a == i32::MIN && b == -1 {
                return Err(Trap::IntegerOverflow);
            }
            a / b
        }
        0x6F => {
            if b == 0 {
                return Err(Trap::DivideByZero);
            }
            a.wrapping_rem(b)
        }
        0x71 => a & b,
        0x72 => a | b,
        0x73 => a ^ b,
        0x74 => a.wrapping_shl(b as u32),
        0x75 => a.wrapping_shr(b as u32),
        0x76 => (a as u32).wrapping_shr(b as u32) as i32,
        _ => return Err(Trap::Malformed),
    })
}

fn exec(funcs: &[Func], all: &[Jumps], idx: usize, args: &[i32], fuel: &mut u64, depth: usize) -> Result<Vec<i32>, Trap> {
    let f = funcs.get(idx).ok_or(Trap::BadFunction(idx))?;
    if depth > 100 {
        return Err(Trap::StackOverflow);
    }
    if args.len() != f.params {
        return Err(Trap::Malformed);
    }
    let code = &f.code;
    let jumps = &all[idx];
    let mut locals: Vec<i32> = args.to_vec();
    locals.resize(f.params + f.locals, 0);
    let mut stack: Vec<i32> = Vec::new();
    let mut labels = vec![Label { is_loop: false, cont: code.len(), height: 0, arity: f.results }];
    let mut pc = 0;
    while pc < code.len() {
        if *fuel == 0 {
            return Err(Trap::OutOfFuel);
        }
        *fuel -= 1;
        let op = code[pc];
        let mut next = next_pc(code, pc)?;
        match op {
            0x00 => return Err(Trap::Unreachable),
            0x01 => {}
            0x02 | 0x03 => {
                let arity = block_arity(code, pc)?;
                let (end, _) = jumps[&pc];
                labels.push(if op == 0x03 {
                    Label { is_loop: false, cont: end + 1, height: stack.len(), arity: 0 }
                } else {
                    Label { is_loop: false, cont: end + 1, height: stack.len(), arity }
                });
            }
            0x04 => {
                let arity = block_arity(code, pc)?;
                let cond = pop(&mut stack)?;
                let (end, els) = jumps[&pc];
                labels.push(Label { is_loop: false, cont: end + 1, height: stack.len(), arity });
                if cond == 0 {
                    next = match els {
                        Some(e) => e + 1,
                        None => end,
                    };
                }
            }
            0x05 => {
                next = jumps[&pc].0; // the then-branch is done: run the `end`
            }
            0x0B => {
                labels.pop();
            }
            0x0C | 0x0D => {
                let mut q = pc + 1;
                let d = read_u32(code, &mut q)? as usize;
                if op == 0x0C || pop(&mut stack)? != 0 {
                    branch(&mut labels, &mut stack, d, &mut next)?;
                }
            }
            0x0F => {
                let d = labels.len() - 1;
                branch(&mut labels, &mut stack, d, &mut next)?;
            }
            0x10 => {
                let mut q = pc + 1;
                let callee = read_u32(code, &mut q)? as usize;
                let n = funcs.get(callee).ok_or(Trap::BadFunction(callee))?.params;
                if stack.len() < n {
                    return Err(Trap::Malformed);
                }
                let a = stack.split_off(stack.len() - n);
                let r = exec(funcs, all, callee, &a, fuel, depth + 1)?;
                stack.extend(r);
            }
            0x1A => {
                pop(&mut stack)?;
            }
            0x1B => {
                let c = pop(&mut stack)?;
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(if c != 0 { a } else { b });
            }
            0x20..=0x22 => {
                let mut q = pc + 1;
                let i = read_u32(code, &mut q)? as usize;
                if i >= locals.len() {
                    return Err(Trap::Malformed);
                }
                match op {
                    0x20 => stack.push(locals[i]),
                    0x21 => locals[i] = pop(&mut stack)?,
                    _ => {
                        let v = *stack.last().ok_or(Trap::Malformed)?;
                        locals[i] = v;
                    }
                }
            }
            0x41 => {
                let mut q = pc + 1;
                stack.push(read_i32(code, &mut q)?);
            }
            0x45 => {
                let a = pop(&mut stack)?;
                stack.push((a == 0) as i32);
            }
            _ => {
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(binop(op, a, b)?);
            }
        }
        pc = next;
    }
    if stack.len() < f.results {
        return Err(Trap::Malformed);
    }
    Ok(stack.split_off(stack.len() - f.results))
}

pub fn run(funcs: &[Func], entry: usize, args: &[i32], fuel: u64) -> Result<Vec<i32>, Trap> {
    let all: Vec<Jumps> = funcs.iter().map(|f| analyze(&f.code)).collect::<Result<_, _>>()?;
    let mut fuel = fuel;
    exec(funcs, &all, entry, args, &mut fuel, 1)
}
