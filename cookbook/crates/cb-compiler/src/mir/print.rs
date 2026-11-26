//! Textual MIR (`dump-mir`) and Graphviz output (`dump-cfg`).

use super::*;
use std::fmt::{self, Write};

pub fn op(o: &Operand) -> String {
    match o {
        Operand::Local(l) => format!("_{l}"),
        Operand::Const(Const::Int(i)) => i.to_string(),
        Operand::Const(Const::Bool(b)) => b.to_string(),
        Operand::Const(Const::Unit) => "()".into(),
        Operand::Const(Const::Str(s)) => format!("{:?}", &**s),
        Operand::Const(Const::Fn(f)) => format!("fn#{f}"),
    }
}

fn ops(xs: &[Operand]) -> String {
    xs.iter().map(op).collect::<Vec<_>>().join(", ")
}

pub fn rvalue(rv: &Rvalue) -> String {
    match rv {
        Rvalue::Use(a) => op(a),
        Rvalue::Unary(o, a) => format!("{}{}", if *o == UnOp::Neg { "-" } else { "!" }, op(a)),
        Rvalue::Binary(o, a, b) => format!("{} {} {}", op(a), o.symbol(), op(b)),
        Rvalue::Agg(k, xs) => match k {
            AggKind::Tuple => format!("tuple({})", ops(xs)),
            AggKind::Array => format!("[{}]", ops(xs)),
            AggKind::Struct(d) => format!("struct#{}({})", d.0, ops(xs)),
            AggKind::Variant(d, v) => format!("enum#{}::{v}({})", d.0, ops(xs)),
        },
        Rvalue::Field(a, i) => format!("{}.{i}", op(a)),
        Rvalue::Index(a, i, checked) => format!(
            "{}[{}]{}",
            op(a),
            op(i),
            if *checked { "" } else { " !nocheck" }
        ),
        Rvalue::Len(a) => format!("len({})", op(a)),
        Rvalue::Tag(a) => format!("tag({})", op(a)),
        Rvalue::Builtin(b, xs) => format!("{}({})", b.name(), ops(xs)),
        Rvalue::Cast(a) => format!("{} as int", op(a)),
        Rvalue::WithField(a, i, v) => format!("with_field({}, .{i}, {})", op(a), op(v)),
    }
}

pub fn stmt(s: &Stmt) -> String {
    match &s.kind {
        StmtKind::Assign(d, rv) => format!("_{d} = {}", rvalue(rv)),
        StmtKind::SetField(d, i, v) => format!("_{d}.{i} = {}", op(v)),
        StmtKind::SetIndex(a, i, v, c) => format!(
            "{}[{}] = {}{}",
            op(a),
            op(i),
            op(v),
            if *c { "" } else { " !nocheck" }
        ),
    }
}

pub fn phi(p: &Phi) -> String {
    let args: Vec<String> = p
        .args
        .iter()
        .map(|(b, o)| format!("bb{b}: {}", op(o)))
        .collect();
    format!("_{} = phi [{}]", p.dest, args.join(", "))
}

pub fn term(t: &Terminator) -> String {
    match t {
        Terminator::Goto(b) => format!("goto bb{b}"),
        Terminator::Branch {
            cond,
            then_bb,
            else_bb,
        } => format!("branch {} -> bb{then_bb}, bb{else_bb}", op(cond)),
        Terminator::Switch {
            value,
            cases,
            default,
        } => {
            let cs: Vec<String> = cases.iter().map(|(v, b)| format!("{v} => bb{b}")).collect();
            format!("switch {} [{}, _ => bb{default}]", op(value), cs.join(", "))
        }
        Terminator::Call {
            callee,
            args,
            dest,
            target,
            ..
        } => {
            let c = match callee {
                Callee::Direct(f) => format!("fn#{f}"),
                Callee::Indirect(o) => format!("*{}", op(o)),
            };
            format!("_{dest} = call {c}({}) -> bb{target}", ops(args))
        }
        Terminator::Return(o) => format!("return {}", op(o)),
        Terminator::Unreachable => "unreachable".into(),
    }
}

impl fmt::Display for Func {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ps: Vec<String> = (0..self.params)
            .map(|i| format!("_{i}: {}", self.locals[i].ty))
            .collect();
        writeln!(
            f,
            "fn {}({}) -> {} {{{}",
            self.inst_name,
            ps.join(", "),
            self.ret,
            if self.ssa { "  // ssa" } else { "" }
        )?;
        for (i, l) in self.locals.iter().enumerate().skip(self.params) {
            match &l.name {
                Some(n) => writeln!(f, "    let _{i}: {};  // {n}", l.ty)?,
                None => writeln!(f, "    let _{i}: {};", l.ty)?,
            }
        }
        for (i, b) in self.blocks.iter().enumerate() {
            writeln!(f, "  bb{i}:")?;
            for p in &b.phis {
                writeln!(f, "    {}", phi(p))?;
            }
            for s in &b.stmts {
                writeln!(f, "    {}", stmt(s))?;
            }
            writeln!(f, "    {}", term(&b.term))?;
        }
        write!(f, "}}")
    }
}

impl fmt::Display for MirProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "// monomorphisation: {} function instances",
            self.stats.total_instances
        )?;
        for (n, c) in &self.stats.generic_instances {
            writeln!(f, "//   generic `{n}`: {c} instance(s)")?;
        }
        for (i, func) in self.funcs.iter().enumerate() {
            writeln!(
                f,
                "// fn#{i}{}",
                if i as u32 == self.main { " (main)" } else { "" }
            )?;
            writeln!(f, "{func}")?;
        }
        Ok(())
    }
}

/// Graphviz DOT for every function (one `digraph` per function, concatenated).
pub fn dot(p: &MirProgram) -> String {
    let mut s = String::new();
    for (fi, func) in p.funcs.iter().enumerate() {
        let _ = writeln!(
            s,
            "digraph \"fn{fi}_{}\" {{\n  node [shape=box fontname=monospace];",
            func.inst_name.replace('"', "")
        );
        for b in func.reachable() {
            let blk = &func.blocks[b as usize];
            let mut label = format!("bb{b}\\l");
            for p in &blk.phis {
                label.push_str(&esc(&phi(p)));
                label.push_str("\\l");
            }
            for st in &blk.stmts {
                label.push_str(&esc(&stmt(st)));
                label.push_str("\\l");
            }
            label.push_str(&esc(&term(&blk.term)));
            label.push_str("\\l");
            let _ = writeln!(s, "  bb{b} [label=\"{label}\"];");
            let succs = blk.term.successors();
            for (i, t) in succs.iter().enumerate() {
                let lbl = match (&blk.term, i) {
                    (Terminator::Branch { .. }, 0) => " [label=\"true\"]",
                    (Terminator::Branch { .. }, _) => " [label=\"false\"]",
                    _ => "",
                };
                let _ = writeln!(s, "  bb{b} -> bb{t}{lbl};");
            }
        }
        s.push_str("}\n");
    }
    s
}

fn esc(x: &str) -> String {
    x.replace('\\', "\\\\").replace('"', "\\\"")
}
