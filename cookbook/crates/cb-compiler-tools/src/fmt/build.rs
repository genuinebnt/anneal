//! AST to [`Doc`], with comment re-attachment.
//!
//! COMMENT RULES (see docs/FORMATTER.md): the builder walks the tree strictly in
//! source order and keeps a cursor into the comment side table.
//! * LEADING: before an element (item, statement, field, arm, argument) every
//!   comment that ends before it is printed on its own line(s) above it.
//! * TRAILING: a comment starting on the same line where an element ends, and
//!   before the next element or the closing delimiter, stays on that line.
//! * DANGLING: comments left before a closing delimiter (including inside an empty
//!   block) are printed last, inside the delimiters.
//! Because the cursor only moves forward and everything left over is flushed at
//! the end of the module, no comment can be lost or reordered.

use super::INDENT;
use super::doc::*;
use crate::analysis::source::Span;
use crate::analysis::syntax::ast::*;
use crate::analysis::syntax::{Comment, CommentKind};

pub fn module_doc(src: &str, m: &Module, comments: &[Comment]) -> Doc {
    let mut b = Builder {
        src,
        comments,
        cur: 0,
    };
    let items: Vec<&Item> = m.items.iter().collect();
    let force = |i: usize| {
        !(matches!(items[i].kind, ItemKind::Const(_))
            && matches!(items[i - 1].kind, ItemKind::Const(_)))
    };
    let (mut docs, last) = b.each(
        &items,
        |i| i.span,
        src.len(),
        true,
        &force,
        |b, i, _| b.item(i),
    );
    docs.extend(b.dangling(src.len(), last, true));
    if docs.is_empty() {
        return Doc::Nil;
    }
    concat(vec![join(&Doc::HardLine, docs), Doc::HardLine])
}

struct Builder<'a> {
    src: &'a str,
    comments: &'a [Comment],
    cur: usize,
}

enum Stmtish<'a> {
    Stmt(&'a Stmt),
    Tail(&'a Expr),
}

fn is_line_comment(c: &Comment) -> bool {
    c.kind != CommentKind::Block
}

fn comment_doc(c: &Comment) -> Doc {
    let t = if is_line_comment(c) {
        c.text.trim_end()
    } else {
        c.text.as_str()
    };
    text(t)
}

fn with_comma(d: Doc, last: bool, force: bool) -> Doc {
    let comma = if !last || force {
        text(",")
    } else {
        if_break(text(","), Doc::Nil)
    };
    concat(vec![d, comma])
}

/// Operator precedence classes (matching the parser's binding powers).
fn prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 2,
        BinOp::And => 4,
        BinOp::Add | BinOp::Sub => 8,
        BinOp::Mul | BinOp::Div | BinOp::Rem => 10,
        _ => 6,
    }
}

/// Expressions that can break internally, so `let x = <expr>` keeps them on the `=` line.
fn hugs(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Struct { .. }
            | ExprKind::Array(_)
            | ExprKind::Tuple(_)
            | ExprKind::Block(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Paren(_)
    )
}

fn delimited(open: &str, close: &str, docs: Vec<Doc>, soft: bool) -> Doc {
    if docs.is_empty() {
        return text(format!("{open}{close}"));
    }
    let sp = if soft { Doc::SoftLine } else { Doc::Line };
    group(concat(vec![
        text(open),
        nest(INDENT, concat(vec![sp.clone(), join(&Doc::Line, docs)])),
        sp,
        text(close),
    ]))
}

fn generics(g: &[Ident]) -> String {
    if g.is_empty() {
        String::new()
    } else {
        format!(
            "<{}>",
            g.iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn ty_str(t: &Type) -> String {
    match &t.kind {
        TypeKind::Path { name, args } => {
            if args.is_empty() {
                name.name.clone()
            } else {
                format!(
                    "{}<{}>",
                    name.name,
                    args.iter().map(ty_str).collect::<Vec<_>>().join(", ")
                )
            }
        }
        TypeKind::Array(e) => format!("[{}]", ty_str(e)),
        TypeKind::Tuple(ts) => {
            format!("({})", ts.iter().map(ty_str).collect::<Vec<_>>().join(", "))
        }
        TypeKind::Fn { params, ret } => {
            let mut s = format!(
                "fn({})",
                params.iter().map(ty_str).collect::<Vec<_>>().join(", ")
            );
            if let Some(r) = ret {
                s.push_str(" -> ");
                s.push_str(&ty_str(r));
            }
            s
        }
        TypeKind::Never => "!".into(),
        TypeKind::Error => String::new(),
    }
}

fn path_str(p: &Path) -> String {
    p.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn pat_list(ps: &[Pattern]) -> String {
    ps.iter().map(pat_str).collect::<Vec<_>>().join(", ")
}

fn pat_str(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Wild => "_".into(),
        PatKind::Binding { mutable, name } => {
            format!("{}{}", if *mutable { "mut " } else { "" }, name.name)
        }
        PatKind::Lit(Lit::Unit) => "()".into(),
        PatKind::Lit(_) => String::new(), // replaced by source text in `pat` (needs the source)
        PatKind::Tuple(ps) if ps.len() == 1 => format!("({},)", pat_str(&ps[0])),
        PatKind::Tuple(ps) => format!("({})", pat_list(ps)),
        PatKind::Or(alts) => alts
            .iter()
            .map(|a| {
                if matches!(a.kind, PatKind::Or(_)) {
                    format!("({})", pat_str(a))
                } else {
                    pat_str(a)
                }
            })
            .collect::<Vec<_>>()
            .join(" | "),
        PatKind::Adt { path, fields } => match fields {
            PatFields::Unit => path_str(path),
            PatFields::Tuple(ps) => format!("{}({})", path_str(path), pat_list(ps)),
            PatFields::Struct { fields, rest } => {
                let mut parts: Vec<String> = fields.iter().map(field_pat_str).collect();
                if *rest {
                    parts.push("..".into());
                }
                if parts.is_empty() {
                    format!("{} {{}}", path_str(path))
                } else {
                    format!("{} {{ {} }}", path_str(path), parts.join(", "))
                }
            }
        },
        PatKind::Error => String::new(),
    }
}

fn field_pat_str(f: &FieldPat) -> String {
    match &f.pat.kind {
        PatKind::Binding { mutable, name }
            if f.pat.span == f.name.span && name.name == f.name.name =>
        {
            format!("{}{}", if *mutable { "mut " } else { "" }, f.name.name)
        }
        _ => format!("{}: {}", f.name.name, pat_str(&f.pat)),
    }
}

impl Builder<'_> {
    // ------------------------------------------------------------ comments

    fn newlines(&self, a: usize, b: usize) -> usize {
        if a >= b {
            0
        } else {
            self.src.as_bytes()[a..b.min(self.src.len())]
                .iter()
                .filter(|&&c| c == b'\n')
                .count()
        }
    }

    fn has_comments_in(&self, a: usize, b: usize) -> bool {
        self.comments[self.cur..]
            .iter()
            .take_while(|c| (c.span.start as usize) < b)
            .any(|c| c.span.start as usize >= a)
    }

    /// Comments before `pos`, each on its own line, plus the blank line (if any) before the element.
    fn lead(&mut self, pos: usize, prev: Option<usize>, keep: bool, force: bool) -> Doc {
        let mut parts = Vec::new();
        let mut last = prev.unwrap_or(0);
        let mut first = true;
        while let Some(c) = self.comments.get(self.cur) {
            if c.span.end as usize > pos {
                break;
            }
            self.cur += 1;
            let start = c.span.start as usize;
            if prev.is_some() && ((first && force) || (keep && self.newlines(last, start) >= 2)) {
                parts.push(Doc::HardLine);
            }
            parts.push(comment_doc(c));
            parts.push(Doc::HardLine);
            last = c.span.end as usize;
            first = false;
        }
        if prev.is_some() && ((first && force) || (keep && self.newlines(last, pos) >= 2)) {
            parts.push(Doc::HardLine);
        }
        concat(parts)
    }

    /// Comments that start on the line where an element ended (and before `limit`).
    fn trail(&mut self, end: usize, limit: usize) -> (Doc, usize) {
        let mut parts = Vec::new();
        let mut at = end;
        let mut last_end = end;
        while let Some(c) = self.comments.get(self.cur) {
            let start = c.span.start as usize;
            if start < end
                || start >= limit
                || self.src.get(at..start).is_none_or(|s| s.contains('\n'))
            {
                break;
            }
            self.cur += 1;
            parts.push(text(" "));
            parts.push(comment_doc(c));
            if is_line_comment(c) {
                parts.push(Doc::BreakParent);
            }
            at = c.span.end as usize;
            last_end = at;
        }
        (concat(parts), last_end)
    }

    /// Comments left before a closing delimiter, one doc each.
    fn dangling(&mut self, close: usize, prev: Option<usize>, keep: bool) -> Vec<Doc> {
        let mut out = Vec::new();
        let mut last = prev;
        while let Some(c) = self.comments.get(self.cur) {
            if c.span.end as usize > close {
                break;
            }
            self.cur += 1;
            let mut parts = Vec::new();
            if let Some(l) = last
                && keep
                && self.newlines(l, c.span.start as usize) >= 2
            {
                parts.push(Doc::HardLine);
            }
            parts.push(comment_doc(c));
            if is_line_comment(c) {
                parts.push(Doc::BreakParent);
            }
            out.push(concat(parts));
            last = Some(c.span.end as usize);
        }
        out
    }

    /// Build one doc per element, wrapping each with its leading and trailing comments.
    /// Returns the docs and the end offset of the last element (or its trailing comment).
    fn each<T>(
        &mut self,
        items: &[T],
        span: impl Fn(&T) -> Span,
        limit: usize,
        keep: bool,
        force: &dyn Fn(usize) -> bool,
        mut f: impl FnMut(&mut Self, &T, bool) -> Doc,
    ) -> (Vec<Doc>, Option<usize>) {
        let mut docs = Vec::new();
        let mut prev: Option<usize> = None;
        for (i, item) in items.iter().enumerate() {
            let sp = span(item);
            let last = i + 1 == items.len();
            let lead = self.lead(sp.start as usize, prev, keep, i > 0 && force(i));
            let body = f(self, item, last);
            let lim = if last {
                limit
            } else {
                span(&items[i + 1]).start as usize
            };
            let (tr, end) = self.trail(sp.end as usize, lim);
            docs.push(concat(vec![lead, body, tr]));
            prev = Some(end.max(sp.end as usize));
        }
        (docs, prev)
    }

    fn no_force(_: usize) -> bool {
        false
    }

    // ------------------------------------------------------------ items

    fn item(&mut self, i: &Item) -> Doc {
        match &i.kind {
            ItemKind::Fn(f) => {
                let close = f.body.span.start as usize;
                let (params, _) = self.each(
                    &f.params,
                    |p| p.span,
                    close,
                    false,
                    &Self::no_force,
                    |b, p, last| with_comma(text(b.param(p)), last, false),
                );
                let mut head = vec![
                    text(format!("fn {}{}", f.name.name, generics(&f.generics))),
                    delimited("(", ")", params, true),
                ];
                if let Some(r) = &f.ret {
                    head.push(text(format!(" -> {}", ty_str(r))));
                }
                head.push(text(" "));
                head.push(self.block(&f.body, false));
                concat(head)
            }
            ItemKind::Struct(s) => {
                let close = i.span.end as usize;
                let (fields, last) = self.each(
                    &s.fields,
                    |f| f.span,
                    close,
                    true,
                    &Self::no_force,
                    |_, f, last| {
                        with_comma(
                            text(format!("{}: {}", f.name.name, ty_str(&f.ty))),
                            last,
                            false,
                        )
                    },
                );
                let mut fields = fields;
                fields.extend(self.dangling(close, last, true));
                concat(vec![
                    text(format!("struct {}{} ", s.name.name, generics(&s.generics))),
                    delimited("{", "}", fields, false),
                ])
            }
            ItemKind::Enum(e) => {
                let close = i.span.end as usize;
                let (vs, last) = self.each(
                    &e.variants,
                    |v| v.span,
                    close,
                    true,
                    &Self::no_force,
                    |_, v, last| with_comma(text(variant_str(v)), last, false),
                );
                let mut vs = vs;
                vs.extend(self.dangling(close, last, true));
                concat(vec![
                    text(format!("enum {}{} ", e.name.name, generics(&e.generics))),
                    delimited("{", "}", vs, false),
                ])
            }
            ItemKind::Const(c) => {
                let v = self.expr(&c.value);
                concat(vec![
                    text(format!("const {}: {} =", c.name.name, ty_str(&c.ty))),
                    self.after_eq(&c.value, v),
                    text(";"),
                ])
            }
            ItemKind::Error => text(i.span.slice(self.src)),
        }
    }

    fn param(&self, p: &Param) -> String {
        format!(
            "{}{}: {}",
            if p.mutable { "mut " } else { "" },
            p.name.name,
            ty_str(&p.ty)
        )
    }

    /// ` <value>` on the same line when it can break itself, else break after `=` if too wide.
    fn after_eq(&self, e: &Expr, v: Doc) -> Doc {
        if hugs(e) {
            concat(vec![text(" "), v])
        } else {
            group(nest(INDENT, concat(vec![Doc::Line, v])))
        }
    }

    // ------------------------------------------------------------ blocks

    fn block(&mut self, b: &Block, inline_ok: bool) -> Doc {
        let close = (b.span.end as usize).saturating_sub(1);
        let open = b.span.start as usize;
        if inline_ok && b.stmts.is_empty() && b.tail.is_some() && !self.has_comments_in(open, close)
        {
            // A comment from outside the block may still get flushed while building the
            // tail; that must not decide the layout (it would differ on the next run),
            // so build the tail on a scratch cursor and fall back if it consumed any.
            let saved = self.cur;
            let tail = b.tail.as_ref().map(|t| self.expr(t)).unwrap_or(Doc::Nil);
            if self.cur == saved {
                return group(concat(vec![
                    text("{"),
                    nest(INDENT, concat(vec![Doc::Line, tail])),
                    Doc::Line,
                    text("}"),
                ]));
            }
            self.cur = saved;
        }
        let mut elems: Vec<Stmtish> = b.stmts.iter().map(Stmtish::Stmt).collect();
        if let Some(t) = &b.tail {
            elems.push(Stmtish::Tail(t));
        }
        let span = |s: &Stmtish| match s {
            Stmtish::Stmt(s) => s.span,
            Stmtish::Tail(e) => e.span,
        };
        let (mut docs, last) =
            self.each(
                &elems,
                span,
                close,
                true,
                &Self::no_force,
                |bd, s, _| match s {
                    Stmtish::Stmt(s) => bd.stmt(s),
                    Stmtish::Tail(e) => bd.expr(e),
                },
            );
        docs.extend(self.dangling(close, last, true));
        if docs.is_empty() {
            return text("{}");
        }
        concat(vec![
            text("{"),
            nest(
                INDENT,
                concat(vec![Doc::HardLine, join(&Doc::HardLine, docs)]),
            ),
            Doc::HardLine,
            text("}"),
        ])
    }

    fn stmt(&mut self, s: &Stmt) -> Doc {
        match &s.kind {
            StmtKind::Let { pat, ty, init } => {
                let mut parts = vec![text(format!("let {}", self.pat(pat)))];
                if let Some(t) = ty {
                    parts.push(text(format!(": {}", ty_str(t))));
                }
                if let Some(e) = init {
                    let v = self.expr(e);
                    parts.push(text(" ="));
                    parts.push(self.after_eq(e, v));
                }
                parts.push(text(";"));
                concat(parts)
            }
            StmtKind::Expr { expr, semi } => {
                let d = self.expr(expr);
                if *semi { concat(vec![d, text(";")]) } else { d }
            }
            StmtKind::Error => text(s.span.slice(self.src)),
        }
    }

    // ------------------------------------------------------------ patterns

    /// Patterns are printed as strings; literals come from the source to keep `1_000` and escapes.
    fn pat(&self, p: &Pattern) -> String {
        match &p.kind {
            PatKind::Lit(Lit::Unit) => "()".into(),
            PatKind::Lit(_) => p.span.slice(self.src).to_string(),
            PatKind::Tuple(ps) => {
                let inner: Vec<String> = ps.iter().map(|p| self.pat(p)).collect();
                if inner.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            PatKind::Or(alts) => alts
                .iter()
                .map(|a| {
                    if matches!(a.kind, PatKind::Or(_)) {
                        format!("({})", self.pat(a))
                    } else {
                        self.pat(a)
                    }
                })
                .collect::<Vec<_>>()
                .join(" | "),
            PatKind::Adt { path, fields } => match fields {
                PatFields::Unit => path_str(path),
                PatFields::Tuple(ps) => format!(
                    "{}({})",
                    path_str(path),
                    ps.iter()
                        .map(|p| self.pat(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                PatFields::Struct { fields, rest } => {
                    let mut parts: Vec<String> = fields.iter().map(|f| self.field_pat(f)).collect();
                    if *rest {
                        parts.push("..".into());
                    }
                    if parts.is_empty() {
                        format!("{} {{}}", path_str(path))
                    } else {
                        format!("{} {{ {} }}", path_str(path), parts.join(", "))
                    }
                }
            },
            _ => pat_str(p),
        }
    }

    fn field_pat(&self, f: &FieldPat) -> String {
        match &f.pat.kind {
            PatKind::Binding { .. } if f.pat.span == f.name.span => field_pat_str(f),
            _ => format!("{}: {}", f.name.name, self.pat(&f.pat)),
        }
    }

    // ------------------------------------------------------------ expressions

    fn expr(&mut self, e: &Expr) -> Doc {
        match &e.kind {
            ExprKind::Lit(Lit::Unit) => text("()"),
            ExprKind::Lit(_) => text(e.span.slice(self.src)),
            ExprKind::Path(p) => text(path_str(p)),
            ExprKind::Unary(op, a) => {
                let d = self.expr(a);
                concat(vec![text(if *op == UnOp::Neg { "-" } else { "!" }), d])
            }
            ExprKind::Binary(op, ..) => self.binary(e, prec(*op)),
            ExprKind::Assign { op, target, value } => {
                let t = self.expr(target);
                let v = self.expr(value);
                let sym = op
                    .map(|o| format!("{}=", o.symbol()))
                    .unwrap_or_else(|| "=".into());
                concat(vec![t, text(format!(" {sym}")), self.after_eq(value, v)])
            }
            ExprKind::Call { callee, args } => {
                let c = self.expr(callee);
                concat(vec![c, self.args(args, e.span, "(", ")")])
            }
            ExprKind::MethodCall { recv, method, args } => {
                let r = self.expr(recv);
                concat(vec![
                    r,
                    text(format!(".{}", method.name)),
                    self.args(args, e.span, "(", ")"),
                ])
            }
            ExprKind::Field(a, name) => {
                let d = self.expr(a);
                concat(vec![d, text(format!(".{}", name.name))])
            }
            ExprKind::TupleField(a, _, sp) => {
                let d = self.expr(a);
                concat(vec![d, text(format!(".{}", sp.slice(self.src)))])
            }
            ExprKind::Index(a, i) => {
                let (a, i) = (self.expr(a), self.expr(i));
                concat(vec![a, text("["), i, text("]")])
            }
            ExprKind::Struct { path, fields } => {
                let close = (e.span.end as usize).saturating_sub(1);
                let (mut docs, last) = self.each(
                    fields,
                    |f| f.span,
                    close,
                    false,
                    &Self::no_force,
                    |b, f, last| {
                        let v = b.expr(&f.value);
                        with_comma(
                            concat(vec![text(format!("{}: ", f.name.name)), v]),
                            last,
                            false,
                        )
                    },
                );
                docs.extend(self.dangling(close, last, false));
                concat(vec![
                    text(format!("{} ", path_str(path))),
                    delimited("{", "}", docs, false),
                ])
            }
            ExprKind::Tuple(xs) => {
                let one = xs.len() == 1;
                self.list(xs, e.span, "(", ")", one)
            }
            ExprKind::Array(xs) => self.list(xs, e.span, "[", "]", false),
            ExprKind::If { cond, then, els } => {
                let c = self.expr(cond);
                let mut parts = vec![text("if "), c, text(" "), self.block(then, true)];
                if let Some(el) = els {
                    parts.push(text(" else "));
                    parts.push(match &el.kind {
                        ExprKind::Block(b) => self.block(b, true),
                        _ => self.expr(el),
                    });
                }
                concat(parts)
            }
            ExprKind::Match { scrut, arms } => self.match_expr(e, scrut, arms),
            ExprKind::Block(b) => self.block(b, false),
            ExprKind::While { cond, body } => {
                let c = self.expr(cond);
                concat(vec![text("while "), c, text(" "), self.block(body, false)])
            }
            ExprKind::Loop(b) => concat(vec![text("loop "), self.block(b, false)]),
            ExprKind::Break(v) => self.keyword_value("break", v),
            ExprKind::Return(v) => self.keyword_value("return", v),
            ExprKind::Continue => text("continue"),
            ExprKind::Cast(a, t) => {
                let d = self.expr(a);
                concat(vec![d, text(format!(" as {}", ty_str(t)))])
            }
            ExprKind::Paren(a) => {
                let d = self.expr(a);
                concat(vec![text("("), d, text(")")])
            }
            ExprKind::Error => text(e.span.slice(self.src)),
        }
    }

    fn keyword_value(&mut self, kw: &str, v: &Option<Box<Expr>>) -> Doc {
        match v {
            None => text(kw),
            Some(x) => {
                let d = self.expr(x);
                concat(vec![text(format!("{kw} ")), d])
            }
        }
    }

    /// A left-leaning chain of same-precedence operators becomes one group so it
    /// breaks before every operator rather than nesting deeper at each step.
    fn binary(&mut self, e: &Expr, p: u8) -> Doc {
        let mut operands: Vec<(Option<BinOp>, &Expr)> = Vec::new();
        fn flatten<'e>(e: &'e Expr, p: u8, out: &mut Vec<(Option<BinOp>, &'e Expr)>) {
            if let ExprKind::Binary(op, l, r) = &e.kind
                && prec(*op) == p
            {
                flatten(l, p, out);
                out.push((Some(*op), r));
            } else {
                out.push((None, e));
            }
        }
        flatten(e, p, &mut operands);
        let mut parts = Vec::new();
        let mut rest = Vec::new();
        for (op, x) in operands {
            let d = self.expr(x);
            match op {
                None => parts.push(d),
                Some(op) => rest.push(concat(vec![
                    Doc::Line,
                    text(format!("{} ", op.symbol())),
                    d,
                ])),
            }
        }
        parts.push(nest(INDENT, concat(rest)));
        group(concat(parts))
    }

    fn args(&mut self, args: &[Expr], whole: Span, open: &str, close: &str) -> Doc {
        self.list(args, whole, open, close, false)
    }

    fn list(
        &mut self,
        xs: &[Expr],
        whole: Span,
        open: &str,
        close: &str,
        force_comma: bool,
    ) -> Doc {
        let end = (whole.end as usize).saturating_sub(1);
        let (mut docs, last) = self.each(
            xs,
            |x| x.span,
            end,
            false,
            &Self::no_force,
            |b, x, last| {
                let d = b.expr(x);
                with_comma(d, last, force_comma)
            },
        );
        docs.extend(self.dangling(end, last, false));
        delimited(open, close, docs, true)
    }

    fn match_expr(&mut self, e: &Expr, scrut: &Expr, arms: &[MatchArm]) -> Doc {
        let s = self.expr(scrut);
        let close = (e.span.end as usize).saturating_sub(1);
        let (mut docs, last) = self.each(
            arms,
            |a| a.span,
            close,
            true,
            &Self::no_force,
            |b, a, _| b.arm(a),
        );
        docs.extend(self.dangling(close, last, true));
        let head = concat(vec![text("match "), s, text(" {")]);
        if docs.is_empty() {
            return concat(vec![head, text("}")]);
        }
        concat(vec![
            head,
            nest(
                INDENT,
                concat(vec![Doc::HardLine, join(&Doc::HardLine, docs)]),
            ),
            Doc::HardLine,
            text("}"),
        ])
    }

    fn arm(&mut self, a: &MatchArm) -> Doc {
        let mut head = self.pat(&a.pat);
        let guard = a.guard.as_ref().map(|g| self.expr(g));
        let mut parts = Vec::new();
        if guard.is_some() {
            head.push_str(" if ");
        }
        parts.push(text(head));
        parts.extend(guard);
        parts.push(text(" =>"));
        match &a.body.kind {
            ExprKind::Block(b) => {
                parts.push(text(" "));
                parts.push(self.block(b, true));
            }
            _ => {
                let body = self.expr(&a.body);
                if hugs(&a.body) {
                    parts.push(text(" "));
                    parts.push(body);
                } else {
                    parts.push(group(nest(INDENT, concat(vec![Doc::Line, body]))));
                }
                parts.push(text(","));
            }
        }
        concat(parts)
    }
}

fn variant_str(v: &Variant) -> String {
    match &v.kind {
        VariantKind::Unit => v.name.name.clone(),
        VariantKind::Tuple(ts) => format!(
            "{}({})",
            v.name.name,
            ts.iter().map(ty_str).collect::<Vec<_>>().join(", ")
        ),
        VariantKind::Struct(fs) => {
            if fs.is_empty() {
                format!("{} {{}}", v.name.name)
            } else {
                format!(
                    "{} {{ {} }}",
                    v.name.name,
                    fs.iter()
                        .map(|f| format!("{}: {}", f.name.name, ty_str(&f.ty)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}
