//! SSA construction and destruction.
//!
//! **Construction** (`to_ssa`) is the classic pruned algorithm: dominators
//! (Cooper-Harvey-Kennedy, `opt::dom`), dominance frontiers, phi placement on
//! the iterated frontier of each local's definition blocks (only where the
//! local is live on entry: with definite initialisation that guarantees every
//! phi argument is defined), then renaming by a walk over the dominator tree
//! with one stack of versions per original local. `SetField(d, i, v)` has no
//! SSA meaning (it redefines its operand), so it first becomes
//! `d = with_field(d, i, v)`, a pure functional update.
//!
//! **Destruction** (`from_ssa`) turns phis back into copies:
//! 1. trivial and dead phis are removed;
//! 2. `with_field` results are *aliased* to their operand when the operand dies
//!    there (so struct updates stay in place instead of cloning);
//! 3. edges into phi blocks are split when the predecessor has several
//!    successors or ends in a call (the *critical edge* rule), so copies can be
//!    placed at the end of the predecessor without being executed on other paths
//!    (this is what fixes the **lost-copy problem**);
//! 4. the phis of a block become one *parallel copy* per predecessor, which is
//!    sequentialised with a temporary only when it contains a cycle (this fixes
//!    the **swap problem**);
//! 5. `with_field` becomes `SetField` again, `t = x; d = t` pairs are folded and the
//!    remaining copies whose two locals do not interfere are coalesced.

use super::dom::{self, DomTree};
use super::live::{liveness, step_back_stmt, step_back_term};
use crate::mir::*;
use std::collections::{HashMap, HashSet};

fn distinct_succs(t: &Terminator) -> Vec<BlockId> {
    let mut v = t.successors();
    v.sort_unstable();
    v.dedup();
    v
}

/// Give the entry block no predecessors by moving its body into a fresh block.
fn isolate_entry(f: &mut Func) {
    if f.predecessors()[0].is_empty() {
        return;
    }
    let new = f.blocks.len() as BlockId;
    let old = std::mem::replace(&mut f.blocks[0], Block::new(vec![], Terminator::Goto(new)));
    f.blocks.push(old);
    for b in 1..f.blocks.len() - 1 {
        f.blocks[b].term.for_each_succ_mut(&mut |t| {
            if *t == 0 {
                *t = new
            }
        });
    }
    let last = f.blocks.len() - 1;
    f.blocks[last].term.for_each_succ_mut(&mut |t| {
        if *t == 0 {
            *t = new
        }
    });
}

/// Convert a function to SSA form. Returns false when it already is.
pub fn to_ssa(f: &mut Func) -> bool {
    if f.ssa {
        return false;
    }
    super::cfg::remove_unreachable(f);
    isolate_entry(f);
    for blk in &mut f.blocks {
        for s in &mut blk.stmts {
            if let StmtKind::SetField(d, i, v) = &s.kind {
                s.kind = StmtKind::Assign(*d, Rvalue::WithField(Operand::Local(*d), *i, v.clone()));
            }
        }
    }
    let nl = f.locals.len();
    let nb = f.blocks.len();
    let live = liveness(f);
    let dt = DomTree::compute(f);
    let df = dt.frontiers();
    // Definition blocks per local (parameters are defined by the entry block).
    let mut def_blocks: Vec<Vec<BlockId>> = vec![Vec::new(); nl];
    for p in 0..f.params {
        def_blocks[p].push(0);
    }
    for &b in &dt.rpo {
        let blk = &f.blocks[b as usize];
        let mut note = |l: LocalId| {
            if !def_blocks[l as usize].contains(&b) {
                def_blocks[l as usize].push(b)
            }
        };
        for s in &blk.stmts {
            if let Some(d) = s.def() {
                note(d)
            }
        }
        if let Some(d) = blk.term.def() {
            note(d)
        }
    }
    // Phi placement. `phi_orig[b][k]` is the original local of the k-th phi of block b.
    let mut phi_orig: Vec<Vec<LocalId>> = vec![Vec::new(); nb];
    for l in 0..nl {
        if def_blocks[l].is_empty() {
            continue;
        }
        let mut placed = vec![false; nb];
        let mut work = def_blocks[l].clone();
        while let Some(x) = work.pop() {
            for &y in &df[x as usize] {
                if placed[y as usize] || !live.live_in[y as usize].contains(l as u32) {
                    continue;
                }
                placed[y as usize] = true;
                let args = dt.preds[y as usize]
                    .iter()
                    .map(|&p| (p, Operand::Local(l as LocalId)))
                    .collect();
                f.blocks[y as usize].phis.push(Phi {
                    dest: l as LocalId,
                    args,
                });
                phi_orig[y as usize].push(l as LocalId);
                if !def_blocks[l].contains(&y) {
                    work.push(y);
                }
            }
        }
    }
    // Renaming. The first definition met for a local keeps its id (so the
    // dump of a straight-line function is unchanged); later ones get fresh locals.
    let mut stacks: Vec<Vec<LocalId>> = vec![Vec::new(); nl];
    let mut id_taken = vec![false; nl];
    for p in 0..f.params {
        stacks[p].push(p as LocalId);
        id_taken[p] = true;
    }
    enum Ev {
        Enter(BlockId),
        Exit(Vec<LocalId>),
    }
    let mut work = vec![Ev::Enter(0)];
    while let Some(ev) = work.pop() {
        let b = match ev {
            Ev::Exit(pushed) => {
                for o in pushed {
                    stacks[o as usize].pop();
                }
                continue;
            }
            Ev::Enter(b) => b as usize,
        };
        let mut pushed: Vec<LocalId> = Vec::new();
        let mut fresh = |f: &mut Func,
                         orig: LocalId,
                         stacks: &mut Vec<Vec<LocalId>>,
                         pushed: &mut Vec<LocalId>|
         -> LocalId {
            let new = if !id_taken[orig as usize] {
                id_taken[orig as usize] = true;
                orig
            } else {
                let decl = f.locals[orig as usize].clone();
                f.locals.push(decl);
                (f.locals.len() - 1) as LocalId
            };
            stacks[orig as usize].push(new);
            pushed.push(orig);
            new
        };
        let ph_n = f.blocks[b].phis.len();
        for k in 0..ph_n {
            let orig = phi_orig[b][k];
            let new = fresh(f, orig, &mut stacks, &mut pushed);
            f.blocks[b].phis[k].dest = new;
        }
        let top = |stacks: &Vec<Vec<LocalId>>, o: &mut Operand| {
            if let Operand::Local(l) = o
                && let Some(&t) = stacks[*l as usize].last()
            {
                *l = t;
            }
        };
        for i in 0..f.blocks[b].stmts.len() {
            let mut st = std::mem::replace(
                &mut f.blocks[b].stmts[i],
                Stmt::new(
                    StmtKind::SetIndex(Operand::UNIT, Operand::UNIT, Operand::UNIT, true),
                    0,
                ),
            );
            st.for_each_use_mut(&mut |o| top(&stacks, o));
            if let StmtKind::Assign(d, _) = &mut st.kind {
                *d = fresh(f, *d, &mut stacks, &mut pushed);
            }
            f.blocks[b].stmts[i] = st;
        }
        let mut term = std::mem::replace(&mut f.blocks[b].term, Terminator::Unreachable);
        term.for_each_use_mut(&mut |o| top(&stacks, o));
        if let Terminator::Call { dest, .. } = &mut term {
            *dest = fresh(f, *dest, &mut stacks, &mut pushed);
        }
        let succs = distinct_succs(&term);
        f.blocks[b].term = term;
        for s in succs {
            for k in 0..f.blocks[s as usize].phis.len() {
                let orig = phi_orig[s as usize][k];
                if let Some(&t) = stacks[orig as usize].last() {
                    for (p, o) in f.blocks[s as usize].phis[k].args.iter_mut() {
                        if *p as usize == b {
                            *o = Operand::Local(t);
                        }
                    }
                }
            }
        }
        work.push(Ev::Exit(pushed));
        for &c in dt.children[b].iter().rev() {
            work.push(Ev::Enter(c));
        }
    }
    f.ssa = true;
    true
}

// ------------------------------------------------------------------ clean-up

fn resolve(map: &[Option<Operand>], o: &Operand) -> Operand {
    let mut cur = o.clone();
    for _ in 0..map.len() + 1 {
        match &cur {
            Operand::Local(l) => match &map[*l as usize] {
                Some(n) => cur = n.clone(),
                None => break,
            },
            _ => break,
        }
    }
    cur
}

/// Rewrite every operand of the function through `map` (a local -> operand replacement table).
pub fn replace_uses(f: &mut Func, map: &[Option<Operand>]) {
    for blk in &mut f.blocks {
        for p in &mut blk.phis {
            for (_, o) in &mut p.args {
                *o = resolve(map, o);
            }
        }
        for s in &mut blk.stmts {
            s.for_each_use_mut(&mut |o| *o = resolve(map, o));
        }
        blk.term.for_each_use_mut(&mut |o| *o = resolve(map, o));
    }
}

/// Count the reads of every local (statements, terminators and phi arguments).
pub fn use_counts(f: &Func) -> Vec<u32> {
    let mut n = vec![0u32; f.locals.len()];
    for b in f.reachable() {
        let blk = &f.blocks[b as usize];
        for p in &blk.phis {
            for (_, o) in &p.args {
                if let Operand::Local(l) = o {
                    n[*l as usize] += 1;
                }
            }
        }
        block_uses(blk, &mut |l| n[l as usize] += 1);
    }
    n
}

/// Remove phis whose arguments are all one value (or the phi itself) and phis nobody reads.
pub fn simplify_phis(f: &mut Func) -> bool {
    let mut any = false;
    loop {
        let mut map: Vec<Option<Operand>> = vec![None; f.locals.len()];
        let mut changed = false;
        for b in 0..f.blocks.len() {
            let mut keep = Vec::new();
            for p in std::mem::take(&mut f.blocks[b].phis) {
                let mut vals: Vec<Operand> = Vec::new();
                for (_, o) in &p.args {
                    let o = resolve(&map, o);
                    if o != Operand::Local(p.dest) && !vals.contains(&o) {
                        vals.push(o);
                    }
                }
                if vals.len() == 1 {
                    map[p.dest as usize] = Some(vals.remove(0));
                    changed = true;
                } else {
                    keep.push(p);
                }
            }
            f.blocks[b].phis = keep;
        }
        if changed {
            replace_uses(f, &map);
        }
        let uses = use_counts(f);
        for blk in &mut f.blocks {
            let before = blk.phis.len();
            blk.phis.retain(|p| uses[p.dest as usize] > 0);
            changed |= blk.phis.len() != before;
        }
        any |= changed;
        if !changed {
            return any;
        }
    }
}

/// Remove unreachable blocks of an SSA function, renumber, and drop phi arguments of dead edges.
pub fn remove_unreachable_blocks(f: &mut Func) {
    let n = f.blocks.len();
    let mut keep = vec![false; n];
    for b in f.reachable() {
        keep[b as usize] = true;
    }
    let mut new_id = vec![0 as BlockId; n];
    let mut next = 0;
    for (i, k) in keep.iter().enumerate() {
        if *k {
            new_id[i] = next;
            next += 1;
        }
    }
    let old = std::mem::take(&mut f.blocks);
    f.blocks = old
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, mut b)| {
            b.term.for_each_succ_mut(&mut |t| *t = new_id[*t as usize]);
            for p in &mut b.phis {
                p.args.retain(|(pr, _)| keep[*pr as usize]);
                p.args
                    .iter_mut()
                    .for_each(|(pr, _)| *pr = new_id[*pr as usize]);
            }
            b
        })
        .collect();
    // An edge can also disappear while both ends survive (a folded branch).
    let preds = f.predecessors();
    for (b, blk) in f.blocks.iter_mut().enumerate() {
        for p in &mut blk.phis {
            p.args.retain(|(pr, _)| preds[b].contains(pr));
        }
    }
}

// ----------------------------------------------------------------- destruction

/// Split the edges into phi blocks whose predecessor has several successors or ends in a call.
pub fn split_critical_edges(f: &mut Func) -> usize {
    let mut splits = 0;
    let preds = f.predecessors();
    for s in 0..f.blocks.len() {
        if f.blocks[s].phis.is_empty() {
            continue;
        }
        for &p in &preds[s] {
            let needs = distinct_succs(&f.blocks[p as usize].term).len() > 1
                || matches!(f.blocks[p as usize].term, Terminator::Call { .. });
            if !needs {
                continue;
            }
            let e = f.blocks.len() as BlockId;
            f.blocks
                .push(Block::new(vec![], Terminator::Goto(s as BlockId)));
            f.blocks[p as usize].term.for_each_succ_mut(&mut |t| {
                if *t as usize == s {
                    *t = e
                }
            });
            for ph in &mut f.blocks[s].phis {
                for (pr, _) in ph.args.iter_mut() {
                    if *pr == p {
                        *pr = e;
                    }
                }
            }
            splits += 1;
        }
    }
    splits
}

/// Order a parallel copy `{d_i <- s_i}` as a sequence of plain copies. A copy
/// whose destination is still needed as a source by another pending copy must
/// wait; if every pending destination is needed (a cycle, like the swap
/// `a, b = b, a`) one value is saved in a fresh temporary first.
pub fn sequentialize(f: &mut Func, copies: Vec<(LocalId, Operand)>) -> Vec<(LocalId, Operand)> {
    let mut pending: Vec<(LocalId, Operand)> = copies
        .into_iter()
        .filter(|(d, s)| *s != Operand::Local(*d))
        .collect();
    let mut out = Vec::new();
    while !pending.is_empty() {
        let free = (0..pending.len()).find(|&i| {
            let d = pending[i].0;
            !pending
                .iter()
                .enumerate()
                .any(|(j, (_, s))| j != i && *s == Operand::Local(d))
        });
        match free {
            Some(i) => out.push(pending.remove(i)),
            None => {
                let d = pending[0].0;
                let ty = f.locals[d as usize].ty.clone();
                let t = f.new_local(ty);
                out.push((t, Operand::Local(d)));
                for (_, s) in pending.iter_mut() {
                    if *s == Operand::Local(d) {
                        *s = Operand::Local(t);
                    }
                }
            }
        }
    }
    out
}

fn find(alias: &mut [LocalId], mut x: LocalId) -> LocalId {
    while alias[x as usize] != x {
        alias[x as usize] = alias[alias[x as usize] as usize];
        x = alias[x as usize];
    }
    x
}

/// Let `d = with_field(a, i, v)` reuse `a`'s storage when `a` dies there, so the
/// final `SetField` updates in place instead of copying the aggregate.
/// `a` must have exactly one use *and* not live across a loop the update sits in
/// (otherwise every iteration would update the same value).
fn alias_with_field(f: &mut Func) {
    let dt = DomTree::compute(f);
    let loops = dom::natural_loops(f, &dt);
    let mut uses = use_counts(f);
    let mut def_block: Vec<BlockId> = vec![0; f.locals.len()];
    for &b in &dt.rpo {
        let blk = &f.blocks[b as usize];
        blk.phis.iter().for_each(|p| def_block[p.dest as usize] = b);
        for s in &blk.stmts {
            if let Some(d) = s.def() {
                def_block[d as usize] = b;
            }
        }
        if let Some(d) = blk.term.def() {
            def_block[d as usize] = b;
        }
    }
    let mut alias: Vec<LocalId> = (0..f.locals.len() as LocalId).collect();
    for b in dt.preorder() {
        for s in &f.blocks[b as usize].stmts {
            let StmtKind::Assign(d, Rvalue::WithField(Operand::Local(a), _, v)) = &s.kind else {
                continue;
            };
            let ra = find(&mut alias, *a);
            if ra == *d || uses[ra as usize] != 1 || *v == Operand::Local(ra) {
                continue;
            }
            let db = def_block[ra as usize];
            if loops.iter().any(|l| l.contains(b) && !l.contains(db)) {
                continue;
            }
            alias[*d as usize] = ra;
            uses[ra as usize] = uses[*d as usize];
        }
    }
    rename_locals(f, &mut alias);
}

/// Convert back to the register form. Returns false if the function was not in SSA form.
pub fn from_ssa(f: &mut Func) -> bool {
    if !f.ssa {
        return false;
    }
    simplify_phis(f);
    alias_with_field(f);
    split_critical_edges(f);
    // Phis -> one parallel copy per incoming edge.
    let mut edge_copies: HashMap<BlockId, Vec<(LocalId, Operand)>> = HashMap::new();
    for blk in &mut f.blocks {
        for p in std::mem::take(&mut blk.phis) {
            for (pred, o) in p.args {
                edge_copies.entry(pred).or_default().push((p.dest, o));
            }
        }
    }
    let mut preds: Vec<BlockId> = edge_copies.keys().copied().collect();
    preds.sort_unstable();
    for p in preds {
        let copies = edge_copies.remove(&p).unwrap_or_default();
        let seq = sequentialize(f, copies);
        let (scope, span) = f.blocks[p as usize]
            .stmts
            .last()
            .map_or((0, Default::default()), |s| (s.scope, s.span));
        for (d, o) in seq {
            f.blocks[p as usize].stmts.push(Stmt::at(
                StmtKind::Assign(d, Rvalue::Use(o)),
                scope,
                span,
            ));
        }
    }
    // with_field -> SetField.
    for b in 0..f.blocks.len() {
        let stmts = std::mem::take(&mut f.blocks[b].stmts);
        let mut out = Vec::with_capacity(stmts.len());
        for s in stmts {
            match s.kind {
                StmtKind::Assign(d, Rvalue::WithField(a, i, v)) => {
                    if a != Operand::Local(d) {
                        out.push(Stmt::at(
                            StmtKind::Assign(d, Rvalue::Use(a)),
                            s.scope,
                            s.span,
                        ));
                    }
                    out.push(Stmt::at(StmtKind::SetField(d, i, v), s.scope, s.span));
                }
                _ => out.push(s),
            }
        }
        f.blocks[b].stmts = out;
    }
    f.ssa = false;
    fold_copies(f);
    coalesce_copies(f);
    super::dce::compact_locals(f);
    true
}

/// Chaitin-style copy coalescing. Phi elimination leaves one copy per phi argument;
/// when the two locals of a copy `d = s` never *interfere* (neither is assigned
/// while the other is live, the copy itself excepted) they can share one local and
/// the copy disappears. Copies that implement a swap or save a lost copy are
/// exactly the ones whose locals do interfere, so they stay.
fn coalesce_copies(f: &mut Func) {
    let live = liveness(f);
    let reach = f.reachable();
    let mut adj: Vec<HashSet<LocalId>> = vec![HashSet::new(); f.locals.len()];
    let interfere = |adj: &mut Vec<HashSet<LocalId>>, a: LocalId, b: LocalId| {
        if a != b {
            adj[a as usize].insert(b);
            adj[b as usize].insert(a);
        }
    };
    // Parameters are all assigned together on entry.
    for p in 0..f.params as LocalId {
        for q in 0..p {
            interfere(&mut adj, p, q);
        }
        for l in live.live_in[0].iter() {
            interfere(&mut adj, p, l);
        }
    }
    let mut copies: Vec<(LocalId, LocalId)> = Vec::new();
    for &b in &reach {
        let blk = &f.blocks[b as usize];
        let mut now = live.live_out[b as usize].clone();
        if let Some(d) = blk.term.def() {
            for l in now.iter() {
                interfere(&mut adj, d, l);
            }
        }
        step_back_term(&blk.term, &mut now);
        for s in blk.stmts.iter().rev() {
            if let Some(d) = s.def() {
                let source = match &s.kind {
                    StmtKind::Assign(_, Rvalue::Use(Operand::Local(x))) => Some(*x),
                    _ => None,
                };
                if let Some(x) = source {
                    copies.push((d, x));
                }
                for l in now.iter() {
                    if Some(l) != source {
                        interfere(&mut adj, d, l);
                    }
                }
            }
            step_back_stmt(s, &mut now);
        }
    }
    let mut alias: Vec<LocalId> = (0..f.locals.len() as LocalId).collect();
    let params = f.params as LocalId;
    // `copies` was collected backwards; coalesce in program order.
    for &(d, s) in copies.iter().rev() {
        let (mut keep, mut gone) = (find(&mut alias, d), find(&mut alias, s));
        if keep == gone
            || adj[keep as usize].contains(&gone)
            || f.locals[keep as usize].ty != f.locals[gone as usize].ty
        {
            continue;
        }
        // A parameter must keep its index.
        if gone < params {
            std::mem::swap(&mut keep, &mut gone);
        }
        if gone < params {
            continue;
        }
        alias[gone as usize] = keep;
        for n in std::mem::take(&mut adj[gone as usize]) {
            adj[n as usize].remove(&gone);
            interfere(&mut adj, keep, n);
        }
        if f.locals[keep as usize].name.is_none() {
            f.locals[keep as usize].name = f.locals[gone as usize].name.clone();
        }
    }
    rename_locals(f, &mut alias);
    for blk in &mut f.blocks {
        blk.stmts.retain(
            |s| !matches!(&s.kind, StmtKind::Assign(d, Rvalue::Use(Operand::Local(x))) if d == x),
        );
    }
}

/// Replace every definition and use of a local by its representative in `alias`.
fn rename_locals(f: &mut Func, alias: &mut [LocalId]) {
    let rename = |alias: &mut [LocalId], l: &mut LocalId| *l = find(alias, *l);
    for blk in &mut f.blocks {
        for p in &mut blk.phis {
            rename(alias, &mut p.dest);
            for (_, o) in &mut p.args {
                if let Operand::Local(l) = o {
                    rename(alias, l)
                }
            }
        }
        for s in &mut blk.stmts {
            if let StmtKind::Assign(d, _) | StmtKind::SetField(d, _, _) = &mut s.kind {
                rename(alias, d);
            }
            s.for_each_use_mut(&mut |o| {
                if let Operand::Local(l) = o {
                    rename(alias, l)
                }
            });
        }
        blk.term.for_each_use_mut(&mut |o| {
            if let Operand::Local(l) = o {
                rename(alias, l)
            }
        });
        if let Terminator::Call { dest, .. } = &mut blk.term {
            rename(alias, dest);
        }
    }
}

/// `s = rv; ...; d = s` becomes `d = rv; ...` when `s` has no other use and `d`
/// is untouched in between: removes the extra move per loop iteration that phi
/// elimination would otherwise leave behind.
fn fold_copies(f: &mut Func) {
    let mut uses = use_counts(f);
    let mut defs = vec![0u32; f.locals.len()];
    for b in f.reachable() {
        let blk = &f.blocks[b as usize];
        blk.stmts
            .iter()
            .filter_map(|s| s.def())
            .for_each(|d| defs[d as usize] += 1);
        if let Some(d) = blk.term.def() {
            defs[d as usize] += 1;
        }
    }
    for b in f.reachable() {
        let mut k = 0;
        while k < f.blocks[b as usize].stmts.len() {
            let stmts = &f.blocks[b as usize].stmts;
            let StmtKind::Assign(d, Rvalue::Use(Operand::Local(s))) = &stmts[k].kind else {
                k += 1;
                continue;
            };
            let (d, s) = (*d, *s);
            if s == d || (s as usize) < f.params || uses[s as usize] != 1 || defs[s as usize] != 1 {
                k += 1;
                continue;
            }
            // The definition of `s` must precede the copy in this block, with `d` untouched in between.
            let mut j = k;
            let mut ok = false;
            while j > 0 {
                j -= 1;
                let st = &stmts[j];
                if st.def() == Some(s) {
                    ok = matches!(st.kind, StmtKind::Assign(..));
                    break;
                }
                let mut touches = st.def() == Some(d);
                st.for_each_use(&mut |o| touches |= *o == Operand::Local(d));
                if touches {
                    break;
                }
            }
            if !ok {
                k += 1;
                continue;
            }
            let blk = &mut f.blocks[b as usize];
            if let StmtKind::Assign(def, _) = &mut blk.stmts[j].kind {
                *def = d;
            }
            blk.stmts.remove(k);
            uses[s as usize] = 0;
            defs[s as usize] = 0;
            defs[d as usize] = defs[d as usize].max(1);
        }
    }
}
