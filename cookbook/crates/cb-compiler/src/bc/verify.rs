//! Bytecode verifier. A chunk that passes can be run by the VM without any
//! out-of-range register, constant, function, list, switch or jump access, and
//! no function can fall off the end of its code. (Type errors, e.g. adding a
//! string, are still caught at run time as `Internal` errors, never as panics.)

use super::*;

pub fn verify(c: &Chunk) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    match c.funcs.get(c.main as usize) {
        None => errs.push("main function index out of range".into()),
        Some(m) if m.nparams != 0 => errs.push("main must take no parameters".into()),
        _ => {}
    }
    for (fi, f) in c.funcs.iter().enumerate() {
        verify_func(c, fi, f, &mut errs);
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn verify_func(c: &Chunk, fi: usize, f: &BcFunc, errs: &mut Vec<String>) {
    let mut err = |pc: usize, m: String| errs.push(format!("f{fi} `{}` @{pc}: {m}", f.name));
    if f.nparams > f.nregs {
        err(0, "more parameters than registers".into());
    }
    if f.code.is_empty() {
        return err(0, "empty function".into());
    }
    if f.pc_scope.len() != f.code.len() {
        err(0, "pc_scope length differs from code length".into());
    }
    if f.pc_scope.iter().any(|s| *s as usize >= f.scopes.len()) {
        err(0, "instruction scope out of range".into());
    }
    if f.scopes
        .iter()
        .enumerate()
        .any(|(i, s)| s.parent.is_some_and(|p| p as usize >= i))
    {
        err(0, "scope parent out of range".into());
    }
    let n = f.code.len() as u32;
    for (i, l) in f.lists.iter().enumerate() {
        if l.iter().any(|r| *r >= f.nregs) {
            err(0, format!("list {i} names a register >= {}", f.nregs));
        }
    }
    for (i, s) in f.switches.iter().enumerate() {
        if s.default >= n || s.cases.iter().any(|c| c.1 >= n) {
            err(0, format!("switch table {i} has a target out of range"));
        }
    }
    for (pc, ins) in f.code.iter().enumerate() {
        let mut bad: Vec<String> = Vec::new();
        ins.visit(&mut |v| match v {
            Visit::Reg(r) if r >= f.nregs => {
                bad.push(format!("register r{r} >= nregs {}", f.nregs))
            }
            Visit::Const(k) if k as usize >= c.consts.len() => {
                bad.push(format!("constant k{k} out of range"))
            }
            Visit::Target(t) if t >= n => bad.push(format!("jump target @{t} out of range")),
            Visit::Func(x) if x as usize >= c.funcs.len() => {
                bad.push(format!("function f{x} out of range"))
            }
            Visit::List(l) if l as usize >= f.lists.len() => {
                bad.push(format!("list {l} out of range"))
            }
            Visit::Switch(s) if s as usize >= f.switches.len() => {
                bad.push(format!("switch {s} out of range"))
            }
            _ => {}
        });
        for m in bad {
            err(pc, m);
        }
        match ins {
            Instr::Call {
                f: callee, args, ..
            } => {
                if let (Some(g), Some(l)) =
                    (c.funcs.get(callee.0 as usize), f.lists.get(args.0 as usize))
                    && g.nparams as usize != l.len()
                {
                    err(
                        pc,
                        format!(
                            "call passes {} arguments to `{}` which takes {}",
                            l.len(),
                            g.name,
                            g.nparams
                        ),
                    );
                }
            }
            Instr::LoadFn { f: callee, .. }
                if c.funcs.get(callee.0 as usize).is_some_and(|g| g.is_const) =>
            {
                err(pc, "function value of a const initialiser".into());
            }
            _ => {}
        }
    }
    if f.code.last().is_some_and(|l| l.falls_through()) {
        err(
            f.code.len() - 1,
            "control can fall off the end of the function".into(),
        );
    }
}
