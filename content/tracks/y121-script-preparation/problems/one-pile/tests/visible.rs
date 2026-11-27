use solution::*;

#[allow(dead_code)]
mod build {
    use solution::*;
    pub fn n(x: i64) -> Expr {
        Expr::Number(x)
    }
    pub fn v(x: &str) -> Expr {
        Expr::Name(x.into())
    }
    pub fn add(a: Expr, b: Expr) -> Expr {
        Expr::Add(Box::new(a), Box::new(b))
    }
    pub fn sub(a: Expr, b: Expr) -> Expr {
        Expr::Sub(Box::new(a), Box::new(b))
    }
    pub fn le(x: &str, e: Expr) -> Stmt {
        Stmt::Let(x.into(), e)
    }
    pub fn set(x: &str, e: Expr) -> Stmt {
        Stmt::Assign(x.into(), e)
    }
    pub fn print(e: Expr) -> Stmt {
        Stmt::Print(e)
    }
    pub fn block(s: Vec<Stmt>) -> Stmt {
        Stmt::Block(s)
    }
    pub fn forever(s: Vec<Stmt>) -> Stmt {
        Stmt::Loop(s)
    }
    pub fn leave(e: Expr, levels: usize) -> Stmt {
        Stmt::LeaveIf(e, levels)
    }
}

/// What a run gave: the printed values, and the most slots the stack ever held.
#[allow(dead_code)]
fn device(code: &[Instr]) -> Result<(Vec<i64>, usize), String> {
    let (mut stack, mut printed, mut at, mut peak) = (Vec::new(), Vec::new(), 0, 0);
    for _ in 0..200_000 {
        if at == code.len() {
            return if stack.is_empty() {
                Ok((printed, peak))
            } else {
                Err(format!("{} values left on the stack", stack.len()))
            };
        }
        let instr = *code.get(at).ok_or("a jump goes past the end")?;
        at += 1;
        let empty = || format!("the stack is empty at {instr:?}");
        match instr {
            Instr::Push(x) => stack.push(x),
            Instr::Copy(slot) => {
                let value = *stack.get(slot).ok_or(format!("no slot {slot}"))?;
                stack.push(value);
            }
            Instr::Store(slot) => {
                let value = stack.pop().ok_or_else(empty)?;
                *stack.get_mut(slot).ok_or(format!("no slot {slot}"))? = value;
            }
            Instr::Add | Instr::Sub => {
                let b: i64 = stack.pop().ok_or_else(empty)?;
                let a: i64 = stack.pop().ok_or_else(empty)?;
                stack.push(if instr == Instr::Add {
                    a.wrapping_add(b)
                } else {
                    a.wrapping_sub(b)
                });
            }
            Instr::Print => printed.push(stack.pop().ok_or_else(empty)?),
            Instr::Pop => {
                stack.pop().ok_or_else(empty)?;
            }
            Instr::Jump(to) => at = to,
            Instr::JumpIfZero(to) => {
                if stack.pop().ok_or_else(empty)? == 0 {
                    at = to;
                }
            }
        }
        peak = peak.max(stack.len());
    }
    Err("the program does not end".into())
}

#[allow(dead_code)]
fn go(program: &[Stmt]) -> Result<Vec<i64>, String> {
    let code = compile(program).map_err(|e| format!("{e:?}"))?;
    device(&code).map(|(printed, _)| printed)
}

#[test]
fn variables_and_blocks() {
    use build::*;
    let p = [
        le("x", n(1)),
        block(vec![le("x", add(v("x"), n(10))), print(v("x"))]),
        print(v("x")),
    ];
    check!("shadowing", go(&p), Ok(vec![11, 1]));
}

#[test]
fn counting_down() {
    use build::*;
    let p = [
        le("i", n(3)),
        forever(vec![
            le("done", sub(n(1), n(1))),
            print(v("i")),
            set("i", sub(v("i"), n(1))),
            leave(sub(n(1), v("i")), 1),
        ]),
    ];
    // Leaves as soon as 1 - i is not 0, which is at once unless i is 1.
    check!("one round", go(&p), Ok(vec![3]));
}

#[test]
fn leaving_two_loops() {
    use build::*;
    let p = [
        forever(vec![
            le("a", n(1)),
            forever(vec![
                le("b", n(2)),
                print(add(v("a"), v("b"))),
                leave(n(1), 2),
            ]),
        ]),
        print(n(0)),
    ];
    check!("two at once", go(&p), Ok(vec![3, 0]));
}

#[test]
fn mistakes() {
    use build::*;
    check!(
        "unknown",
        compile(&[print(v("y"))]),
        Err(CompileError::Unknown("y".into()))
    );
    check!(
        "twice",
        compile(&[le("a", n(1)), le("a", n(2))]),
        Err(CompileError::Twice("a".into()))
    );
    check!(
        "no loop",
        compile(&[leave(n(1), 1)]),
        Err(CompileError::NoSuchLoop)
    );
}

#[test]
fn ended_blocks_give_their_slots_back() {
    use build::*;
    let p = [
        block(vec![le("a", n(1)), le("b", n(2))]),
        block(vec![le("c", n(3)), print(v("c"))]),
    ];
    let code = compile(&p).unwrap();
    check!("two blocks", device(&code), Ok((vec![3], 2)));
}
