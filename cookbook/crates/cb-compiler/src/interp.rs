//! A tree-walking interpreter over the typed HIR. It is the *oracle* for the
//! stage-2 VM: both must print the same output and trap in the same situations.
//!
//! Semantics worth knowing (also in `docs/LANGUAGE.md`):
//! * Integers are 64-bit and **trap** on overflow (no wrapping), division by zero,
//!   and `i64::MIN / -1`.
//! * Arrays are references (aliasing is visible); structs, enums and tuples behave
//!   like values (copy-on-write through `Rc::make_mut`).
//! * `len(str)` counts UTF-8 bytes; `chars(s)` splits into one-scalar strings.
//! * Output is buffered while the program runs and written to the sink at the end
//!   (including on error), because the interpreter runs on a big-stack thread.
//! * Limits: recursion depth and a step budget ("fuel") turn runaway programs into errors.

use crate::sema::hir::*;
use crate::sema::types::DefId;
use crate::source::Span;
use crate::syntax::ast::{BinOp, Lit, UnOp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Overflow,
    DivZero,
    Bounds,
    EmptyPop,
    Panic,
    AssertFail,
    DepthLimit,
    FuelExhausted,
    /// A bug (type confusion, unchecked program); never produced for a program that checks.
    Internal,
}

#[derive(Clone, Debug)]
pub struct TraceFrame {
    pub function: String,
    /// Where this function was called (for `main`, its own definition).
    pub call_site: Span,
    /// The call was inlined by the optimiser (only the VM at `-O3` reports this).
    pub inlined: bool,
}

#[derive(Clone, Debug)]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub message: String,
    /// The offending expression.
    pub span: Option<Span>,
    /// Innermost frame first.
    pub trace: Vec<TraceFrame>,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "runtime error: {}", self.message)?;
        let n = self.trace.len();
        for (i, fr) in self.trace.iter().enumerate() {
            if n > 12 && i == 6 {
                write!(f, "\n  ... {} more frames ...", n - 12)?;
            }
            if n > 12 && i >= 6 && i < n - 6 {
                continue;
            }
            write!(f, "\n  at {}", fr.function)?;
        }
        Ok(())
    }
}

impl RuntimeError {
    /// The error as the CLI prints it for both back ends: the [`fmt::Display`]
    /// text, the `-->` location of the failing expression, and one note per
    /// frame the optimiser inlined (those frames have no machine frame of
    /// their own, so the call site is the only evidence of the call).
    pub fn render(&self, file: &crate::source::SourceFile) -> String {
        let loc = |s: Span| {
            let lc = file.line_col_chars(s.start as usize);
            format!("{}:{}:{}", file.name, lc.line + 1, lc.col + 1)
        };
        let mut out = self.to_string();
        if let Some(s) = self.span {
            out.push_str(&format!("\n  --> {}", loc(s)));
        }
        for fr in self.trace.iter().filter(|f| f.inlined) {
            out.push_str(&format!(
                "\n  note: `{}` was inlined from {}",
                fr.function,
                loc(fr.call_site)
            ));
        }
        out
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Clone, Copy, Debug)]
pub struct RunOptions {
    pub max_depth: usize,
    pub fuel: u64,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            max_depth: 10_000,
            fuel: 200_000_000,
        }
    }
}

#[derive(Clone, Debug)]
struct AdtVal {
    def: DefId,
    variant: usize,
    fields: Vec<Value>,
}

#[derive(Clone, Debug)]
enum Value {
    Int(i64),
    Bool(bool),
    Str(Rc<str>),
    Unit,
    Tuple(Rc<Vec<Value>>),
    Array(Rc<RefCell<Vec<Value>>>),
    Adt(Rc<AdtVal>),
    Fn(DefId),
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Unit, Value::Unit) => true,
        (Value::Tuple(x), Value::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_eq(p, q))
        }
        (Value::Array(x), Value::Array(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_eq(p, q))
        }
        (Value::Adt(x), Value::Adt(y)) => {
            x.def == y.def
                && x.variant == y.variant
                && x.fields
                    .iter()
                    .zip(y.fields.iter())
                    .all(|(p, q)| values_eq(p, q))
        }
        (Value::Fn(x), Value::Fn(y)) => x == y,
        _ => false,
    }
}

enum Flow {
    Break(Option<Value>),
    Continue,
    Return(Value),
    Err(Box<RuntimeError>),
}

type R<T> = Result<T, Flow>;

fn fail(kind: RuntimeErrorKind, message: impl Into<String>, span: Span) -> Flow {
    Flow::Err(Box::new(RuntimeError {
        kind,
        message: message.into(),
        span: Some(span),
        trace: vec![],
    }))
}

fn internal(message: &str) -> Flow {
    Flow::Err(Box::new(RuntimeError {
        kind: RuntimeErrorKind::Internal,
        message: format!("internal error: {message}"),
        span: None,
        trace: vec![],
    }))
}

enum ConstState {
    Running,
    Done(Value),
}

struct Interp<'a> {
    prog: &'a Program,
    out: Vec<u8>,
    fuel: u64,
    max_depth: usize,
    frames: Vec<TraceFrame>,
    locals: Vec<Value>,
    base: u32,
    consts: HashMap<DefId, ConstState>,
}

/// Run `main` with default limits, writing program output to `out`.
pub fn run(prog: &Program, out: &mut dyn Write) -> Result<(), RuntimeError> {
    run_with(prog, out, &RunOptions::default())
}

pub fn run_with(
    prog: &Program,
    out: &mut dyn Write,
    opts: &RunOptions,
) -> Result<(), RuntimeError> {
    let opts = *opts;
    let joined = std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(768 * 1024 * 1024)
            .spawn_scoped(s, || {
                let mut it = Interp {
                    prog,
                    out: Vec::new(),
                    fuel: opts.fuel,
                    max_depth: opts.max_depth,
                    frames: Vec::new(),
                    locals: Vec::new(),
                    base: 0,
                    consts: HashMap::new(),
                };
                let res = it.run_main();
                (it.out, res)
            })
            .map(|h| h.join())
    });
    let (buf, res) = match joined {
        Ok(Ok(x)) => x,
        _ => {
            return Err(RuntimeError {
                kind: RuntimeErrorKind::Internal,
                message: "interpreter thread failed".into(),
                span: None,
                trace: vec![],
            });
        }
    };
    if out.write_all(&buf).and_then(|_| out.flush()).is_err() {
        return Err(RuntimeError {
            kind: RuntimeErrorKind::Internal,
            message: "could not write program output".into(),
            span: None,
            trace: vec![],
        });
    }
    res
}

impl Interp<'_> {
    fn run_main(&mut self) -> Result<(), RuntimeError> {
        let Some(main) = self.prog.main else {
            return Err(RuntimeError {
                kind: RuntimeErrorKind::Internal,
                message: "no `main` function".into(),
                span: None,
                trace: vec![],
            });
        };
        match self.call_fn(main, vec![], self.prog.def(main).span) {
            Ok(_) => Ok(()),
            Err(Flow::Err(e)) => Err(*e),
            Err(_) => Err(RuntimeError {
                kind: RuntimeErrorKind::Internal,
                message: "stray control flow".into(),
                span: None,
                trace: vec![],
            }),
        }
    }

    fn slot(&self, d: DefId) -> R<usize> {
        let i =
            d.0.checked_sub(self.base)
                .map(|i| i as usize)
                .filter(|i| *i < self.locals.len());
        i.ok_or_else(|| internal("local out of range"))
    }

    fn call_fn(&mut self, def: DefId, args: Vec<Value>, call_site: Span) -> R<Value> {
        let prog = self.prog;
        let Some(f) = prog.fn_by_def(def) else {
            return Err(internal("call of unknown function"));
        };
        if self.frames.len() >= self.max_depth {
            return Err(fail(
                RuntimeErrorKind::DepthLimit,
                format!("recursion depth limit ({}) exceeded", self.max_depth),
                call_site,
            ));
        }
        let saved = std::mem::replace(&mut self.locals, vec![Value::Unit; f.locals_len as usize]);
        let saved_base = std::mem::replace(&mut self.base, f.locals_start);
        self.frames.push(TraceFrame {
            function: f.name.clone(),
            call_site,
            inlined: false,
        });
        let mut res = (|| {
            for (p, a) in f.params.iter().zip(args) {
                let i = self.slot(*p)?;
                self.locals[i] = a;
            }
            match self.eval(&f.body) {
                Ok(v) | Err(Flow::Return(v)) => Ok(v),
                Err(e) => Err(e),
            }
        })();
        if let Err(Flow::Err(e)) = &mut res
            && e.trace.is_empty()
        {
            e.trace = self.frames.iter().rev().cloned().collect();
        }
        self.frames.pop();
        self.locals = saved;
        self.base = saved_base;
        res
    }

    fn eval_const(&mut self, def: DefId) -> R<Value> {
        match self.consts.get(&def) {
            Some(ConstState::Done(v)) => return Ok(v.clone()),
            Some(ConstState::Running) => return Err(internal("cyclic constant")),
            None => {}
        }
        let prog = self.prog;
        let Some(c) = prog.const_by_def(def) else {
            return Err(internal("unknown constant"));
        };
        self.consts.insert(def, ConstState::Running);
        let saved = std::mem::replace(&mut self.locals, vec![Value::Unit; c.locals_len as usize]);
        let saved_base = std::mem::replace(&mut self.base, c.locals_start);
        let r = self.eval(&c.body);
        self.locals = saved;
        self.base = saved_base;
        let v = r?;
        self.consts.insert(def, ConstState::Done(v.clone()));
        Ok(v)
    }

    fn as_int(v: Value) -> R<i64> {
        match v {
            Value::Int(i) => Ok(i),
            _ => Err(internal("expected int")),
        }
    }
    fn as_bool(v: Value) -> R<bool> {
        match v {
            Value::Bool(b) => Ok(b),
            _ => Err(internal("expected bool")),
        }
    }

    fn eval(&mut self, e: &HirExpr) -> R<Value> {
        if self.fuel == 0 {
            return Err(fail(
                RuntimeErrorKind::FuelExhausted,
                "step limit exceeded (possible infinite loop)",
                e.span,
            ));
        }
        self.fuel -= 1;
        match &e.kind {
            HirExprKind::Lit(l) => Ok(match l {
                Lit::Int(i) => Value::Int(*i),
                Lit::Bool(b) => Value::Bool(*b),
                Lit::Str(s) => Value::Str(Rc::from(s.as_str())),
                Lit::Unit => Value::Unit,
            }),
            HirExprKind::Local(d) => {
                let i = self.slot(*d)?;
                Ok(self.locals[i].clone())
            }
            HirExprKind::Fn(d, _) => Ok(Value::Fn(*d)),
            HirExprKind::Const(d) => self.eval_const(*d),
            HirExprKind::Unary(op, a) => {
                let v = self.eval(a)?;
                match op {
                    UnOp::Neg => Self::as_int(v)?
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| {
                            fail(
                                RuntimeErrorKind::Overflow,
                                "integer overflow in negation",
                                e.span,
                            )
                        }),
                    UnOp::Not => Ok(Value::Bool(!Self::as_bool(v)?)),
                }
            }
            HirExprKind::Binary(op, a, b) => {
                if matches!(op, BinOp::And | BinOp::Or) {
                    let l = Self::as_bool(self.eval(a)?)?;
                    if (*op == BinOp::And && !l) || (*op == BinOp::Or && l) {
                        return Ok(Value::Bool(l));
                    }
                    return Ok(Value::Bool(Self::as_bool(self.eval(b)?)?));
                }
                let l = self.eval(a)?;
                let r = self.eval(b)?;
                binop(*op, l, r, e.span)
            }
            HirExprKind::Assign { op, target, value } => {
                let rhs = self.eval(value)?;
                let span = e.span;
                let op = *op;
                self.modify(target, &mut |slot| {
                    let new = match op {
                        None => rhs.clone(),
                        Some(o) => binop(o, slot.clone(), rhs.clone(), span)?,
                    };
                    *slot = new;
                    Ok(())
                })?;
                Ok(Value::Unit)
            }
            HirExprKind::Call(c, args) => {
                let f = self.eval(c)?;
                let mut vs = Vec::with_capacity(args.len());
                for a in args {
                    vs.push(self.eval(a)?);
                }
                match f {
                    Value::Fn(d) => self.call_fn(d, vs, e.span),
                    _ => Err(internal("call of a non-function")),
                }
            }
            HirExprKind::Builtin(b, args) => self.builtin(*b, args, e.span),
            HirExprKind::Field(b, i) => match self.eval(b)? {
                Value::Adt(a) => a
                    .fields
                    .get(*i)
                    .cloned()
                    .ok_or_else(|| internal("field index")),
                _ => Err(internal("field of non-struct")),
            },
            HirExprKind::TupleField(b, i) => match self.eval(b)? {
                Value::Tuple(t) => t.get(*i).cloned().ok_or_else(|| internal("tuple index")),
                _ => Err(internal("tuple field of non-tuple")),
            },
            HirExprKind::Index(b, i) => {
                let arr = self.eval(b)?;
                let idx = Self::as_int(self.eval(i)?)?;
                match arr {
                    Value::Array(a) => {
                        let a = a.borrow();
                        usize::try_from(idx)
                            .ok()
                            .and_then(|u| a.get(u).cloned())
                            .ok_or_else(|| bounds(idx, a.len(), e.span))
                    }
                    _ => Err(internal("index of non-array")),
                }
            }
            HirExprKind::Struct { def, fields } => {
                let n = self
                    .prog
                    .struct_info(*def)
                    .map(|s| s.fields.len())
                    .unwrap_or(0);
                let vals = self.eval_fields(n, fields)?;
                Ok(Value::Adt(Rc::new(AdtVal {
                    def: *def,
                    variant: 0,
                    fields: vals,
                })))
            }
            HirExprKind::Variant {
                enum_def,
                variant,
                fields,
            } => {
                let n = self
                    .prog
                    .enum_info(*enum_def)
                    .map(|s| s.variants[*variant].fields.len())
                    .unwrap_or(0);
                let vals = self.eval_fields(n, fields)?;
                Ok(Value::Adt(Rc::new(AdtVal {
                    def: *enum_def,
                    variant: *variant,
                    fields: vals,
                })))
            }
            HirExprKind::Tuple(xs) => {
                let mut vs = Vec::new();
                for x in xs {
                    vs.push(self.eval(x)?);
                }
                Ok(Value::Tuple(Rc::new(vs)))
            }
            HirExprKind::Array(xs) => {
                let mut vs = Vec::new();
                for x in xs {
                    vs.push(self.eval(x)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(vs))))
            }
            HirExprKind::If(c, t, el) => {
                if Self::as_bool(self.eval(c)?)? {
                    self.eval(t)
                } else if let Some(el) = el {
                    self.eval(el)
                } else {
                    Ok(Value::Unit)
                }
            }
            HirExprKind::Match(s, arms) => {
                let v = self.eval(s)?;
                for arm in arms {
                    if self.match_pat(&arm.pat, &v)? {
                        if let Some(g) = &arm.guard
                            && !Self::as_bool(self.eval(g)?)?
                        {
                            continue;
                        }
                        return self.eval(&arm.body);
                    }
                }
                Err(fail(
                    RuntimeErrorKind::Internal,
                    "internal error: no match arm matched",
                    e.span,
                ))
            }
            HirExprKind::Block(b) => {
                for s in &b.stmts {
                    match s {
                        HirStmt::Let { pat, init } => {
                            let v = self.eval(init)?;
                            if !self.match_pat(pat, &v)? {
                                return Err(internal("refutable let"));
                            }
                        }
                        HirStmt::Expr(x) => {
                            self.eval(x)?;
                        }
                    }
                }
                match &b.tail {
                    Some(t) => self.eval(t),
                    None => Ok(Value::Unit),
                }
            }
            HirExprKind::While(c, body) => {
                while Self::as_bool(self.eval(c)?)? {
                    match self.eval(body) {
                        Ok(_) | Err(Flow::Continue) => {}
                        Err(Flow::Break(_)) => break,
                        Err(e) => return Err(e),
                    }
                }
                Ok(Value::Unit)
            }
            HirExprKind::Loop(body) => loop {
                match self.eval(body) {
                    Ok(_) | Err(Flow::Continue) => {}
                    Err(Flow::Break(v)) => return Ok(v.unwrap_or(Value::Unit)),
                    Err(e) => return Err(e),
                }
            },
            HirExprKind::Break(v) => {
                let v = match v {
                    Some(x) => Some(self.eval(x)?),
                    None => None,
                };
                Err(Flow::Break(v))
            }
            HirExprKind::Continue => Err(Flow::Continue),
            HirExprKind::Return(v) => {
                let v = match v {
                    Some(x) => self.eval(x)?,
                    None => Value::Unit,
                };
                Err(Flow::Return(v))
            }
            HirExprKind::Cast(a) => Ok(Value::Int(i64::from(Self::as_bool(self.eval(a)?)?))),
            HirExprKind::Error => Err(internal("program contains errors")),
        }
    }

    fn eval_fields(&mut self, n: usize, fields: &[(usize, HirExpr)]) -> R<Vec<Value>> {
        let mut vals = vec![Value::Unit; n];
        for (i, f) in fields {
            let v = self.eval(f)?;
            *vals.get_mut(*i).ok_or_else(|| internal("field index"))? = v;
        }
        Ok(vals)
    }

    /// Apply `f` to the storage slot designated by a place expression, evaluating
    /// index sub-expressions exactly once.
    fn modify(&mut self, target: &HirExpr, f: &mut dyn FnMut(&mut Value) -> R<()>) -> R<()> {
        match &target.kind {
            HirExprKind::Local(d) => {
                let i = self.slot(*d)?;
                f(&mut self.locals[i])
            }
            HirExprKind::Field(b, i) | HirExprKind::TupleField(b, i) => {
                let i = *i;
                self.modify(b, &mut |v| match v {
                    Value::Adt(rc) => f(Rc::make_mut(rc)
                        .fields
                        .get_mut(i)
                        .ok_or_else(|| internal("field index"))?),
                    Value::Tuple(rc) => f(Rc::make_mut(rc)
                        .get_mut(i)
                        .ok_or_else(|| internal("tuple index"))?),
                    _ => Err(internal("field of non-aggregate")),
                })
            }
            HirExprKind::Index(b, ix) => {
                let arr = self.eval(b)?;
                let idx = Self::as_int(self.eval(ix)?)?;
                match arr {
                    Value::Array(a) => {
                        let mut a = a.borrow_mut();
                        let len = a.len();
                        let slot = usize::try_from(idx)
                            .ok()
                            .and_then(|u| a.get_mut(u))
                            .ok_or_else(|| bounds(idx, len, target.span))?;
                        f(slot)
                    }
                    _ => Err(internal("index of non-array")),
                }
            }
            _ => Err(internal("not a place")),
        }
    }

    fn match_pat(&mut self, p: &HirPat, v: &Value) -> R<bool> {
        match (&p.kind, v) {
            (HirPatKind::Wild, _) => Ok(true),
            (HirPatKind::Bind(d, _), _) => {
                let i = self.slot(*d)?;
                self.locals[i] = v.clone();
                Ok(true)
            }
            (HirPatKind::Lit(l), _) => Ok(match (l, v) {
                (Lit::Int(a), Value::Int(b)) => a == b,
                (Lit::Bool(a), Value::Bool(b)) => a == b,
                (Lit::Str(a), Value::Str(b)) => a.as_str() == &**b,
                (Lit::Unit, Value::Unit) => true,
                _ => false,
            }),
            (HirPatKind::Tuple(ps), Value::Tuple(vs)) => self.match_all(ps, vs),
            (HirPatKind::Struct(_, ps), Value::Adt(a)) => self.match_all(ps, &a.fields),
            (
                HirPatKind::Variant {
                    variant, fields, ..
                },
                Value::Adt(a),
            ) => {
                if a.variant != *variant {
                    return Ok(false);
                }
                self.match_all(fields, &a.fields)
            }
            (HirPatKind::Or(ps), _) => {
                for q in ps {
                    if self.match_pat(q, v)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Err(internal("pattern/value shape mismatch")),
        }
    }

    fn match_all(&mut self, ps: &[HirPat], vs: &[Value]) -> R<bool> {
        for (p, v) in ps.iter().zip(vs) {
            if !self.match_pat(p, v)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn builtin(&mut self, b: Builtin, args: &[HirExpr], span: Span) -> R<Value> {
        let mut vs = Vec::with_capacity(args.len());
        for a in args {
            vs.push(self.eval(a)?);
        }
        match (b, vs.as_slice()) {
            (Builtin::Print, [v]) => {
                self.out.extend_from_slice(display(v)?.as_bytes());
                Ok(Value::Unit)
            }
            (Builtin::Println, vs) => {
                if let Some(v) = vs.first() {
                    self.out.extend_from_slice(display(v)?.as_bytes());
                }
                self.out.push(b'\n');
                Ok(Value::Unit)
            }
            (Builtin::Assert, [c, rest @ ..]) => {
                if Self::as_bool(c.clone())? {
                    return Ok(Value::Unit);
                }
                let msg = match rest.first() {
                    Some(m) => format!("assertion failed: {}", display(m)?),
                    None => "assertion failed".to_string(),
                };
                Err(fail(RuntimeErrorKind::AssertFail, msg, span))
            }
            (Builtin::Panic, [m]) => Err(fail(
                RuntimeErrorKind::Panic,
                format!("panic: {}", display(m)?),
                span,
            )),
            (Builtin::ToStr, [v]) => Ok(Value::Str(Rc::from(display(v)?.as_str()))),
            (Builtin::Len, [Value::Str(s)]) => Ok(Value::Int(s.len() as i64)),
            (Builtin::Len, [Value::Array(a)]) => Ok(Value::Int(a.borrow().len() as i64)),
            (Builtin::Push, [Value::Array(a), v]) => {
                a.borrow_mut().push(v.clone());
                Ok(Value::Unit)
            }
            (Builtin::Pop, [Value::Array(a)]) => a
                .borrow_mut()
                .pop()
                .ok_or_else(|| fail(RuntimeErrorKind::EmptyPop, "pop from an empty array", span)),
            (Builtin::Chars, [Value::Str(s)]) => {
                let cs: Vec<Value> = s
                    .chars()
                    .map(|c| Value::Str(Rc::from(c.to_string().as_str())))
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(cs))))
            }
            _ => Err(internal("bad built-in call")),
        }
    }
}

fn bounds(idx: i64, len: usize, span: Span) -> Flow {
    fail(
        RuntimeErrorKind::Bounds,
        format!("index out of bounds: the length is {len} but the index is {idx}"),
        span,
    )
}

fn display(v: &Value) -> R<String> {
    Ok(match v {
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.to_string(),
        Value::Unit => "()".to_string(),
        _ => return Err(internal("value is not printable")),
    })
}

fn binop(op: BinOp, l: Value, r: Value, span: Span) -> R<Value> {
    use RuntimeErrorKind::*;
    match (op, &l, &r) {
        (BinOp::Eq, ..) => Ok(Value::Bool(values_eq(&l, &r))),
        (BinOp::Ne, ..) => Ok(Value::Bool(!values_eq(&l, &r))),
        (BinOp::Add, Value::Str(a), Value::Str(b)) => {
            Ok(Value::Str(Rc::from(format!("{a}{b}").as_str())))
        }
        (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, Value::Str(a), Value::Str(b)) => {
            Ok(Value::Bool(cmp(op, a.cmp(b))))
        }
        (_, Value::Int(a), Value::Int(b)) => {
            let (a, b) = (*a, *b);
            let ovf = |what: &str| fail(Overflow, format!("integer overflow in {what}"), span);
            match op {
                BinOp::Add => a
                    .checked_add(b)
                    .map(Value::Int)
                    .ok_or_else(|| ovf("addition")),
                BinOp::Sub => a
                    .checked_sub(b)
                    .map(Value::Int)
                    .ok_or_else(|| ovf("subtraction")),
                BinOp::Mul => a
                    .checked_mul(b)
                    .map(Value::Int)
                    .ok_or_else(|| ovf("multiplication")),
                BinOp::Div | BinOp::Rem => {
                    if b == 0 {
                        return Err(fail(
                            DivZero,
                            if op == BinOp::Div {
                                "division by zero"
                            } else {
                                "remainder by zero"
                            },
                            span,
                        ));
                    }
                    let r = if op == BinOp::Div {
                        a.checked_div(b)
                    } else {
                        a.checked_rem(b)
                    };
                    r.map(Value::Int).ok_or_else(|| ovf("division"))
                }
                _ => Ok(Value::Bool(cmp(op, a.cmp(&b)))),
            }
        }
        _ => Err(internal("bad operands")),
    }
}

fn cmp(op: BinOp, o: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        BinOp::Lt => o == Less,
        BinOp::Le => o != Greater,
        BinOp::Gt => o == Greater,
        _ => o != Less,
    }
}
