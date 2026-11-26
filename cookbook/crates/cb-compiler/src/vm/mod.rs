//! The bytecode virtual machine.
//!
//! **Value representation.** A tagged enum ([`Value`], 24 bytes) with `Rc` for
//! heap values: strings (`Rc<str>`), aggregates (`Rc<Agg>`, copy-on-write via
//! `Rc::make_mut`, which gives tuples/structs/enums their *value semantics*)
//! and arrays (`Rc<RefCell<Vec<Value>>>`, *reference semantics*). NaN-boxing
//! would shrink values to 8 bytes but needs `unsafe` and a separate heap with a
//! tracing collector; with no floats in PEBBLE the tagged enum keeps the VM
//! 100% safe code and the speed-up over the tree walker comes from the register
//! machine, not from value packing.
//!
//! **Heap.** Reference counting. Cycles are only possible through arrays of
//! recursive enums (`push(a, E::Arr(a))`); they leak until the process exits
//! and never cause unsafety. Dropping a very long chain of nested values
//! recurses natively, so run the VM on a thread with a large stack (the CLI does).
//!
//! **Frames.** One shared register stack (`Vec<Value>`); a frame is a window
//! `base..base+nregs`. Calls push a [`Frame`], returns pop it, so deep PEBBLE
//! recursion never touches the native stack. Runtime errors carry the same
//! messages and stack trace shape as `interp` (the logical frames of inlined
//! code are recovered from the per-instruction scope table).

use crate::bc::*;
use crate::interp::{RunOptions, RuntimeError, RuntimeErrorKind, TraceFrame};
use crate::source::Span;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Agg {
    pub tag: u32,
    pub fields: Vec<Value>,
}

#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
    Str(Rc<str>),
    Agg(Rc<Agg>),
    Array(Rc<RefCell<Vec<Value>>>),
    Fn(u32),
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Unit, Value::Unit) => true,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Fn(x), Value::Fn(y)) => x == y,
        (Value::Agg(x), Value::Agg(y)) => {
            Rc::ptr_eq(x, y)
                || (x.tag == y.tag
                    && x.fields.len() == y.fields.len()
                    && x.fields.iter().zip(&y.fields).all(|(p, q)| values_eq(p, q)))
        }
        (Value::Array(x), Value::Array(y)) => {
            Rc::ptr_eq(x, y) || {
                let (x, y) = (x.borrow(), y.borrow());
                x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| values_eq(p, q))
            }
        }
        _ => false,
    }
}

/// Execution statistics (for `--time` and the benchmark command).
#[derive(Clone, Copy, Debug, Default)]
pub struct VmStats {
    pub instructions: u64,
    pub max_depth: usize,
}

/// Bounds hostile bytecode: at most this many live register slots and bytes per string.
const MAX_STACK_SLOTS: usize = 16 * 1024 * 1024;
const MAX_STRING_BYTES: usize = 64 * 1024 * 1024;

struct Trap {
    kind: RuntimeErrorKind,
    message: String,
}

type Res<T> = Result<T, Box<Trap>>;

fn trap(kind: RuntimeErrorKind, message: impl Into<String>) -> Box<Trap> {
    Box::new(Trap {
        kind,
        message: message.into(),
    })
}

fn internal(what: &str) -> Box<Trap> {
    trap(
        RuntimeErrorKind::Internal,
        format!("internal error: {what}"),
    )
}

struct Frame {
    func: u32,
    /// Resume pc (already advanced past the call for suspended frames).
    pc: usize,
    base: usize,
    /// Caller register receiving the result.
    dest: u16,
    /// Cache the result as the value of this const initialiser.
    memo: bool,
}

fn display(v: &Value) -> Res<String> {
    Ok(match v {
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.to_string(),
        Value::Unit => "()".to_string(),
        _ => return Err(internal("value is not printable")),
    })
}

fn bounds(idx: i64, len: usize) -> Box<Trap> {
    trap(
        RuntimeErrorKind::Bounds,
        format!("index out of bounds: the length is {len} but the index is {idx}"),
    )
}

fn overflow(what: &str) -> Box<Trap> {
    trap(
        RuntimeErrorKind::Overflow,
        format!("integer overflow in {what}"),
    )
}

/// Run `main` of a verified chunk. Output is buffered and written at the end
/// (also on error), exactly like the tree-walking interpreter.
pub fn run(
    chunk: &Chunk,
    out: &mut dyn Write,
    opts: &RunOptions,
) -> (Result<(), RuntimeError>, VmStats) {
    let mut vm = Vm {
        chunk,
        regs: Vec::new(),
        frames: Vec::new(),
        out: Vec::new(),
        consts: vec![None; chunk.funcs.len()],
        stats: VmStats::default(),
    };
    let res = vm.exec(opts);
    let write_failed = out.write_all(&vm.out).and_then(|_| out.flush()).is_err();
    let res = match (res, write_failed) {
        (Ok(()), true) => Err(RuntimeError {
            kind: RuntimeErrorKind::Internal,
            message: "could not write program output".into(),
            span: None,
            trace: vec![],
        }),
        (r, _) => r,
    };
    (res, vm.stats)
}

struct Vm<'a> {
    chunk: &'a Chunk,
    regs: Vec<Value>,
    frames: Vec<Frame>,
    out: Vec<u8>,
    consts: Vec<Option<Value>>,
    stats: VmStats,
}

impl Vm<'_> {
    fn exec(&mut self, opts: &RunOptions) -> Result<(), RuntimeError> {
        let main = self.chunk.main as usize;
        let Some(mf) = self.chunk.funcs.get(main) else {
            return Err(self.error(
                *trap(
                    RuntimeErrorKind::Internal,
                    "internal error: no `main` function",
                ),
                0,
            ));
        };
        self.regs.resize(mf.nregs as usize, Value::Unit);
        self.frames.push(Frame {
            func: main as u32,
            pc: 0,
            base: 0,
            dest: 0,
            memo: false,
        });
        match self.run_loop(opts) {
            Ok(()) => Ok(()),
            Err((t, pc)) => Err(self.error(*t, pc)),
        }
    }

    /// Build the runtime error with a stack trace: the innermost frame first;
    /// scope chains expand inlined callees into their logical frames.
    fn error(&self, t: Trap, pc: usize) -> RuntimeError {
        let n = self.frames.len();
        let at = |i: usize, fr: &Frame| {
            if i + 1 == n {
                pc
            } else {
                fr.pc.saturating_sub(1)
            }
        };
        let span_at = |i: usize, fr: &Frame| {
            self.chunk.funcs[fr.func as usize]
                .pc_span
                .get(at(i, fr))
                .copied()
                .filter(|s| *s != Span::default())
        };
        // The span of the failing instruction, and for every other frame the span of its call instruction.
        let span = self.frames.last().and_then(|fr| span_at(n - 1, fr));
        let mut trace = Vec::new();
        for (i, fr) in self.frames.iter().enumerate().rev() {
            let f = &self.chunk.funcs[fr.func as usize];
            // Physical frame `i` was entered by the call instruction of frame `i - 1`.
            let entered_at = if i > 0 {
                span_at(i - 1, &self.frames[i - 1])
            } else {
                None
            };
            let mut scope = f.pc_scope.get(at(i, fr)).copied();
            while let Some(s) = scope {
                let Some(sc) = f.scopes.get(s as usize) else {
                    break;
                };
                if !(f.is_const && sc.parent.is_none()) {
                    let (call_site, inlined) = match sc.parent {
                        Some(_) => (sc.call_site, true),
                        None => (entered_at.unwrap_or_default(), false),
                    };
                    trace.push(TraceFrame {
                        function: sc.name.clone(),
                        call_site,
                        inlined,
                    });
                }
                scope = sc.parent;
            }
        }
        RuntimeError {
            kind: t.kind,
            message: t.message,
            span,
            trace,
        }
    }

    #[inline(always)]
    fn int(v: &Value) -> Res<i64> {
        match v {
            Value::Int(i) => Ok(*i),
            _ => Err(internal("expected int")),
        }
    }

    fn run_loop(&mut self, opts: &RunOptions) -> Result<(), (Box<Trap>, usize)> {
        let chunk = self.chunk;
        let mut fi = self.frames.last().map_or(0, |f| f.func) as usize;
        let mut func = &chunk.funcs[fi];
        let mut code: &[Instr] = &func.code;
        let mut base = 0usize;
        let mut pc = 0usize;
        let mut steps: u64 = 0;
        let fuel = opts.fuel;
        macro_rules! reg {
            ($r:expr) => {
                self.regs[base + $r.0 as usize]
            };
        }
        macro_rules! tryv {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(t) => {
                        self.stats.instructions += steps;
                        return Err((t, pc.saturating_sub(1)));
                    }
                }
            };
        }
        macro_rules! fail {
            ($t:expr) => {{
                self.stats.instructions += steps;
                return Err(($t, pc.saturating_sub(1)));
            }};
        }
        macro_rules! arith {
            ($d:expr, $a:expr, $b:expr, $m:ident, $what:expr) => {{
                let x = tryv!(Self::int(&reg!($a)));
                let y = tryv!(Self::int(&reg!($b)));
                match x.$m(y) {
                    Some(v) => reg!($d) = Value::Int(v),
                    None => fail!(overflow($what)),
                }
            }};
        }
        macro_rules! cmp {
            ($d:expr, $a:expr, $b:expr, $op:tt) => {{
                let x = tryv!(Self::int(&reg!($a)));
                let y = tryv!(Self::int(&reg!($b)));
                reg!($d) = Value::Bool(x $op y);
            }};
        }
        macro_rules! cmps {
            ($d:expr, $a:expr, $b:expr, $op:tt) => {{
                let r = match (&reg!($a), &reg!($b)) {
                    (Value::Str(x), Value::Str(y)) => x $op y,
                    _ => fail!(internal("expected strings")),
                };
                reg!($d) = Value::Bool(r);
            }};
        }
        macro_rules! call {
            ($callee_id:expr, $dest:expr, $args:expr) => {{
                let callee_id: u32 = $callee_id;
                let dest: u16 = $dest;
                let Some(callee) = chunk.funcs.get(callee_id as usize) else {
                    fail!(internal("call of unknown function"))
                };
                let Some(arglist) = func.lists.get($args.0 as usize) else {
                    fail!(internal("bad list"))
                };
                if arglist.len() != callee.nparams as usize {
                    fail!(internal("call arity mismatch"));
                }
                if callee.is_const
                    && let Some(Some(v)) = self.consts.get(callee_id as usize)
                {
                    let v = v.clone();
                    self.regs[base + dest as usize] = v;
                    continue;
                }
                if self.frames.len() >= opts.max_depth {
                    fail!(trap(
                        RuntimeErrorKind::DepthLimit,
                        format!("recursion depth limit ({}) exceeded", opts.max_depth)
                    ));
                }
                let new_base = base + func.nregs as usize;
                if new_base + callee.nregs as usize > MAX_STACK_SLOTS {
                    fail!(trap(
                        RuntimeErrorKind::DepthLimit,
                        "register stack exhausted"
                    ));
                }
                if self.regs.len() < new_base + callee.nregs as usize {
                    self.regs
                        .resize(new_base + callee.nregs as usize, Value::Unit);
                }
                for (i, r) in arglist.iter().enumerate() {
                    self.regs[new_base + i] = self.regs[base + *r as usize].clone();
                }
                if let Some(top) = self.frames.last_mut() {
                    top.pc = pc;
                }
                self.frames.push(Frame {
                    func: callee_id,
                    pc: 0,
                    base: new_base,
                    dest,
                    memo: callee.is_const,
                });
                self.stats.max_depth = self.stats.max_depth.max(self.frames.len());
                fi = callee_id as usize;
                func = callee;
                code = &func.code;
                base = new_base;
                pc = 0;
            }};
        }
        loop {
            steps += 1;
            if steps > fuel {
                // `pc` has not advanced yet: report the instruction that would have run.
                self.stats.instructions += steps - 1;
                return Err((
                    trap(
                        RuntimeErrorKind::FuelExhausted,
                        "step limit exceeded (possible infinite loop)",
                    ),
                    pc,
                ));
            }
            let Some(ins) = code.get(pc) else {
                fail!(internal("pc out of range"))
            };
            pc += 1;
            match ins {
                Instr::Move { d, s } => reg!(d) = reg!(s).clone(),
                Instr::LoadK { d, k } => {
                    reg!(d) = match chunk.consts.get(k.0 as usize) {
                        Some(BConst::Int(i)) => Value::Int(*i),
                        Some(BConst::Str(s)) => Value::Str(s.clone()),
                        None => fail!(internal("bad constant")),
                    }
                }
                Instr::LoadI { d, v } => reg!(d) = Value::Int(v.0 as i64),
                Instr::LoadB { d, v } => reg!(d) = Value::Bool(v.0),
                Instr::LoadUnit { d } => reg!(d) = Value::Unit,
                Instr::LoadFn { d, f } => reg!(d) = Value::Fn(f.0),
                Instr::Add { d, a, b } => arith!(d, a, b, checked_add, "addition"),
                Instr::Sub { d, a, b } => arith!(d, a, b, checked_sub, "subtraction"),
                Instr::Mul { d, a, b } => arith!(d, a, b, checked_mul, "multiplication"),
                Instr::Div { d, a, b } | Instr::Rem { d, a, b } => {
                    let is_div = matches!(ins, Instr::Div { .. });
                    let x = tryv!(Self::int(&reg!(a)));
                    let y = tryv!(Self::int(&reg!(b)));
                    if y == 0 {
                        fail!(trap(
                            RuntimeErrorKind::DivZero,
                            if is_div {
                                "division by zero"
                            } else {
                                "remainder by zero"
                            }
                        ));
                    }
                    match if is_div {
                        x.checked_div(y)
                    } else {
                        x.checked_rem(y)
                    } {
                        Some(v) => reg!(d) = Value::Int(v),
                        None => fail!(overflow("division")),
                    }
                }
                Instr::AddI { d, a, v } => {
                    let x = tryv!(Self::int(&reg!(a)));
                    match x.checked_add(v.0 as i64) {
                        Some(r) => reg!(d) = Value::Int(r),
                        None => fail!(overflow("addition")),
                    }
                }
                Instr::SubI { d, a, v } => {
                    let x = tryv!(Self::int(&reg!(a)));
                    match x.checked_sub(v.0 as i64) {
                        Some(r) => reg!(d) = Value::Int(r),
                        None => fail!(overflow("subtraction")),
                    }
                }
                Instr::MulI { d, a, v } => {
                    let x = tryv!(Self::int(&reg!(a)));
                    match x.checked_mul(v.0 as i64) {
                        Some(r) => reg!(d) = Value::Int(r),
                        None => fail!(overflow("multiplication")),
                    }
                }
                Instr::Neg { d, a } => {
                    let x = tryv!(Self::int(&reg!(a)));
                    match x.checked_neg() {
                        Some(r) => reg!(d) = Value::Int(r),
                        None => fail!(overflow("negation")),
                    }
                }
                Instr::EqI { d, a, b } => cmp!(d, a, b, ==),
                Instr::NeI { d, a, b } => cmp!(d, a, b, !=),
                Instr::LtI { d, a, b } => cmp!(d, a, b, <),
                Instr::LeI { d, a, b } => cmp!(d, a, b, <=),
                Instr::GtI { d, a, b } => cmp!(d, a, b, >),
                Instr::GeI { d, a, b } => cmp!(d, a, b, >=),
                Instr::LtS { d, a, b } => cmps!(d, a, b, <),
                Instr::LeS { d, a, b } => cmps!(d, a, b, <=),
                Instr::GtS { d, a, b } => cmps!(d, a, b, >),
                Instr::GeS { d, a, b } => cmps!(d, a, b, >=),
                Instr::Eq { d, a, b } => reg!(d) = Value::Bool(values_eq(&reg!(a), &reg!(b))),
                Instr::Ne { d, a, b } => reg!(d) = Value::Bool(!values_eq(&reg!(a), &reg!(b))),
                Instr::Not { d, a } => match reg!(a) {
                    Value::Bool(x) => reg!(d) = Value::Bool(!x),
                    _ => fail!(internal("expected bool")),
                },
                Instr::Cast { d, a } => match reg!(a) {
                    Value::Bool(x) => reg!(d) = Value::Int(x as i64),
                    _ => fail!(internal("expected bool")),
                },
                Instr::Concat { d, a, b } => {
                    let s = match (&reg!(a), &reg!(b)) {
                        (Value::Str(x), Value::Str(y)) if x.len() + y.len() > MAX_STRING_BYTES => {
                            fail!(internal("string too large"))
                        }
                        (Value::Str(x), Value::Str(y)) => {
                            let mut s = String::with_capacity(x.len() + y.len());
                            s.push_str(x);
                            s.push_str(y);
                            s
                        }
                        _ => fail!(internal("expected strings")),
                    };
                    reg!(d) = Value::Str(Rc::from(s));
                }
                Instr::MkAgg { d, tag, list } => {
                    let Some(l) = func.lists.get(list.0 as usize) else {
                        fail!(internal("bad list"))
                    };
                    let fields: Vec<Value> = l
                        .iter()
                        .map(|r| self.regs[base + *r as usize].clone())
                        .collect();
                    reg!(d) = Value::Agg(Rc::new(Agg {
                        tag: tag.0 as u32,
                        fields,
                    }));
                }
                Instr::MkArray { d, list } => {
                    let Some(l) = func.lists.get(list.0 as usize) else {
                        fail!(internal("bad list"))
                    };
                    let items: Vec<Value> = l
                        .iter()
                        .map(|r| self.regs[base + *r as usize].clone())
                        .collect();
                    reg!(d) = Value::Array(Rc::new(RefCell::new(items)));
                }
                Instr::GetField { d, a, i } => {
                    let v = match &reg!(a) {
                        Value::Agg(g) => g.fields.get(i.0 as usize).cloned(),
                        _ => None,
                    };
                    match v {
                        Some(v) => reg!(d) = v,
                        None => fail!(internal("bad field access")),
                    }
                }
                Instr::SetField { r, i, v } => {
                    let val = reg!(v).clone();
                    match &mut reg!(r) {
                        Value::Agg(g) => match Rc::make_mut(g).fields.get_mut(i.0 as usize) {
                            Some(slot) => *slot = val,
                            None => fail!(internal("bad field store")),
                        },
                        _ => fail!(internal("field store into non-aggregate")),
                    }
                }
                Instr::Tag { d, a } => match &reg!(a) {
                    Value::Agg(g) => {
                        let t = g.tag as i64;
                        reg!(d) = Value::Int(t)
                    }
                    _ => fail!(internal("tag of non-enum")),
                },
                Instr::Index { d, a, i } | Instr::IndexU { d, a, i } => {
                    let idx = tryv!(Self::int(&reg!(i)));
                    let r = match &reg!(a) {
                        Value::Array(arr) => {
                            let arr = arr.borrow();
                            match usize::try_from(idx).ok().and_then(|u| arr.get(u)) {
                                Some(v) => Ok(v.clone()),
                                None => Err(if matches!(ins, Instr::Index { .. }) {
                                    bounds(idx, arr.len())
                                } else {
                                    internal("unchecked index out of range")
                                }),
                            }
                        }
                        _ => Err(internal("index of non-array")),
                    };
                    reg!(d) = tryv!(r);
                }
                Instr::SetIndex { a, i, v } | Instr::SetIndexU { a, i, v } => {
                    let idx = tryv!(Self::int(&reg!(i)));
                    let val = reg!(v).clone();
                    let r = match &reg!(a) {
                        Value::Array(arr) => {
                            let mut arr = arr.borrow_mut();
                            let len = arr.len();
                            match usize::try_from(idx).ok().and_then(|u| arr.get_mut(u)) {
                                Some(slot) => {
                                    *slot = val;
                                    Ok(())
                                }
                                None => Err(if matches!(ins, Instr::SetIndex { .. }) {
                                    bounds(idx, len)
                                } else {
                                    internal("unchecked index out of range")
                                }),
                            }
                        }
                        _ => Err(internal("index of non-array")),
                    };
                    tryv!(r);
                }
                Instr::Len { d, a } => {
                    let n = match &reg!(a) {
                        Value::Str(s) => s.len(),
                        Value::Array(arr) => arr.borrow().len(),
                        _ => fail!(internal("len of non-sequence")),
                    };
                    reg!(d) = Value::Int(n as i64);
                }
                Instr::Push { a, v } => {
                    let val = reg!(v).clone();
                    match &reg!(a) {
                        Value::Array(arr) => arr.borrow_mut().push(val),
                        _ => fail!(internal("push to non-array")),
                    }
                }
                Instr::Pop { d, a } => {
                    let r = match &reg!(a) {
                        Value::Array(arr) => arr.borrow_mut().pop().ok_or_else(|| {
                            trap(RuntimeErrorKind::EmptyPop, "pop from an empty array")
                        }),
                        _ => Err(internal("pop from non-array")),
                    };
                    reg!(d) = tryv!(r);
                }
                Instr::Chars { d, a } => {
                    let cs: Vec<Value> = match &reg!(a) {
                        Value::Str(s) => s
                            .chars()
                            .map(|c| Value::Str(Rc::from(c.to_string().as_str())))
                            .collect(),
                        _ => fail!(internal("chars of non-string")),
                    };
                    reg!(d) = Value::Array(Rc::new(RefCell::new(cs)));
                }
                Instr::ToStr { d, a } => {
                    let s = tryv!(display(&reg!(a)));
                    reg!(d) = Value::Str(Rc::from(s));
                }
                Instr::Print { a } => {
                    let s = tryv!(display(&reg!(a)));
                    self.out.extend_from_slice(s.as_bytes());
                }
                Instr::Println { a } => {
                    let s = tryv!(display(&reg!(a)));
                    self.out.extend_from_slice(s.as_bytes());
                    self.out.push(b'\n');
                }
                Instr::PrintlnEmpty {} => self.out.push(b'\n'),
                Instr::Assert { c } => match reg!(c) {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        fail!(trap(RuntimeErrorKind::AssertFail, "assertion failed"))
                    }
                    _ => fail!(internal("expected bool")),
                },
                Instr::AssertMsg { c, m } => match reg!(c) {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        let s = tryv!(display(&reg!(m)));
                        fail!(trap(
                            RuntimeErrorKind::AssertFail,
                            format!("assertion failed: {s}")
                        ))
                    }
                    _ => fail!(internal("expected bool")),
                },
                Instr::Panic { m } => {
                    let s = tryv!(display(&reg!(m)));
                    fail!(trap(RuntimeErrorKind::Panic, format!("panic: {s}")))
                }
                Instr::Jmp { t } => pc = t.0 as usize,
                Instr::JmpIf { c, t } => match reg!(c) {
                    Value::Bool(true) => pc = t.0 as usize,
                    Value::Bool(false) => {}
                    _ => fail!(internal("expected bool")),
                },
                Instr::JmpIfNot { c, t } => match reg!(c) {
                    Value::Bool(false) => pc = t.0 as usize,
                    Value::Bool(true) => {}
                    _ => fail!(internal("expected bool")),
                },
                Instr::JmpLt { a, b, t } => {
                    let x = tryv!(Self::int(&reg!(a)));
                    let y = tryv!(Self::int(&reg!(b)));
                    if x < y {
                        pc = t.0 as usize;
                    }
                }
                Instr::JmpGe { a, b, t } => {
                    let x = tryv!(Self::int(&reg!(a)));
                    let y = tryv!(Self::int(&reg!(b)));
                    if x >= y {
                        pc = t.0 as usize;
                    }
                }
                Instr::JmpLtI { a, v, t } => {
                    if tryv!(Self::int(&reg!(a))) < v.0 as i64 {
                        pc = t.0 as usize;
                    }
                }
                Instr::JmpGeI { a, v, t } => {
                    if tryv!(Self::int(&reg!(a))) >= v.0 as i64 {
                        pc = t.0 as usize;
                    }
                }
                Instr::JmpGtI { a, v, t } => {
                    if tryv!(Self::int(&reg!(a))) > v.0 as i64 {
                        pc = t.0 as usize;
                    }
                }
                Instr::JmpLeI { a, v, t } => {
                    if tryv!(Self::int(&reg!(a))) <= v.0 as i64 {
                        pc = t.0 as usize;
                    }
                }
                Instr::Switch { r, s } => {
                    let v = tryv!(Self::int(&reg!(r)));
                    let Some(tab) = func.switches.get(s.0 as usize) else {
                        fail!(internal("bad switch"))
                    };
                    pc = tab
                        .cases
                        .iter()
                        .find(|c| c.0 == v)
                        .map_or(tab.default, |c| c.1) as usize;
                }
                Instr::Call { d, f, args } => call!(f.0, d.0, args),
                Instr::CallInd { d, fr, args } => {
                    let id = match &reg!(fr) {
                        Value::Fn(id) => *id,
                        _ => fail!(internal("call of a non-function")),
                    };
                    call!(id, d.0, args)
                }
                Instr::Ret { a } => {
                    let v = std::mem::replace(&mut reg!(a), Value::Unit);
                    let done = self.frames.pop().expect("frame");
                    // Release the callee's registers promptly so heap values die with the frame.
                    for r in &mut self.regs[done.base..done.base + func.nregs as usize] {
                        *r = Value::Unit;
                    }
                    if done.memo {
                        self.consts[fi] = Some(v.clone());
                    }
                    let Some(caller) = self.frames.last() else {
                        self.stats.instructions += steps;
                        return Ok(());
                    };
                    fi = caller.func as usize;
                    func = &chunk.funcs[fi];
                    code = &func.code;
                    base = caller.base;
                    pc = caller.pc;
                    self.regs[base + done.dest as usize] = v;
                }
                Instr::Unreachable {} => fail!(internal("reached unreachable code")),
            }
        }
    }
}
