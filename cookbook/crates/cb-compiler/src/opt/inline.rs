//! Function inlining with a simple cost model, plus removal of functions that
//! became unreferenced.
//!
//! Policy: inline a direct call when the callee is not part of any call cycle
//! (so recursion is never unrolled), is not a `const` initialiser, and either
//! is tiny (`SMALL` statements) or has exactly one call site and is never used
//! as a function value (`SINGLE`). Callers stop growing past `CALLER_CAP`.
//!
//! Scope trees keep stack traces exact: the callee's root scope is attached
//! below the call site's scope, so a trap in inlined code still reports
//! `at callee` / `at caller` frames.

use crate::mir::*;
use std::collections::HashSet;

const SMALL: usize = 24;
const SINGLE: usize = 400;
const CALLER_CAP: usize = 3000;

struct Info {
    recursive: Vec<bool>,
    direct_sites: Vec<usize>,
    address_taken: Vec<bool>,
}

fn analyse(p: &MirProgram) -> Info {
    let n = p.funcs.len();
    let mut direct_sites = vec![0; n];
    let mut address_taken = vec![false; n];
    let mut edges: Vec<Vec<usize>> = vec![vec![]; n];
    let mut has_indirect = vec![false; n];
    for (i, f) in p.funcs.iter().enumerate() {
        for b in f.reachable() {
            let blk = &f.blocks[b as usize];
            let mut note = |o: &Operand| {
                if let Operand::Const(Const::Fn(c)) = o {
                    address_taken[*c as usize] = true;
                }
            };
            blk.stmts.iter().for_each(|s| s.for_each_use(&mut note));
            blk.term.for_each_use(&mut note);
            if let Terminator::Call { callee, .. } = &blk.term {
                match callee {
                    Callee::Direct(c) => {
                        direct_sites[*c as usize] += 1;
                        edges[i].push(*c as usize);
                    }
                    Callee::Indirect(_) => has_indirect[i] = true,
                }
            }
        }
    }
    for i in 0..n {
        if has_indirect[i] {
            for c in 0..n {
                if address_taken[c] {
                    edges[i].push(c);
                }
            }
        }
    }
    let recursive = (0..n)
        .map(|s| {
            let mut seen = vec![false; n];
            let mut stack: Vec<usize> = edges[s].clone();
            while let Some(x) = stack.pop() {
                if x == s {
                    return true;
                }
                if !seen[x] {
                    seen[x] = true;
                    stack.extend(&edges[x]);
                }
            }
            false
        })
        .collect();
    Info {
        recursive,
        direct_sites,
        address_taken,
    }
}

pub fn inline(p: &mut MirProgram) -> bool {
    let mut any = false;
    for _round in 0..3 {
        let info = analyse(p);
        let mut changed = false;
        for ci in 0..p.funcs.len() {
            let orig = p.funcs[ci].blocks.len();
            for bi in 0..orig {
                let Terminator::Call {
                    callee: Callee::Direct(c),
                    ..
                } = &p.funcs[ci].blocks[bi].term
                else {
                    continue;
                };
                let c = *c as usize;
                let callee = &p.funcs[c];
                let size = callee.size();
                let ok = c != ci
                    && !info.recursive[c]
                    && !callee.is_const
                    && (size <= SMALL
                        || (info.direct_sites[c] == 1 && !info.address_taken[c] && size <= SINGLE))
                    && p.funcs[ci].size() < CALLER_CAP;
                if !ok {
                    continue;
                }
                let callee = p.funcs[c].clone();
                inline_call(&mut p.funcs[ci], bi, &callee);
                changed = true;
            }
        }
        any |= changed;
        if !changed {
            break;
        }
    }
    any |= remove_dead_funcs(p);
    any
}

fn shift(o: &mut Operand, lo: u32) {
    if let Operand::Local(l) = o {
        *l += lo;
    }
}

fn inline_call(caller: &mut Func, bi: usize, callee: &Func) {
    let Terminator::Call {
        args,
        dest,
        target,
        scope,
        span,
        ..
    } = std::mem::replace(&mut caller.blocks[bi].term, Terminator::Unreachable)
    else {
        return;
    };
    let (lo, bo, so) = (
        caller.locals.len() as u32,
        caller.blocks.len() as u32,
        caller.scopes.len() as u32,
    );
    caller.locals.extend(callee.locals.iter().cloned());
    for (i, s) in callee.scopes.iter().enumerate() {
        let parent = if i == 0 {
            Some(scope)
        } else {
            s.parent.map(|p| p + so)
        };
        let call_site = if i == 0 { span } else { s.call_site };
        caller.scopes.push(Scope {
            name: s.name.clone(),
            parent,
            call_site,
        });
    }
    for (i, a) in args.into_iter().enumerate() {
        caller.blocks[bi].stmts.push(Stmt::at(
            StmtKind::Assign(lo + i as u32, Rvalue::Use(a)),
            scope,
            span,
        ));
    }
    caller.blocks[bi].term = Terminator::Goto(bo);
    for blk in &callee.blocks {
        let mut nb = blk.clone();
        for s in &mut nb.stmts {
            s.scope += so;
            match &mut s.kind {
                StmtKind::Assign(d, _) | StmtKind::SetField(d, ..) => *d += lo,
                StmtKind::SetIndex(..) => {}
            }
            s.for_each_use_mut(&mut |o| shift(o, lo));
        }
        nb.term.for_each_use_mut(&mut |o| shift(o, lo));
        nb.term.for_each_succ_mut(&mut |t| *t += bo);
        match &mut nb.term {
            Terminator::Call { dest, scope, .. } => {
                *dest += lo;
                *scope += so;
            }
            Terminator::Return(o) => {
                let v = o.clone();
                nb.stmts
                    .push(Stmt::at(StmtKind::Assign(dest, Rvalue::Use(v)), so, span));
                nb.term = Terminator::Goto(target);
            }
            _ => {}
        }
        caller.blocks.push(nb);
    }
}

/// Delete functions not reachable from `main` (calls and function constants)
/// and renumber the rest.
pub fn remove_dead_funcs(p: &mut MirProgram) -> bool {
    p.funcs.iter_mut().for_each(super::cfg::remove_unreachable);
    let n = p.funcs.len();
    let mut live: HashSet<usize> = HashSet::from([p.main as usize]);
    let mut stack = vec![p.main as usize];
    while let Some(i) = stack.pop() {
        let f = &p.funcs[i];
        let mut found = Vec::new();
        for b in f.reachable() {
            let blk = &f.blocks[b as usize];
            let mut note = |o: &Operand| {
                if let Operand::Const(Const::Fn(c)) = o {
                    found.push(*c as usize);
                }
            };
            blk.stmts.iter().for_each(|s| s.for_each_use(&mut note));
            blk.term.for_each_use(&mut note);
            if let Terminator::Call {
                callee: Callee::Direct(c),
                ..
            } = &blk.term
            {
                found.push(*c as usize);
            }
        }
        for c in found {
            if live.insert(c) {
                stack.push(c);
            }
        }
    }
    if live.len() == n {
        return false;
    }
    let mut new_id = vec![0u32; n];
    let mut next = 0;
    for (i, id) in new_id.iter_mut().enumerate() {
        if live.contains(&i) {
            *id = next;
            next += 1;
        }
    }
    let old = std::mem::take(&mut p.funcs);
    p.funcs = old
        .into_iter()
        .enumerate()
        .filter(|(i, _)| live.contains(i))
        .map(|(_, f)| f)
        .collect();
    p.main = new_id[p.main as usize];
    for f in &mut p.funcs {
        for blk in &mut f.blocks {
            let mut fix = |o: &mut Operand| {
                if let Operand::Const(Const::Fn(c)) = o {
                    *c = new_id[*c as usize];
                }
            };
            blk.stmts
                .iter_mut()
                .for_each(|s| s.for_each_use_mut(&mut fix));
            blk.term.for_each_use_mut(&mut fix);
            if let Terminator::Call {
                callee: Callee::Direct(c),
                ..
            } = &mut blk.term
            {
                *c = new_id[*c as usize];
            }
        }
    }
    true
}
