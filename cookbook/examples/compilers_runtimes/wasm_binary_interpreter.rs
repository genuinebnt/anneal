//! RECIPE 421 (compilers_runtimes): WebAssembly (MVP subset) binary decoder + validator + interpreter, with an assembler.
//! LEVEL: advanced
//! Use in: plugin sandboxes, edge/serverless runtimes, smart contracts, browsers, portable bytecode targets.   Run: cargo run --release --example wasm_binary_interpreter
//!
//! WHAT: the full pipeline a WebAssembly engine runs on untrusted bytes: DECODE (magic/version, sections type,
//! function, table, memory, global, export, start, element, code, data; LEB128 integers), VALIDATE (a type check
//! of the operand stack per instruction and of structured control flow), INSTANTIATE (globals, memory, table,
//! data and element segments, start function) and EXECUTE. Value types are i32 and i64 (floats are left out;
//! the structure would not change). A small ASSEMBLER (`Asm` + `ModuleBuilder`) builds binaries so tests can
//! create modules, and two modules are also written out BY HAND as byte arrays and compared with the builder.
//!
//! HOW: (1) LEB128: 7 bits per byte, high bit = continue. We cap the byte count by the integer width and reject
//! non-canonical final bytes, so decoding is total: every failure is an `Error { offset, msg }`, never a panic.
//! Vector lengths are checked against remaining bytes before anything is allocated (no allocation bombs).
//! (2) Function bodies are decoded once into `Vec<Instr>`; block/loop/if/else/end are matched with a stack and
//! each `Block`/`If` stores the index of its `End` (and `Else`), so a branch is an index jump, not a scan.
//! (3) Validation follows the spec's algorithm: an operand stack of `Option<ValType>` (None = "unknown",
//! produced after `unreachable`/`br`, which makes the stack polymorphic) plus a control-frame stack that records
//! the height at entry; `end` must leave exactly the block result above that height.
//! (4) The interpreter keeps ONE u64 value stack: callee locals are the top of the caller's stack (arguments
//! become locals, zero-filled extra locals follow), and a label stack records, per block, the continuation
//! pc, stack height and branch arity. `br n` copies the label's arity values down to its height, pops labels
//! and jumps. Traps are a `Result` error: div by zero, `i32::MIN / -1`, out-of-bounds memory (computed in u64 so
//! `addr + offset` cannot wrap), bad `call_indirect`, call-depth limit, and a FUEL counter that stops infinite loops.
//! Shift counts are taken modulo the width, rotates likewise; `rem_s(MIN, -1)` is 0, not a trap.
//!
//! COMMON USES: running untrusted plugins in a sandbox (Envoy/proxy filters, database UDFs, editors), edge and
//! serverless runtimes, blockchain VMs that meter execution with fuel/gas, language targets (Rust, C, Go) in
//! browsers, and teaching VM design: validation up front is what lets the interpreter skip per-op type checks.
//! Real engines: wasmtime (Cranelift JIT) and wasmi/wasm3 (interpreters).
//! Read: WebAssembly Core Specification https://webassembly.github.io/spec/core/   wasmtime https://github.com/bytecodealliance/wasmtime
#![allow(dead_code)]
use std::cmp::Ordering;

// ===================== errors and basic types =====================
#[derive(Debug, Clone, PartialEq)]
struct Error {
    offset: usize,
    msg: String,
}
type Res<T> = Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValType {
    I32,
    I64,
}
impl ValType {
    fn from_byte(b: u8) -> Option<ValType> {
        match b {
            0x7F => Some(ValType::I32),
            0x7E => Some(ValType::I64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct FuncType {
    params: Vec<ValType>,
    results: Vec<ValType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Trap {
    Unreachable,
    DivByZero,
    IntOverflow,
    OutOfBoundsMemory,
    UndefinedElement,
    UninitializedElement,
    IndirectCallTypeMismatch,
    CallStackExhausted,
    OutOfFuel,
    NoSuchExport,
    BadArguments,
    Instantiate(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    I32(i32),
    I64(i64),
}
impl Value {
    fn raw(self) -> u64 {
        match self {
            Value::I32(v) => v as u32 as u64,
            Value::I64(v) => v as u64,
        }
    }
    fn ty(self) -> ValType {
        match self {
            Value::I32(_) => ValType::I32,
            Value::I64(_) => ValType::I64,
        }
    }
    fn from_raw(t: ValType, r: u64) -> Value {
        match t {
            ValType::I32 => Value::I32(r as u32 as i32),
            ValType::I64 => Value::I64(r as i64),
        }
    }
}

// ===================== binary reader with LEB128 =====================
struct Reader<'a> {
    b: &'a [u8], // already cut at the end of the enclosing section/body
    pos: usize,
}

impl<'a> Reader<'a> {
    fn err<T>(&self, msg: impl Into<String>) -> Res<T> {
        Err(Error {
            offset: self.pos,
            msg: msg.into(),
        })
    }
    fn left(&self) -> usize {
        self.b.len() - self.pos
    }
    fn u8(&mut self) -> Res<u8> {
        match self.b.get(self.pos) {
            Some(&x) => {
                self.pos += 1;
                Ok(x)
            }
            None => self.err("unexpected end of input"),
        }
    }
    fn bytes(&mut self, n: usize) -> Res<&'a [u8]> {
        if n > self.left() {
            return self.err(format!("need {n} bytes, only {} left", self.left()));
        }
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    /// Unsigned LEB128 of at most `bits` bits. Unused high bits of the last byte must be zero.
    fn leb_u(&mut self, bits: u32) -> Res<u64> {
        let mut result = 0u64;
        for k in 0..bits.div_ceil(7) {
            let shift = 7 * k;
            let b = self.u8()?;
            let low = (b & 0x7F) as u64;
            if shift + 7 > bits && (b & 0x80 != 0 || (low >> (bits - shift)) != 0) {
                return self.err("integer too large");
            }
            result |= low << shift;
            if b & 0x80 == 0 {
                return Ok(result);
            }
        }
        self.err("integer representation too long")
    }
    /// Signed LEB128: the unused bits of the last byte must replicate the sign bit.
    fn leb_s(&mut self, bits: u32) -> Res<i64> {
        let mut result = 0u64;
        for k in 0..bits.div_ceil(7) {
            let shift = 7 * k;
            let b = self.u8()?;
            let low = (b & 0x7F) as u64;
            result |= low << shift;
            if shift + 7 > bits {
                let rem = bits - shift;
                let high = low >> (rem - 1);
                let ones = (1u64 << (8 - rem)) - 1;
                if b & 0x80 != 0 || (high != 0 && high != ones) {
                    return self.err("integer too large");
                }
            }
            if b & 0x80 == 0 {
                if b & 0x40 != 0 && shift + 7 < 64 {
                    result |= u64::MAX << (shift + 7); // sign-extend
                }
                return Ok(result as i64);
            }
        }
        self.err("integer representation too long")
    }
    fn u32(&mut self) -> Res<u32> {
        Ok(self.leb_u(32)? as u32)
    }
    /// A vector length: every element takes at least one byte, so a longer claim is a lie.
    fn count(&mut self) -> Res<usize> {
        let n = self.u32()? as usize;
        if n > self.left() {
            return self.err("vector length exceeds remaining bytes");
        }
        Ok(n)
    }
    fn name(&mut self) -> Res<String> {
        let n = self.count()?;
        let raw = self.bytes(n)?.to_vec();
        String::from_utf8(raw).or_else(|_| self.err("invalid utf-8 name"))
    }
    fn valtype(&mut self) -> Res<ValType> {
        let b = self.u8()?;
        match ValType::from_byte(b) {
            Some(t) => Ok(t),
            None => Err(Error {
                offset: self.pos - 1,
                msg: format!("unsupported value type {b:#04x}"),
            }),
        }
    }
    fn limits(&mut self) -> Res<(u32, Option<u32>)> {
        let flag = self.u8()?;
        let min = self.u32()?;
        let max = match flag {
            0 => None,
            1 => Some(self.u32()?),
            _ => return self.err("bad limits flag"),
        };
        if max.is_some_and(|m| m < min) {
            return self.err("limits: max < min");
        }
        Ok((min, max))
    }
}

// ===================== module representation =====================
#[derive(Debug, Clone, PartialEq)]
enum Instr {
    Unreachable,
    Nop,
    Block {
        result: Option<ValType>,
        end: u32,
    },
    Loop {
        result: Option<ValType>,
    },
    If {
        result: Option<ValType>,
        else_: u32,
        end: u32,
    },
    Else {
        end: u32,
    },
    End,
    Br(u32),
    BrIf(u32),
    BrTable(Vec<u32>, u32),
    Return,
    Call(u32),
    CallIndirect(u32),
    Drop,
    Select,
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    Load {
        op: u8,
        align: u32,
        offset: u32,
    },
    Store {
        op: u8,
        align: u32,
        offset: u32,
    },
    MemorySize,
    MemoryGrow,
    I32Const(i32),
    I64Const(i64),
    Num(u8), // raw opcode of a numeric instruction
}

#[derive(Debug)]
struct Func {
    type_idx: u32,
    locals: Vec<ValType>,
    body: Vec<Instr>,
    offs: Vec<usize>, // byte offset of each instruction, for validation errors
}
#[derive(Debug)]
struct Global {
    ty: ValType,
    mutable: bool,
    init: u64,
}
#[derive(Debug, Default)]
struct Module {
    types: Vec<FuncType>,
    func_types: Vec<u32>,
    funcs: Vec<Func>,
    table: Option<(u32, Option<u32>)>,
    mem: Option<(u32, Option<u32>)>,
    globals: Vec<Global>,
    exports: Vec<(String, u8, u32)>,
    start: Option<u32>,
    elems: Vec<(u32, Vec<u32>)>,
    data: Vec<(u32, Vec<u8>)>,
}

/// (value type, byte size, sign-extending) for the supported loads.
fn load_info(op: u8) -> Option<(ValType, u32, bool)> {
    use ValType::*;
    Some(match op {
        0x28 => (I32, 4, false),
        0x29 => (I64, 8, false),
        0x2C => (I32, 1, true),
        0x2D => (I32, 1, false),
        0x2E => (I32, 2, true),
        0x2F => (I32, 2, false),
        0x30 => (I64, 1, true),
        0x31 => (I64, 1, false),
        0x32 => (I64, 2, true),
        0x33 => (I64, 2, false),
        0x34 => (I64, 4, true),
        0x35 => (I64, 4, false),
        _ => return None,
    })
}
fn store_info(op: u8) -> Option<(ValType, u32)> {
    use ValType::*;
    Some(match op {
        0x36 => (I32, 4),
        0x37 => (I64, 8),
        0x3A => (I32, 1),
        0x3B => (I32, 2),
        0x3C => (I64, 1),
        0x3D => (I64, 2),
        0x3E => (I64, 4),
        _ => return None,
    })
}
/// (input type, output type, arity) of a numeric opcode; None if unsupported.
fn num_sig(op: u8) -> Option<(ValType, ValType, usize)> {
    use ValType::*;
    Some(match op {
        0x45 | 0x67..=0x69 => (I32, I32, 1),
        0x46..=0x4F | 0x6A..=0x78 => (I32, I32, 2),
        0x50 => (I64, I32, 1),
        0x51..=0x5A => (I64, I32, 2),
        0x79..=0x7B => (I64, I64, 1),
        0x7C..=0x8A => (I64, I64, 2),
        0xA7 => (I64, I32, 1),
        0xAC | 0xAD => (I32, I64, 1),
        _ => return None,
    })
}

// ===================== decoder =====================
const MAX_LOCALS: u64 = 10_000;

fn block_type(r: &mut Reader) -> Res<Option<ValType>> {
    let b = r.u8()?;
    if b == 0x40 {
        return Ok(None);
    }
    match ValType::from_byte(b) {
        Some(t) => Ok(Some(t)),
        None => Err(Error {
            offset: r.pos - 1,
            msg: "unsupported block type".into(),
        }),
    }
}

/// Decode one function body (after its locals) into instructions, matching structured control flow.
fn decode_body(r: &mut Reader) -> Res<(Vec<Instr>, Vec<usize>)> {
    let (mut code, mut offs): (Vec<Instr>, Vec<usize>) = (Vec::new(), Vec::new());
    let mut open: Vec<usize> = Vec::new(); // indexes of unmatched Block/Loop/If
    loop {
        let off = r.pos;
        let op = r.u8()?;
        let idx = code.len() as u32;
        let ins = match op {
            0x00 => Instr::Unreachable,
            0x01 => Instr::Nop,
            0x02 | 0x03 | 0x04 => {
                let result = block_type(r)?;
                open.push(code.len());
                match op {
                    0x02 => Instr::Block { result, end: 0 },
                    0x03 => Instr::Loop { result },
                    _ => Instr::If {
                        result,
                        else_: 0,
                        end: 0,
                    },
                }
            }
            0x05 => {
                let ok = match open.last().map(|&i| &mut code[i]) {
                    Some(Instr::If { else_, .. }) if *else_ == 0 => {
                        *else_ = idx;
                        true
                    }
                    _ => false,
                };
                if !ok {
                    return Err(Error {
                        offset: off,
                        msg: "else without matching if".into(),
                    });
                }
                Instr::Else { end: 0 }
            }
            0x0B => match open.pop() {
                Some(i) => {
                    match &mut code[i] {
                        Instr::Block { end, .. } => *end = idx,
                        Instr::If { else_, end, .. } => {
                            *end = idx;
                            let e = *else_ as usize;
                            if e != 0 {
                                code[e] = Instr::Else { end: idx };
                            }
                        }
                        _ => {}
                    }
                    Instr::End
                }
                None => {
                    code.push(Instr::End); // the function's own end
                    offs.push(off);
                    return Ok((code, offs));
                }
            },
            0x0C => Instr::Br(r.u32()?),
            0x0D => Instr::BrIf(r.u32()?),
            0x0E => {
                let n = r.count()?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(r.u32()?);
                }
                Instr::BrTable(v, r.u32()?)
            }
            0x0F => Instr::Return,
            0x10 => Instr::Call(r.u32()?),
            0x11 => {
                let t = r.u32()?;
                if r.u8()? != 0 {
                    return r.err("call_indirect: only table 0 exists");
                }
                Instr::CallIndirect(t)
            }
            0x1A => Instr::Drop,
            0x1B => Instr::Select,
            0x20 => Instr::LocalGet(r.u32()?),
            0x21 => Instr::LocalSet(r.u32()?),
            0x22 => Instr::LocalTee(r.u32()?),
            0x23 => Instr::GlobalGet(r.u32()?),
            0x24 => Instr::GlobalSet(r.u32()?),
            _ if load_info(op).is_some() => Instr::Load {
                op,
                align: r.u32()?,
                offset: r.u32()?,
            },
            _ if store_info(op).is_some() => Instr::Store {
                op,
                align: r.u32()?,
                offset: r.u32()?,
            },
            0x3F | 0x40 => {
                if r.u8()? != 0 {
                    return r.err("memory index must be 0");
                }
                if op == 0x3F {
                    Instr::MemorySize
                } else {
                    Instr::MemoryGrow
                }
            }
            0x41 => Instr::I32Const(r.leb_s(32)? as i32),
            0x42 => Instr::I64Const(r.leb_s(64)?),
            _ if num_sig(op).is_some() => Instr::Num(op),
            _ => {
                return Err(Error {
                    offset: off,
                    msg: format!("unsupported opcode {op:#04x}"),
                });
            }
        };
        code.push(ins);
        offs.push(off);
    }
}

/// A constant expression: `i32.const v end` or `i64.const v end` of the expected type.
fn const_expr(r: &mut Reader, want: ValType) -> Res<u64> {
    let v = match (r.u8()?, want) {
        (0x41, ValType::I32) => r.leb_s(32)? as i32 as u32 as u64,
        (0x42, ValType::I64) => r.leb_s(64)? as u64,
        _ => return r.err("unsupported or mistyped constant expression"),
    };
    if r.u8()? != 0x0B {
        return r.err("constant expression must end with `end`");
    }
    Ok(v)
}

fn section(id: u8, s: &mut Reader, m: &mut Module) -> Res<()> {
    match id {
        1 => {
            for _ in 0..s.count()? {
                if s.u8()? != 0x60 {
                    return s.err("expected function type tag 0x60");
                }
                let mut ft = FuncType {
                    params: vec![],
                    results: vec![],
                };
                for _ in 0..s.count()? {
                    ft.params.push(s.valtype()?);
                }
                for _ in 0..s.count()? {
                    ft.results.push(s.valtype()?);
                }
                if ft.results.len() > 1 {
                    return s.err("multi-value results are not supported");
                }
                m.types.push(ft);
            }
        }
        2 => return s.err("imports are not supported"),
        3 => {
            for _ in 0..s.count()? {
                m.func_types.push(s.u32()?);
            }
        }
        4 => {
            let n = s.count()?;
            if n > 1 || (n == 1 && m.table.is_some()) {
                return s.err("at most one table");
            }
            if n == 1 {
                if s.u8()? != 0x70 {
                    return s.err("table element type must be funcref");
                }
                m.table = Some(s.limits()?);
            }
        }
        5 => {
            let n = s.count()?;
            if n > 1 {
                return s.err("at most one memory");
            }
            if n == 1 {
                m.mem = Some(s.limits()?);
            }
        }
        6 => {
            for _ in 0..s.count()? {
                let ty = s.valtype()?;
                let mutable = match s.u8()? {
                    0 => false,
                    1 => true,
                    _ => return s.err("bad mutability"),
                };
                let init = const_expr(s, ty)?;
                m.globals.push(Global { ty, mutable, init });
            }
        }
        7 => {
            for _ in 0..s.count()? {
                let name = s.name()?;
                let kind = s.u8()?;
                let idx = s.u32()?;
                m.exports.push((name, kind, idx));
            }
        }
        8 => m.start = Some(s.u32()?),
        9 => {
            for _ in 0..s.count()? {
                if s.u32()? != 0 {
                    return s.err("only active element segments for table 0");
                }
                let off = const_expr(s, ValType::I32)? as u32;
                let mut fs = Vec::new();
                for _ in 0..s.count()? {
                    fs.push(s.u32()?);
                }
                m.elems.push((off, fs));
            }
        }
        10 => {
            if s.count()? != m.func_types.len() {
                return s.err("function and code section lengths differ");
            }
            for i in 0..m.func_types.len() {
                let size = s.u32()? as usize;
                if size > s.left() {
                    return s.err("function body exceeds section");
                }
                let end = s.pos + size;
                let mut b = Reader {
                    b: &s.b[..end],
                    pos: s.pos,
                };
                let (mut locals, mut total) = (Vec::new(), 0u64);
                for _ in 0..b.count()? {
                    let n = b.u32()?;
                    let t = b.valtype()?;
                    total += n as u64;
                    if total > MAX_LOCALS {
                        return b.err("too many locals");
                    }
                    locals.extend(std::iter::repeat_n(t, n as usize));
                }
                let (body, offs) = decode_body(&mut b)?;
                if b.left() != 0 {
                    return b.err("bytes after function end");
                }
                s.pos = end;
                m.funcs.push(Func {
                    type_idx: m.func_types[i],
                    locals,
                    body,
                    offs,
                });
            }
        }
        11 => {
            for _ in 0..s.count()? {
                if s.u32()? != 0 {
                    return s.err("only active data segments for memory 0");
                }
                let off = const_expr(s, ValType::I32)? as u32;
                let n = s.count()?;
                m.data.push((off, s.bytes(n)?.to_vec()));
            }
        }
        _ => return s.err(format!("unsupported section id {id}")),
    }
    Ok(())
}

fn decode(bytes: &[u8]) -> Res<Module> {
    if bytes.len() < 4 || &bytes[..4] != b"\0asm" {
        return Err(Error {
            offset: 0,
            msg: "bad magic".into(),
        });
    }
    if bytes.len() < 8 || bytes[4..8] != [1, 0, 0, 0] {
        return Err(Error {
            offset: 4,
            msg: "bad or truncated version".into(),
        });
    }
    let mut r = Reader { b: bytes, pos: 8 };
    let mut m = Module::default();
    let mut last = 0u8;
    while r.left() > 0 {
        let id = r.u8()?;
        let size = r.u32()? as usize;
        if size > r.left() {
            return r.err("section size exceeds module");
        }
        let end = r.pos + size;
        if id != 0 {
            if id <= last {
                return r.err("section out of order or duplicated");
            }
            last = id;
            let mut s = Reader {
                b: &bytes[..end],
                pos: r.pos,
            };
            section(id, &mut s, &mut m)?;
            if s.left() != 0 {
                return s.err("section size mismatch");
            }
        }
        r.pos = end; // custom sections are skipped
    }
    if m.funcs.len() != m.func_types.len() {
        return r.err("function section without code section");
    }
    Ok(m)
}

// ===================== validator =====================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Func,
    Block,
    Loop,
    If,
}
#[derive(Clone)]
struct Frame {
    kind: Kind,
    result: Option<ValType>,
    height: usize,
    unreachable: bool,
    has_else: bool,
}
type VR = Result<(), String>;

struct V<'a> {
    m: &'a Module,
    vals: Vec<Option<ValType>>, // None = unknown type (polymorphic stack after unreachable code)
    ctrls: Vec<Frame>,
    locals: Vec<ValType>,
}

impl V<'_> {
    fn top(&self) -> Result<&Frame, String> {
        self.ctrls
            .last()
            .ok_or_else(|| "empty control stack".to_string())
    }
    fn pop(&mut self, expect: Option<ValType>) -> Result<Option<ValType>, String> {
        let fr = self.top()?;
        if self.vals.len() == fr.height {
            return if fr.unreachable {
                Ok(expect)
            } else {
                Err("operand stack underflow".into())
            };
        }
        let actual = self.vals.pop().unwrap();
        if let (Some(a), Some(e)) = (actual, expect) {
            if a != e {
                return Err(format!("type mismatch: expected {e:?}, found {a:?}"));
            }
        }
        Ok(actual.or(expect))
    }
    fn pop_t(&mut self, t: ValType) -> VR {
        self.pop(Some(t)).map(|_| ())
    }
    fn set_unreachable(&mut self) -> VR {
        let h = self.top()?.height;
        self.vals.truncate(h);
        self.ctrls.last_mut().unwrap().unreachable = true;
        Ok(())
    }
    /// Types a branch to `depth` must supply: nothing for a loop (it re-enters), the result otherwise.
    fn label_type(&self, depth: u32) -> Result<Option<ValType>, String> {
        let i = self
            .ctrls
            .len()
            .checked_sub(1 + depth as usize)
            .ok_or("unknown label")?;
        let f = &self.ctrls[i];
        Ok(if f.kind == Kind::Loop { None } else { f.result })
    }
    fn local(&self, i: u32) -> Result<ValType, String> {
        self.locals
            .get(i as usize)
            .copied()
            .ok_or_else(|| format!("unknown local {i}"))
    }
    fn call_sig(&mut self, ty: &FuncType) -> VR {
        for p in ty.params.iter().rev() {
            self.pop_t(*p)?;
        }
        self.vals.extend(ty.results.iter().map(|&t| Some(t)));
        Ok(())
    }
    fn step(&mut self, ins: &Instr) -> VR {
        match ins {
            Instr::Unreachable => self.set_unreachable()?,
            Instr::Nop => {}
            Instr::Block { result, .. } | Instr::Loop { result } | Instr::If { result, .. } => {
                let kind = match ins {
                    Instr::Block { .. } => Kind::Block,
                    Instr::Loop { .. } => Kind::Loop,
                    _ => {
                        self.pop_t(ValType::I32)?;
                        Kind::If
                    }
                };
                self.ctrls.push(Frame {
                    kind,
                    result: *result,
                    height: self.vals.len(),
                    unreachable: false,
                    has_else: false,
                });
            }
            Instr::Else { .. } => {
                let fr = self.top()?.clone();
                if fr.kind != Kind::If || fr.has_else {
                    return Err("else without if".into());
                }
                if let Some(t) = fr.result {
                    self.pop_t(t)?;
                }
                if self.vals.len() != fr.height {
                    return Err("values remaining on stack at else".into());
                }
                let f = self.ctrls.last_mut().unwrap();
                f.has_else = true;
                f.unreachable = false;
            }
            Instr::End => {
                let fr = self.top()?.clone();
                if let Some(t) = fr.result {
                    self.pop_t(t)?;
                }
                if self.vals.len() != fr.height {
                    return Err("values remaining on stack at end of block".into());
                }
                if fr.kind == Kind::If && !fr.has_else && fr.result.is_some() {
                    return Err("if with a result requires an else branch".into());
                }
                self.ctrls.pop();
                if let (Some(t), true) = (fr.result, fr.kind != Kind::Func) {
                    self.vals.push(Some(t)); // None here would mean "unknown type", not "no value"
                }
            }
            Instr::Br(d) => {
                if let Some(t) = self.label_type(*d)? {
                    self.pop_t(t)?;
                }
                self.set_unreachable()?;
            }
            Instr::BrIf(d) => {
                self.pop_t(ValType::I32)?;
                let lt = self.label_type(*d)?;
                if let Some(t) = lt {
                    self.pop_t(t)?;
                    self.vals.push(Some(t));
                }
            }
            Instr::BrTable(ls, d) => {
                self.pop_t(ValType::I32)?;
                let dt = self.label_type(*d)?;
                for &l in ls {
                    if self.label_type(l)? != dt {
                        return Err("br_table targets have different types".into());
                    }
                }
                if let Some(t) = dt {
                    self.pop_t(t)?;
                }
                self.set_unreachable()?;
            }
            Instr::Return => {
                if let Some(t) = self.ctrls[0].result {
                    self.pop_t(t)?;
                }
                self.set_unreachable()?;
            }
            Instr::Call(f) => {
                let ti = *self
                    .m
                    .func_types
                    .get(*f as usize)
                    .ok_or_else(|| format!("unknown function {f}"))?;
                let ty = self.m.types[ti as usize].clone();
                self.call_sig(&ty)?;
            }
            Instr::CallIndirect(t) => {
                if self.m.table.is_none() {
                    return Err("call_indirect without a table".into());
                }
                let ty = self.m.types.get(*t as usize).ok_or("unknown type")?.clone();
                self.pop_t(ValType::I32)?;
                self.call_sig(&ty)?;
            }
            Instr::Drop => {
                self.pop(None)?;
            }
            Instr::Select => {
                self.pop_t(ValType::I32)?;
                let a = self.pop(None)?;
                let b = self.pop(a)?;
                self.vals.push(b.or(a));
            }
            Instr::LocalGet(i) => {
                let t = self.local(*i)?;
                self.vals.push(Some(t));
            }
            Instr::LocalSet(i) => {
                let t = self.local(*i)?;
                self.pop_t(t)?;
            }
            Instr::LocalTee(i) => {
                let t = self.local(*i)?;
                self.pop_t(t)?;
                self.vals.push(Some(t));
            }
            Instr::GlobalGet(i) => {
                let g = self.m.globals.get(*i as usize).ok_or("unknown global")?;
                self.vals.push(Some(g.ty));
            }
            Instr::GlobalSet(i) => {
                let g = self.m.globals.get(*i as usize).ok_or("unknown global")?;
                if !g.mutable {
                    return Err("global.set on an immutable global".into());
                }
                self.pop_t(g.ty)?;
            }
            Instr::Load { op, align, .. } | Instr::Store { op, align, .. } => {
                if self.m.mem.is_none() {
                    return Err("memory instruction without a memory".into());
                }
                let is_load = matches!(ins, Instr::Load { .. });
                let (ty, size) = match is_load {
                    true => load_info(*op).map(|(t, s, _)| (t, s)),
                    false => store_info(*op),
                }
                .unwrap();
                if *align > 3 || (1u32 << align) > size {
                    return Err("alignment must not be larger than natural".into());
                }
                if is_load {
                    self.pop_t(ValType::I32)?;
                    self.vals.push(Some(ty));
                } else {
                    self.pop_t(ty)?;
                    self.pop_t(ValType::I32)?;
                }
            }
            Instr::MemorySize | Instr::MemoryGrow => {
                if self.m.mem.is_none() {
                    return Err("memory instruction without a memory".into());
                }
                if matches!(ins, Instr::MemoryGrow) {
                    self.pop_t(ValType::I32)?;
                }
                self.vals.push(Some(ValType::I32));
            }
            Instr::I32Const(_) => self.vals.push(Some(ValType::I32)),
            Instr::I64Const(_) => self.vals.push(Some(ValType::I64)),
            Instr::Num(op) => {
                let (inp, out, n) = num_sig(*op).unwrap();
                for _ in 0..n {
                    self.pop_t(inp)?;
                }
                self.vals.push(Some(out));
            }
        }
        Ok(())
    }
}

fn validate(m: &Module) -> Res<()> {
    let e = |msg: &str| {
        Err(Error {
            offset: 0,
            msg: msg.to_string(),
        })
    };
    if m.func_types.iter().any(|&t| t as usize >= m.types.len()) {
        return e("function refers to unknown type");
    }
    if m.mem
        .is_some_and(|(min, max)| min > 65536 || max.is_some_and(|x| x > 65536))
    {
        return e("memory size must be at most 65536 pages");
    }
    for (_, kind, idx) in &m.exports {
        let n = match kind {
            0 => m.funcs.len(),
            1 => m.table.is_some() as usize,
            2 => m.mem.is_some() as usize,
            3 => m.globals.len(),
            _ => return e("unknown export kind"),
        };
        if *idx as usize >= n {
            return e("export index out of range");
        }
    }
    if let Some(s) = m.start {
        match m.func_types.get(s as usize) {
            Some(&t)
                if m.types[t as usize]
                    == (FuncType {
                        params: vec![],
                        results: vec![],
                    }) => {}
            _ => return e("start function must exist and have type [] -> []"),
        }
    }
    if !m.elems.is_empty() && m.table.is_none() {
        return e("element segment without a table");
    }
    if m.elems
        .iter()
        .flat_map(|(_, fs)| fs)
        .any(|&f| f as usize >= m.funcs.len())
    {
        return e("element refers to unknown function");
    }
    if !m.data.is_empty() && m.mem.is_none() {
        return e("data segment without a memory");
    }
    for f in &m.funcs {
        let ty = &m.types[f.type_idx as usize];
        let mut v = V {
            m,
            vals: vec![],
            ctrls: vec![],
            locals: ty.params.iter().chain(&f.locals).copied().collect(),
        };
        v.ctrls.push(Frame {
            kind: Kind::Func,
            result: ty.results.first().copied(),
            height: 0,
            unreachable: false,
            has_else: false,
        });
        for (pc, ins) in f.body.iter().enumerate() {
            v.step(ins).map_err(|msg| Error {
                offset: f.offs[pc],
                msg,
            })?;
        }
        if !v.ctrls.is_empty() {
            return Err(Error {
                offset: *f.offs.last().unwrap_or(&0),
                msg: "unterminated block".into(),
            });
        }
    }
    Ok(())
}

// ===================== interpreter =====================
const PAGE: usize = 65536;
const MAX_PAGES: u32 = 64; // this interpreter's own cap (4 MiB), so hostile modules cannot exhaust memory
const MAX_DEPTH: u32 = 200;

#[derive(Clone, Copy)]
struct Label {
    cont: usize,   // pc to continue at after a branch to this label
    height: usize, // value-stack height at entry
    arity: u32,    // values a branch carries (0 for loops)
    is_loop: bool,
}

struct Instance<'m> {
    m: &'m Module,
    mem: Vec<u8>,
    globals: Vec<u64>,
    table: Vec<Option<u32>>,
    stack: Vec<u64>,
    labels: Vec<Label>,
    fuel: u64,
    budget: u64,
}

fn cmp_result(k: u8, s: Ordering, u: Ordering) -> bool {
    use Ordering::*;
    match k {
        0 => u == Equal,
        1 => u != Equal,
        2 => s == Less,
        3 => u == Less,
        4 => s == Greater,
        5 => u == Greater,
        6 => s != Greater,
        7 => u != Greater,
        8 => s != Less,
        _ => u != Less,
    }
}

fn bin32(op: u8, a: u32, b: u32) -> Result<u32, Trap> {
    let (x, y) = (a as i32, b as i32);
    Ok(match op {
        0x6A => a.wrapping_add(b),
        0x6B => a.wrapping_sub(b),
        0x6C => a.wrapping_mul(b),
        0x6D => match (b, x == i32::MIN && y == -1) {
            (0, _) => return Err(Trap::DivByZero),
            (_, true) => return Err(Trap::IntOverflow),
            _ => (x / y) as u32,
        },
        0x6E => a.checked_div(b).ok_or(Trap::DivByZero)?,
        0x6F => match b {
            0 => return Err(Trap::DivByZero),
            _ => x.wrapping_rem(y) as u32, // MIN % -1 == 0, no trap
        },
        0x70 => a.checked_rem(b).ok_or(Trap::DivByZero)?,
        0x71 => a & b,
        0x72 => a | b,
        0x73 => a ^ b,
        0x74 => a.wrapping_shl(b), // wrapping_shl masks the count by width-1: "count mod 32"
        0x75 => x.wrapping_shr(b) as u32,
        0x76 => a.wrapping_shr(b),
        0x77 => a.rotate_left(b % 32),
        _ => a.rotate_right(b % 32),
    })
}

fn bin64(op: u8, a: u64, b: u64) -> Result<u64, Trap> {
    let (x, y) = (a as i64, b as i64);
    let c = b as u32;
    Ok(match op {
        0x7C => a.wrapping_add(b),
        0x7D => a.wrapping_sub(b),
        0x7E => a.wrapping_mul(b),
        0x7F => match (b, x == i64::MIN && y == -1) {
            (0, _) => return Err(Trap::DivByZero),
            (_, true) => return Err(Trap::IntOverflow),
            _ => (x / y) as u64,
        },
        0x80 => a.checked_div(b).ok_or(Trap::DivByZero)?,
        0x81 => match b {
            0 => return Err(Trap::DivByZero),
            _ => x.wrapping_rem(y) as u64,
        },
        0x82 => a.checked_rem(b).ok_or(Trap::DivByZero)?,
        0x83 => a & b,
        0x84 => a | b,
        0x85 => a ^ b,
        0x86 => a.wrapping_shl(c),
        0x87 => x.wrapping_shr(c) as u64,
        0x88 => a.wrapping_shr(c),
        0x89 => a.rotate_left((b % 64) as u32),
        _ => a.rotate_right((b % 64) as u32),
    })
}

impl<'m> Instance<'m> {
    fn new(m: &'m Module) -> Result<Self, Trap> {
        let mem_pages = m.mem.map_or(0, |(min, _)| min);
        if mem_pages > MAX_PAGES {
            return Err(Trap::Instantiate(
                "memory larger than this interpreter allows",
            ));
        }
        let table_len = m.table.map_or(0, |(min, _)| min);
        if table_len > 10_000 {
            return Err(Trap::Instantiate("table too large"));
        }
        let mut inst = Instance {
            m,
            mem: vec![0; mem_pages as usize * PAGE],
            globals: m.globals.iter().map(|g| g.init).collect(),
            table: vec![None; table_len as usize],
            stack: Vec::new(),
            labels: Vec::new(),
            fuel: 0,
            budget: 2_000_000,
        };
        for (off, fs) in &m.elems {
            let off = *off as usize;
            if off + fs.len() > inst.table.len() {
                return Err(Trap::Instantiate("element segment does not fit the table"));
            }
            for (i, f) in fs.iter().enumerate() {
                inst.table[off + i] = Some(*f);
            }
        }
        for (off, bytes) in &m.data {
            let off = *off as usize;
            if off + bytes.len() > inst.mem.len() {
                return Err(Trap::Instantiate("data segment does not fit the memory"));
            }
            inst.mem[off..off + bytes.len()].copy_from_slice(bytes);
        }
        if let Some(s) = m.start {
            inst.fuel = inst.budget;
            inst.invoke(s, 0)?;
        }
        Ok(inst)
    }
    fn pop(&mut self) -> u64 {
        self.stack.pop().expect("validated code never underflows")
    }
    fn pop32(&mut self) -> u32 {
        self.pop() as u32
    }
    fn push32(&mut self, v: u32) {
        self.stack.push(v as u64);
    }
    /// Effective address check in u64 so `addr + offset` can never wrap around.
    fn ea(&self, addr: u32, offset: u32, size: u32) -> Result<usize, Trap> {
        let ea = addr as u64 + offset as u64;
        if ea + size as u64 > self.mem.len() as u64 {
            return Err(Trap::OutOfBoundsMemory);
        }
        Ok(ea as usize)
    }
    /// Branch to the label `depth` levels out: carry its arity values down to its height, pop labels, jump.
    fn branch(&mut self, depth: u32) -> usize {
        let idx = self.labels.len() - 1 - depth as usize;
        let l = self.labels[idx];
        let top = self.stack.len();
        let ar = l.arity as usize;
        self.stack.copy_within(top - ar..top, l.height);
        self.stack.truncate(l.height + ar);
        self.labels.truncate(if l.is_loop { idx + 1 } else { idx });
        l.cont
    }
    fn invoke(&mut self, fidx: u32, depth: u32) -> Result<(), Trap> {
        if depth >= MAX_DEPTH {
            return Err(Trap::CallStackExhausted);
        }
        let m = self.m; // copy of the shared reference: borrowing `m` does not borrow `self`
        let func = &m.funcs[fidx as usize];
        let ty = &m.types[func.type_idx as usize];
        let nres = ty.results.len();
        let base = self.stack.len() - ty.params.len(); // arguments are already the first locals
        self.stack
            .resize(base + ty.params.len() + func.locals.len(), 0);
        let lb = self.labels.len();
        let body = &func.body;
        self.labels.push(Label {
            cont: body.len(),
            height: self.stack.len(),
            arity: nres as u32,
            is_loop: false,
        });
        let mut pc = 0usize;
        while pc < body.len() {
            if self.fuel == 0 {
                return Err(Trap::OutOfFuel);
            }
            self.fuel -= 1;
            match &body[pc] {
                Instr::Unreachable => return Err(Trap::Unreachable),
                Instr::Nop => pc += 1,
                Instr::Block { result, end } => {
                    let l = Label {
                        cont: *end as usize + 1,
                        height: self.stack.len(),
                        arity: result.is_some() as u32,
                        is_loop: false,
                    };
                    self.labels.push(l);
                    pc += 1;
                }
                Instr::Loop { .. } => {
                    self.labels.push(Label {
                        cont: pc + 1,
                        height: self.stack.len(),
                        arity: 0,
                        is_loop: true,
                    });
                    pc += 1;
                }
                Instr::If { result, else_, end } => {
                    let c = self.pop32();
                    let l = Label {
                        cont: *end as usize + 1,
                        height: self.stack.len(),
                        arity: result.is_some() as u32,
                        is_loop: false,
                    };
                    self.labels.push(l);
                    pc = if c != 0 {
                        pc + 1
                    } else if *else_ != 0 {
                        *else_ as usize + 1
                    } else {
                        *end as usize // jump to End, which pops the label
                    };
                }
                Instr::Else { end } => pc = *end as usize,
                Instr::End => {
                    self.labels.pop();
                    pc += 1;
                }
                Instr::Br(d) => pc = self.branch(*d),
                Instr::BrIf(d) => {
                    pc = if self.pop32() != 0 {
                        self.branch(*d)
                    } else {
                        pc + 1
                    }
                }
                Instr::BrTable(ls, d) => {
                    let i = self.pop32() as usize;
                    pc = self.branch(*ls.get(i).unwrap_or(d));
                }
                Instr::Return => pc = self.branch((self.labels.len() - 1 - lb) as u32),
                Instr::Call(f) => {
                    self.invoke(*f, depth + 1)?;
                    pc += 1;
                }
                Instr::CallIndirect(t) => {
                    let i = self.pop32() as usize;
                    let f = self
                        .table
                        .get(i)
                        .ok_or(Trap::UndefinedElement)?
                        .ok_or(Trap::UninitializedElement)?;
                    if m.types[m.func_types[f as usize] as usize] != m.types[*t as usize] {
                        return Err(Trap::IndirectCallTypeMismatch);
                    }
                    self.invoke(f, depth + 1)?;
                    pc += 1;
                }
                Instr::Drop => {
                    self.pop();
                    pc += 1;
                }
                Instr::Select => {
                    let c = self.pop32();
                    let b = self.pop();
                    let a = self.pop();
                    self.stack.push(if c != 0 { a } else { b });
                    pc += 1;
                }
                Instr::LocalGet(i) => {
                    self.stack.push(self.stack[base + *i as usize]);
                    pc += 1;
                }
                Instr::LocalSet(i) => {
                    let v = self.pop();
                    self.stack[base + *i as usize] = v;
                    pc += 1;
                }
                Instr::LocalTee(i) => {
                    self.stack[base + *i as usize] = *self.stack.last().unwrap();
                    pc += 1;
                }
                Instr::GlobalGet(i) => {
                    self.stack.push(self.globals[*i as usize]);
                    pc += 1;
                }
                Instr::GlobalSet(i) => {
                    self.globals[*i as usize] = self.pop();
                    pc += 1;
                }
                Instr::Load { op, offset, .. } => {
                    let (ty, size, signed) = load_info(*op).unwrap();
                    let a = self.pop32();
                    let ea = self.ea(a, *offset, size)?;
                    let mut buf = [0u8; 8];
                    buf[..size as usize].copy_from_slice(&self.mem[ea..ea + size as usize]);
                    let mut v = u64::from_le_bytes(buf);
                    if signed {
                        let sh = 64 - 8 * size;
                        v = (((v << sh) as i64) >> sh) as u64;
                    }
                    self.stack.push(if ty == ValType::I32 {
                        v as u32 as u64
                    } else {
                        v
                    });
                    pc += 1;
                }
                Instr::Store { op, offset, .. } => {
                    let (_, size) = store_info(*op).unwrap();
                    let v = self.pop();
                    let a = self.pop32();
                    let ea = self.ea(a, *offset, size)?;
                    self.mem[ea..ea + size as usize]
                        .copy_from_slice(&v.to_le_bytes()[..size as usize]);
                    pc += 1;
                }
                Instr::MemorySize => {
                    self.push32((self.mem.len() / PAGE) as u32);
                    pc += 1;
                }
                Instr::MemoryGrow => {
                    let n = self.pop32() as u64;
                    let old = (self.mem.len() / PAGE) as u64;
                    let limit = m
                        .mem
                        .and_then(|(_, max)| max)
                        .unwrap_or(MAX_PAGES)
                        .min(MAX_PAGES) as u64;
                    if old + n > limit {
                        self.push32(u32::MAX); // -1: growth failed, memory unchanged
                    } else {
                        self.mem.resize((old + n) as usize * PAGE, 0);
                        self.push32(old as u32);
                    }
                    pc += 1;
                }
                Instr::I32Const(v) => {
                    self.push32(*v as u32);
                    pc += 1;
                }
                Instr::I64Const(v) => {
                    self.stack.push(*v as u64);
                    pc += 1;
                }
                Instr::Num(op) => {
                    self.numeric(*op)?;
                    pc += 1;
                }
            }
        }
        // Results sit on top; slide them down over the locals and drop the frame.
        let start = self.stack.len() - nres;
        self.stack.copy_within(start.., base);
        self.stack.truncate(base + nres);
        self.labels.truncate(lb);
        Ok(())
    }
    fn numeric(&mut self, op: u8) -> Result<(), Trap> {
        match op {
            0x45 => {
                let a = self.pop32();
                self.push32((a == 0) as u32);
            }
            0x46..=0x4F => {
                let (b, a) = (self.pop32(), self.pop32());
                self.push32(cmp_result(op - 0x46, (a as i32).cmp(&(b as i32)), a.cmp(&b)) as u32);
            }
            0x50 => {
                let a = self.pop();
                self.push32((a == 0) as u32);
            }
            0x51..=0x5A => {
                let (b, a) = (self.pop(), self.pop());
                self.push32(cmp_result(op - 0x51, (a as i64).cmp(&(b as i64)), a.cmp(&b)) as u32);
            }
            0x67..=0x69 => {
                let a = self.pop32();
                self.push32(match op {
                    0x67 => a.leading_zeros(),
                    0x68 => a.trailing_zeros(),
                    _ => a.count_ones(),
                });
            }
            0x6A..=0x78 => {
                let (b, a) = (self.pop32(), self.pop32());
                let r = bin32(op, a, b)?;
                self.push32(r);
            }
            0x79..=0x7B => {
                let a = self.pop();
                self.stack.push(match op {
                    0x79 => a.leading_zeros(),
                    0x7A => a.trailing_zeros(),
                    _ => a.count_ones(),
                } as u64);
            }
            0x7C..=0x8A => {
                let (b, a) = (self.pop(), self.pop());
                let r = bin64(op, a, b)?;
                self.stack.push(r);
            }
            0xA7 => {
                let a = self.pop();
                self.push32(a as u32); // i32.wrap_i64
            }
            0xAC => {
                let a = self.pop32();
                self.stack.push(a as i32 as i64 as u64); // extend_s
            }
            _ => {
                let a = self.pop32();
                self.stack.push(a as u64); // extend_u
            }
        }
        Ok(())
    }
    fn call_export(&mut self, name: &str, args: &[Value]) -> Result<Vec<Value>, Trap> {
        let m = self.m;
        let fidx = m
            .exports
            .iter()
            .find(|e| e.0 == name && e.1 == 0)
            .map(|e| e.2)
            .ok_or(Trap::NoSuchExport)?;
        let ty = &m.types[m.funcs[fidx as usize].type_idx as usize];
        if args.len() != ty.params.len() || args.iter().zip(&ty.params).any(|(a, p)| a.ty() != *p) {
            return Err(Trap::BadArguments);
        }
        self.stack.clear(); // a previous trap may have left garbage
        self.labels.clear();
        self.fuel = self.budget;
        self.stack.extend(args.iter().map(|a| a.raw()));
        self.invoke(fidx, 0)?;
        Ok(ty
            .results
            .iter()
            .zip(&self.stack)
            .map(|(&t, &r)| Value::from_raw(t, r))
            .collect())
    }
}

// ===================== assembler =====================
const I32: u8 = 0x7F;
const I64: u8 = 0x7E;
const VOID: u8 = 0x40;

fn uleb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}
fn sleb(out: &mut Vec<u8>, mut v: i64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7; // arithmetic shift
        if (v == 0 && b & 0x40 == 0) || (v == -1 && b & 0x40 != 0) {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Instruction emitter with chaining: `a.local_get(0).i32c(1).op(I32_ADD)`.
#[derive(Default, Clone)]
struct Asm(Vec<u8>);
impl Asm {
    fn op(&mut self, b: u8) -> &mut Self {
        self.0.push(b);
        self
    }
    fn imm(&mut self, b: u8, v: u32) -> &mut Self {
        self.0.push(b);
        uleb(&mut self.0, v as u64);
        self
    }
    fn i32c(&mut self, v: i32) -> &mut Self {
        self.0.push(0x41);
        sleb(&mut self.0, v as i64);
        self
    }
    fn i64c(&mut self, v: i64) -> &mut Self {
        self.0.push(0x42);
        sleb(&mut self.0, v);
        self
    }
    fn local_get(&mut self, i: u32) -> &mut Self {
        self.imm(0x20, i)
    }
    fn local_set(&mut self, i: u32) -> &mut Self {
        self.imm(0x21, i)
    }
    fn local_tee(&mut self, i: u32) -> &mut Self {
        self.imm(0x22, i)
    }
    fn global_get(&mut self, i: u32) -> &mut Self {
        self.imm(0x23, i)
    }
    fn global_set(&mut self, i: u32) -> &mut Self {
        self.imm(0x24, i)
    }
    fn block(&mut self, bt: u8) -> &mut Self {
        self.op(0x02).op(bt)
    }
    fn loop_(&mut self, bt: u8) -> &mut Self {
        self.op(0x03).op(bt)
    }
    fn if_(&mut self, bt: u8) -> &mut Self {
        self.op(0x04).op(bt)
    }
    fn else_(&mut self) -> &mut Self {
        self.op(0x05)
    }
    fn end(&mut self) -> &mut Self {
        self.op(0x0B)
    }
    fn br(&mut self, d: u32) -> &mut Self {
        self.imm(0x0C, d)
    }
    fn br_if(&mut self, d: u32) -> &mut Self {
        self.imm(0x0D, d)
    }
    fn br_table(&mut self, ls: &[u32], d: u32) -> &mut Self {
        self.op(0x0E);
        uleb(&mut self.0, ls.len() as u64);
        for &l in ls {
            uleb(&mut self.0, l as u64);
        }
        uleb(&mut self.0, d as u64);
        self
    }
    fn call(&mut self, f: u32) -> &mut Self {
        self.imm(0x10, f)
    }
    fn call_indirect(&mut self, t: u32) -> &mut Self {
        self.imm(0x11, t).op(0)
    }
    fn mem(&mut self, op: u8, align: u32, offset: u32) -> &mut Self {
        self.op(op);
        uleb(&mut self.0, align as u64);
        uleb(&mut self.0, offset as u64);
        self
    }
}

fn section_bytes(out: &mut Vec<u8>, id: u8, payload: &[u8]) {
    out.push(id);
    uleb(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

#[derive(Default)]
struct ModuleBuilder {
    types: Vec<(Vec<u8>, Vec<u8>)>,
    funcs: Vec<(u32, Vec<u8>, Vec<u8>)>, // type, local types, body
    mem: Option<(u32, Option<u32>)>,
    table: Option<u32>,
    globals: Vec<(u8, bool, i64)>,
    exports: Vec<(String, u8, u32)>,
    start: Option<u32>,
    elems: Vec<(u32, Vec<u32>)>,
    data: Vec<(u32, Vec<u8>)>,
}

impl ModuleBuilder {
    fn type_(&mut self, p: &[u8], r: &[u8]) -> u32 {
        let t = (p.to_vec(), r.to_vec());
        match self.types.iter().position(|x| *x == t) {
            Some(i) => i as u32,
            None => {
                self.types.push(t);
                self.types.len() as u32 - 1
            }
        }
    }
    fn next_func(&self) -> u32 {
        self.funcs.len() as u32
    }
    /// Add a function; the final `end` is appended for you. Returns its function index.
    fn func(&mut self, p: &[u8], r: &[u8], locals: &[u8], body: &Asm) -> u32 {
        let t = self.type_(p, r);
        let mut b = body.0.clone();
        b.push(0x0B);
        self.funcs.push((t, locals.to_vec(), b));
        self.funcs.len() as u32 - 1
    }
    fn export(&mut self, name: &str, f: u32) -> &mut Self {
        self.exports.push((name.to_string(), 0, f));
        self
    }
    fn build(&self) -> Vec<u8> {
        let mut out = b"\0asm\x01\0\0\0".to_vec();
        let vec_of = |items: Vec<Vec<u8>>| {
            let mut v = Vec::new();
            uleb(&mut v, items.len() as u64);
            items.into_iter().for_each(|i| v.extend(i));
            v
        };
        let limits = |(min, max): (u32, Option<u32>)| {
            let mut v = vec![max.is_some() as u8];
            uleb(&mut v, min as u64);
            if let Some(x) = max {
                uleb(&mut v, x as u64);
            }
            v
        };
        let expr = |ty: u8, v: i64| {
            let mut e = vec![if ty == I32 { 0x41 } else { 0x42 }];
            sleb(&mut e, v);
            e.push(0x0B);
            e
        };
        let types = self.types.iter().map(|(p, r)| {
            let mut v = vec![0x60];
            uleb(&mut v, p.len() as u64);
            v.extend(p);
            uleb(&mut v, r.len() as u64);
            v.extend(r);
            v
        });
        section_bytes(&mut out, 1, &vec_of(types.collect()));
        let ftypes = self.funcs.iter().map(|f| {
            let mut v = Vec::new();
            uleb(&mut v, f.0 as u64);
            v
        });
        section_bytes(&mut out, 3, &vec_of(ftypes.collect()));
        if let Some(n) = self.table {
            let mut t = vec![0x70];
            t.extend(limits((n, None)));
            section_bytes(&mut out, 4, &vec_of(vec![t]));
        }
        if let Some(l) = self.mem {
            section_bytes(&mut out, 5, &vec_of(vec![limits(l)]));
        }
        if !self.globals.is_empty() {
            let gs = self.globals.iter().map(|&(t, m, v)| {
                let mut g = vec![t, m as u8];
                g.extend(expr(t, v));
                g
            });
            section_bytes(&mut out, 6, &vec_of(gs.collect()));
        }
        if !self.exports.is_empty() {
            let es = self.exports.iter().map(|(n, k, i)| {
                let mut e = Vec::new();
                uleb(&mut e, n.len() as u64);
                e.extend(n.as_bytes());
                e.push(*k);
                uleb(&mut e, *i as u64);
                e
            });
            section_bytes(&mut out, 7, &vec_of(es.collect()));
        }
        if let Some(s) = self.start {
            let mut v = Vec::new();
            uleb(&mut v, s as u64);
            section_bytes(&mut out, 8, &v);
        }
        if !self.elems.is_empty() {
            let es = self.elems.iter().map(|(off, fs)| {
                let mut e = vec![0];
                e.extend(expr(I32, *off as i64));
                uleb(&mut e, fs.len() as u64);
                fs.iter().for_each(|&f| uleb(&mut e, f as u64));
                e
            });
            section_bytes(&mut out, 9, &vec_of(es.collect()));
        }
        let codes = self.funcs.iter().map(|(_, locals, body)| {
            let mut groups: Vec<(u32, u8)> = Vec::new();
            for &l in locals {
                match groups.last_mut() {
                    Some((n, t)) if *t == l => *n += 1,
                    _ => groups.push((1, l)),
                }
            }
            let mut c = Vec::new();
            uleb(&mut c, groups.len() as u64);
            for (n, t) in groups {
                uleb(&mut c, n as u64);
                c.push(t);
            }
            c.extend(body);
            let mut e = Vec::new();
            uleb(&mut e, c.len() as u64);
            e.extend(c);
            e
        });
        section_bytes(&mut out, 10, &vec_of(codes.collect()));
        if !self.data.is_empty() {
            let ds = self.data.iter().map(|(off, bytes)| {
                let mut d = vec![0];
                d.extend(expr(I32, *off as i64));
                uleb(&mut d, bytes.len() as u64);
                d.extend(bytes);
                d
            });
            section_bytes(&mut out, 11, &vec_of(ds.collect()));
        }
        out
    }
}

// opcodes used by the hand-written tests
const I32_EQZ: u8 = 0x45;
const I32_LT_S: u8 = 0x48;
const I32_ADD: u8 = 0x6A;
const I32_SUB: u8 = 0x6B;
const I32_DIV_S: u8 = 0x6D;
const I32_DIV_U: u8 = 0x6E;
const I32_REM_S: u8 = 0x6F;
const I32_REM_U: u8 = 0x70;
const I32_SHL: u8 = 0x74;
const I32_ROTL: u8 = 0x77;
const I64_EQZ: u8 = 0x50;
const I64_ADD: u8 = 0x7C;
const I64_SUB: u8 = 0x7D;
const I64_MUL: u8 = 0x7E;
const I64_DIV_S: u8 = 0x7F;

// ===================== modules used by the tests =====================
/// factorial, recursive: (func (param i64) (result i64) ...), built with the assembler.
fn factorial_module() -> Vec<u8> {
    let mut a = Asm::default();
    a.local_get(0).op(I64_EQZ).if_(I64).i64c(1).else_();
    a.local_get(0)
        .local_get(0)
        .i64c(1)
        .op(I64_SUB)
        .call(0)
        .op(I64_MUL)
        .end();
    let mut mb = ModuleBuilder::default();
    let f = mb.func(&[I64], &[I64], &[], &a);
    mb.export("fac", f);
    mb.build()
}

/// The same factorial assembled BY HAND, byte by byte, with the meaning of each group in the comments.
const FACTORIAL_WASM: [u8; 54] = [
    0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // magic, version 1
    0x01, 0x06, 0x01, 0x60, 0x01, 0x7E, 0x01, 0x7E, // type section: (i64) -> i64
    0x03, 0x02, 0x01, 0x00, // function section: one function of type 0
    0x07, 0x07, 0x01, 0x03, b'f', b'a', b'c', 0x00, 0x00, // export "fac" = func 0
    0x0A, 0x17, 0x01, 0x15, 0x00, // code section: 1 body of 21 bytes, 0 local groups
    0x20, 0x00, 0x50, // local.get 0; i64.eqz
    0x04, 0x7E, 0x42, 0x01, // if (result i64) i64.const 1
    0x05, 0x20, 0x00, 0x20, 0x00, 0x42, 0x01,
    0x7D, // else local.get 0; local.get 0; i64.const 1; i64.sub
    0x10, 0x00, 0x7E, 0x0B, 0x0B, // call 0; i64.mul; end(if); end(func)
];

/// gcd by the Euclidean algorithm with a block/loop, written by hand as bytes.
const GCD_WASM: [u8; 62] = [
    0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // header
    0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F, // type: (i32, i32) -> i32
    0x03, 0x02, 0x01, 0x00, // function section
    0x07, 0x07, 0x01, 0x03, b'g', b'c', b'd', 0x00, 0x00, // export "gcd"
    0x0A, 0x1E, 0x01, 0x1C, 0x00, // code section, body of 28 bytes, no locals
    0x02, 0x40, 0x03, 0x40, // block; loop
    0x20, 0x01, 0x45, 0x0D, 0x01, // local.get 1; i32.eqz; br_if 1 (exit block)
    0x20, 0x01, 0x20, 0x00, 0x20, 0x01, 0x70, // push b; push a % b (a, b, then rem_u)
    0x21, 0x01, 0x21, 0x00, // local.set 1 (b = a % b); local.set 0 (a = old b)
    0x0C, 0x00, 0x0B, 0x0B, // br 0 (loop again); end; end
    0x20, 0x00, 0x0B, // local.get 0; end
];

/// fib iterative (i64): locals a, b; loop (a, b) = (b, a + b) n times.
fn fib_iter_module() -> Vec<u8> {
    let mut a = Asm::default();
    a.i64c(1).local_set(2);
    a.block(VOID).loop_(VOID);
    a.local_get(0).op(I64_EQZ).br_if(1);
    a.local_get(2)
        .local_get(1)
        .local_get(2)
        .op(I64_ADD)
        .local_set(2)
        .local_set(1);
    a.local_get(0)
        .i64c(1)
        .op(I64_SUB)
        .local_set(0)
        .br(0)
        .end()
        .end();
    a.local_get(1);
    let mut mb = ModuleBuilder::default();
    let f = mb.func(&[I64], &[I64], &[I64, I64], &a);
    mb.export("fib", f);
    mb.build()
}

/// fib recursive (i32): the call-heavy case.
fn fib_rec_module() -> Vec<u8> {
    let mut a = Asm::default();
    a.local_get(0)
        .i32c(2)
        .op(I32_LT_S)
        .if_(I32)
        .local_get(0)
        .else_();
    a.local_get(0)
        .i32c(1)
        .op(I32_SUB)
        .call(0)
        .local_get(0)
        .i32c(2)
        .op(I32_SUB)
        .call(0)
        .op(I32_ADD)
        .end();
    let mut mb = ModuleBuilder::default();
    let f = mb.func(&[I32], &[I32], &[], &a);
    mb.export("fib", f);
    mb.build()
}

/// A module that exercises every trap kind through one export each.
fn trap_module() -> Vec<u8> {
    let mut mb = ModuleBuilder::default();
    mb.mem = Some((1, Some(2)));
    let bin = |op: u8| {
        let mut a = Asm::default();
        a.local_get(0).local_get(1).op(op);
        a
    };
    for (name, op) in [
        ("div_s", I32_DIV_S),
        ("div_u", I32_DIV_U),
        ("rem_s", I32_REM_S),
        ("rem_u", I32_REM_U),
        ("shl", I32_SHL),
        ("rotl", I32_ROTL),
    ] {
        let f = mb.func(&[I32, I32], &[I32], &[], &bin(op));
        mb.export(name, f);
    }
    let f = mb.func(&[I64, I64], &[I64], &[], &{
        let mut a = Asm::default();
        a.local_get(0).local_get(1).op(I64_DIV_S);
        a
    });
    mb.export("div_s64", f);
    let mut a = Asm::default();
    a.op(0x00); // unreachable
    let f = mb.func(&[], &[], &[], &a);
    mb.export("unreachable", f);
    let mut a = Asm::default();
    a.local_get(0).mem(0x28, 2, 16); // i32.load offset=16
    let f = mb.func(&[I32], &[I32], &[], &a);
    mb.export("load", f);
    let mut a = Asm::default();
    a.local_get(0).i32c(7).mem(0x36, 2, 0); // i32.store
    let f = mb.func(&[I32], &[], &[], &a);
    mb.export("store", f);
    let mut a = Asm::default();
    a.loop_(VOID).br(0).end();
    let f = mb.func(&[], &[], &[], &a);
    mb.export("spin", f);
    let mut a = Asm::default();
    a.call(mb.next_func());
    let f = mb.func(&[], &[], &[], &a);
    mb.export("recurse", f);
    let mut a = Asm::default();
    a.local_get(0).op(0x40).op(0); // memory.grow
    let f = mb.func(&[I32], &[I32], &[], &a);
    mb.export("grow", f);
    mb.build()
}

/// Memory with data segments, signed/unsigned sub-word loads and a byte-sum loop.
fn memory_module() -> Vec<u8> {
    let mut mb = ModuleBuilder::default();
    mb.mem = Some((1, Some(2)));
    mb.data.push((16, b"Hello, Wasm!".to_vec()));
    mb.data.push((100, vec![0xFE, 0xFF, 0xFF, 0xFF]));
    let load = |op| {
        let mut a = Asm::default();
        a.local_get(0).mem(op, 0, 0);
        a
    };
    let f = mb.func(&[I32], &[I32], &[], &load(0x2C));
    mb.export("load8_s", f);
    let f = mb.func(&[I32], &[I32], &[], &load(0x2D));
    mb.export("load8_u", f);
    let f = mb.func(&[I32], &[I32], &[], &load(0x2E));
    mb.export("load16_s", f);
    let mut a = Asm::default();
    a.local_get(0)
        .local_get(1)
        .mem(0x37, 3, 0)
        .local_get(0)
        .mem(0x29, 3, 0); // i64.store; i64.load
    let f = mb.func(&[I32, I64], &[I64], &[], &a);
    mb.export("roundtrip", f);
    // sum(ptr, len): bytes added with a loop; locals: acc
    let mut a = Asm::default();
    a.block(VOID).loop_(VOID);
    a.local_get(1).op(I32_EQZ).br_if(1);
    a.local_get(2)
        .local_get(0)
        .mem(0x2D, 0, 0)
        .op(I32_ADD)
        .local_set(2);
    a.local_get(0).i32c(1).op(I32_ADD).local_set(0);
    a.local_get(1)
        .i32c(1)
        .op(I32_SUB)
        .local_set(1)
        .br(0)
        .end()
        .end();
    a.local_get(2);
    let f = mb.func(&[I32, I32], &[I32], &[I32], &a);
    mb.export("sum", f);
    let mut a = Asm::default();
    a.op(0x3F).op(0); // memory.size
    let f = mb.func(&[], &[I32], &[], &a);
    mb.export("size", f);
    let mut a = Asm::default();
    a.local_get(0).op(0x40).op(0);
    let f = mb.func(&[I32], &[I32], &[], &a);
    mb.export("grow", f);
    let mut a = Asm::default();
    a.i64c(i64::MIN).i64c(-1).op(0x81); // i64.rem_s MIN % -1 = 0 (no trap)
    let f = mb.func(&[], &[I64], &[], &a);
    mb.export("rem_min", f);
    mb.build()
}

/// Globals, a start function, a function table and call_indirect.
fn table_module() -> Vec<u8> {
    let mut mb = ModuleBuilder::default();
    mb.table = Some(4);
    mb.globals.push((I32, true, 10));
    let t_unary = mb.type_(&[I32], &[I32]);
    let mut a = Asm::default();
    a.local_get(0).i32c(2).op(0x6C);
    let double = mb.func(&[I32], &[I32], &[], &a);
    let mut a = Asm::default();
    a.local_get(0).i32c(3).op(0x6C);
    let triple = mb.func(&[I32], &[I32], &[], &a);
    let mut a = Asm::default();
    a.i32c(7);
    let seven = mb.func(&[], &[I32], &[], &a);
    mb.elems.push((0, vec![double, triple, seven]));
    let mut a = Asm::default();
    a.local_get(1).local_get(0).call_indirect(t_unary);
    let f = mb.func(&[I32, I32], &[I32], &[], &a);
    mb.export("call_ind", f);
    // start: global += 5 ; bump(): global += 1 and return it
    let mut a = Asm::default();
    a.global_get(0).i32c(5).op(I32_ADD).global_set(0);
    let start = mb.func(&[], &[], &[], &a);
    mb.start = Some(start);
    let mut a = Asm::default();
    a.global_get(0)
        .i32c(1)
        .op(I32_ADD)
        .global_set(0)
        .global_get(0);
    let f = mb.func(&[], &[I32], &[], &a);
    mb.export("bump", f);
    // br_table dispatch: 0 -> 100, 1 -> 200, else -> 300
    let mut a = Asm::default();
    a.block(VOID)
        .block(VOID)
        .block(VOID)
        .local_get(0)
        .br_table(&[0, 1], 2)
        .end();
    a.i32c(100).op(0x0F).end();
    a.i32c(200).op(0x0F).end();
    a.i32c(300);
    let f = mb.func(&[I32], &[I32], &[], &a);
    mb.export("dispatch", f);
    mb.build()
}

fn run(bytes: &[u8], name: &str, args: &[Value]) -> Result<Vec<Value>, Trap> {
    let m = decode(bytes).expect("decode");
    validate(&m).expect("validate");
    Instance::new(&m)?.call_export(name, args)
}

// ===================== random expression differential test =====================
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next() >> 11) % n as u64) as usize
    }
    fn interesting32(&mut self) -> i32 {
        let pool = [0, 1, -1, 2, 31, 32, 33, i32::MAX, i32::MIN, 255, -256];
        if self.below(3) == 0 {
            pool[self.below(pool.len())]
        } else {
            self.next() as i32 >> self.below(30)
        }
    }
    fn interesting64(&mut self) -> i64 {
        let pool = [
            0,
            1,
            -1,
            63,
            64,
            65,
            i64::MAX,
            i64::MIN,
            i32::MAX as i64,
            i32::MIN as i64,
        ];
        if self.below(3) == 0 {
            pool[self.below(pool.len())]
        } else {
            self.next() as i64 >> self.below(60)
        }
    }
}

enum E32 {
    C(i32),
    P(u32),
    Un(u8, Box<E32>),
    Bin(u8, Box<E32>, Box<E32>),
    Cmp64(u8, Box<E64>, Box<E64>),
    Wrap(Box<E64>),
    Eqz64(Box<E64>),
    If(Box<E32>, Box<E32>, Box<E32>),
    Sel(Box<E32>, Box<E32>, Box<E32>),
}
enum E64 {
    C(i64),
    P(u32),
    Un(u8, Box<E64>),
    Bin(u8, Box<E64>, Box<E64>),
    Ext(bool, Box<E32>),
}

fn gen32(r: &mut Rng, d: u32) -> E32 {
    let leaf = d == 0 || r.below(4) == 0;
    if leaf {
        return if r.below(2) == 0 {
            E32::C(r.interesting32())
        } else {
            E32::P(r.below(3) as u32)
        };
    }
    let sub = |r: &mut Rng| Box::new(gen32(r, d - 1));
    match r.below(8) {
        0 => E32::Un([0x45, 0x67, 0x68, 0x69][r.below(4)], sub(r)),
        1 | 2 => E32::Bin(0x6A + r.below(15) as u8, sub(r), sub(r)),
        3 => E32::Cmp64(
            0x51 + r.below(10) as u8,
            Box::new(gen64(r, d - 1)),
            Box::new(gen64(r, d - 1)),
        ),
        4 => E32::Wrap(Box::new(gen64(r, d - 1))),
        5 => E32::Eqz64(Box::new(gen64(r, d - 1))),
        6 => E32::If(sub(r), sub(r), sub(r)),
        _ => E32::Sel(sub(r), sub(r), sub(r)),
    }
}
fn gen64(r: &mut Rng, d: u32) -> E64 {
    if d == 0 || r.below(4) == 0 {
        return if r.below(2) == 0 {
            E64::C(r.interesting64())
        } else {
            E64::P(3 + r.below(2) as u32)
        };
    }
    match r.below(5) {
        0 => E64::Un(0x79 + r.below(3) as u8, Box::new(gen64(r, d - 1))),
        1 | 2 => E64::Bin(
            0x7C + r.below(15) as u8,
            Box::new(gen64(r, d - 1)),
            Box::new(gen64(r, d - 1)),
        ),
        _ => E64::Ext(r.below(2) == 0, Box::new(gen32(r, d - 1))),
    }
}

fn emit32(e: &E32, a: &mut Asm) {
    match e {
        E32::C(v) => {
            a.i32c(*v);
        }
        E32::P(i) => {
            a.local_get(*i);
        }
        E32::Un(op, x) => {
            emit32(x, a);
            a.op(*op);
        }
        E32::Bin(op, x, y) => {
            emit32(x, a);
            emit32(y, a);
            a.op(*op);
        }
        E32::Cmp64(op, x, y) => {
            emit64(x, a);
            emit64(y, a);
            a.op(*op);
        }
        E32::Wrap(x) => {
            emit64(x, a);
            a.op(0xA7);
        }
        E32::Eqz64(x) => {
            emit64(x, a);
            a.op(I64_EQZ);
        }
        E32::If(c, t, f) => {
            emit32(c, a);
            a.if_(I32);
            emit32(t, a);
            a.else_();
            emit32(f, a);
            a.end();
        }
        E32::Sel(x, y, c) => {
            emit32(x, a);
            emit32(y, a);
            emit32(c, a);
            a.op(0x1B);
        }
    }
}
fn emit64(e: &E64, a: &mut Asm) {
    match e {
        E64::C(v) => {
            a.i64c(*v);
        }
        E64::P(i) => {
            a.local_get(*i);
        }
        E64::Un(op, x) => {
            emit64(x, a);
            a.op(*op);
        }
        E64::Bin(op, x, y) => {
            emit64(x, a);
            emit64(y, a);
            a.op(*op);
        }
        E64::Ext(signed, x) => {
            emit32(x, a);
            a.op(if *signed { 0xAC } else { 0xAD });
        }
    }
}

/// Oracle for i32 binary ops, deliberately written differently from the interpreter: widen to i64 and truncate.
fn oracle_bin32(op: u8, a: i32, b: i32) -> Result<i32, Trap> {
    let (x, y) = (a as i64, b as i64);
    let (ux, uy) = (a as u32 as u64, b as u32 as u64);
    let k = (uy & 31) as u32;
    Ok(match op {
        0x6A => (x + y) as i32,
        0x6B => (x - y) as i32,
        0x6C => (x * y) as i32,
        0x6D | 0x6F if y == 0 => return Err(Trap::DivByZero),
        0x6D if x / y == 1 << 31 => return Err(Trap::IntOverflow),
        0x6D => (x / y) as i32,
        0x6F => (x % y) as i32,
        0x6E | 0x70 if uy == 0 => return Err(Trap::DivByZero),
        0x6E => (ux / uy) as i32,
        0x70 => (ux % uy) as i32,
        0x71 => a & b,
        0x72 => a | b,
        0x73 => a ^ b,
        0x74 => (ux << k) as u32 as i32,
        0x75 => (x >> k) as i32,
        0x76 => (ux >> k) as i32,
        0x77 => ((ux << k) | (ux >> (32 - k))) as u32 as i32,
        _ => ((ux >> k) | (ux << (32 - k))) as u32 as i32,
    })
}
fn oracle_bin64(op: u8, a: i64, b: i64) -> Result<i64, Trap> {
    let (x, y) = (a as i128, b as i128);
    let (ux, uy) = (a as u64 as u128, b as u64 as u128);
    let k = (uy & 63) as u32;
    Ok(match op {
        0x7C => (x + y) as i64,
        0x7D => (x - y) as i64,
        0x7E => (x * y) as i64,
        0x7F | 0x81 if y == 0 => return Err(Trap::DivByZero),
        0x7F if x / y == 1 << 63 => return Err(Trap::IntOverflow),
        0x7F => (x / y) as i64,
        0x81 => (x % y) as i64,
        0x80 | 0x82 if uy == 0 => return Err(Trap::DivByZero),
        0x80 => (ux / uy) as i64,
        0x82 => (ux % uy) as i64,
        0x83 => a & b,
        0x84 => a | b,
        0x85 => a ^ b,
        0x86 => (ux << k) as u64 as i64,
        0x87 => (x >> k) as i64,
        0x88 => (ux >> k) as i64,
        0x89 => ((ux << k) | (ux >> (64 - k))) as u64 as i64,
        _ => ((ux >> k) | (ux << (64 - k))) as u64 as i64,
    })
}
fn oracle_cmp64(op: u8, a: i64, b: i64) -> bool {
    let (ua, ub) = (a as u64, b as u64);
    match op {
        0x51 => a == b,
        0x52 => a != b,
        0x53 => a < b,
        0x54 => ua < ub,
        0x55 => a > b,
        0x56 => ua > ub,
        0x57 => a <= b,
        0x58 => ua <= ub,
        0x59 => a >= b,
        _ => ua >= ub,
    }
}

fn eval32(e: &E32, p: &([i32; 3], [i64; 2])) -> Result<i32, Trap> {
    Ok(match e {
        E32::C(v) => *v,
        E32::P(i) => p.0[*i as usize],
        E32::Un(op, x) => {
            let v = eval32(x, p)?;
            match op {
                0x45 => (v == 0) as i32,
                0x67 => v.leading_zeros() as i32,
                0x68 => v.trailing_zeros() as i32,
                _ => v.count_ones() as i32,
            }
        }
        E32::Bin(op, x, y) => oracle_bin32(*op, eval32(x, p)?, eval32(y, p)?)?,
        E32::Cmp64(op, x, y) => oracle_cmp64(*op, eval64(x, p)?, eval64(y, p)?) as i32,
        E32::Wrap(x) => eval64(x, p)? as i32,
        E32::Eqz64(x) => (eval64(x, p)? == 0) as i32,
        E32::If(c, t, f) => {
            if eval32(c, p)? != 0 {
                eval32(t, p)?
            } else {
                eval32(f, p)?
            }
        }
        E32::Sel(x, y, c) => {
            let (a, b, c) = (eval32(x, p)?, eval32(y, p)?, eval32(c, p)?);
            if c != 0 { a } else { b }
        }
    })
}
fn eval64(e: &E64, p: &([i32; 3], [i64; 2])) -> Result<i64, Trap> {
    Ok(match e {
        E64::C(v) => *v,
        E64::P(i) => p.1[*i as usize - 3],
        E64::Un(op, x) => {
            let v = eval64(x, p)?;
            (match op {
                0x79 => v.leading_zeros(),
                0x7A => v.trailing_zeros(),
                _ => v.count_ones(),
            }) as i64
        }
        E64::Bin(op, x, y) => oracle_bin64(*op, eval64(x, p)?, eval64(y, p)?)?,
        E64::Ext(true, x) => eval32(x, p)? as i64,
        E64::Ext(false, x) => eval32(x, p)? as u32 as i64,
    })
}

/// Returns (cases, trapping cases).
fn differential(cases: usize, seed: u64) -> (usize, usize) {
    let mut r = Rng(seed);
    let mut traps = 0;
    for case in 0..cases {
        let want64 = r.below(3) == 0;
        let (e32, e64) = if want64 {
            (None, Some(gen64(&mut r, 4)))
        } else {
            (Some(gen32(&mut r, 4)), None)
        };
        let mut a = Asm::default();
        match (&e32, &e64) {
            (Some(e), _) => emit32(e, &mut a),
            (_, Some(e)) => emit64(e, &mut a),
            _ => unreachable!(),
        }
        let mut mb = ModuleBuilder::default();
        let f = mb.func(
            &[I32, I32, I32, I64, I64],
            &[if want64 { I64 } else { I32 }],
            &[],
            &a,
        );
        mb.export("f", f);
        let bytes = mb.build();
        let module = decode(&bytes).unwrap_or_else(|e| panic!("case {case}: decode {e:?}"));
        validate(&module).unwrap_or_else(|e| panic!("case {case}: validate {e:?}"));
        let mut inst = Instance::new(&module).unwrap();
        for _ in 0..2 {
            let p = (
                [r.interesting32(), r.interesting32(), r.interesting32()],
                [r.interesting64(), r.interesting64()],
            );
            let args = [
                Value::I32(p.0[0]),
                Value::I32(p.0[1]),
                Value::I32(p.0[2]),
                Value::I64(p.1[0]),
                Value::I64(p.1[1]),
            ];
            let got = inst.call_export("f", &args);
            let want = match (&e32, &e64) {
                (Some(e), _) => eval32(e, &p).map(|v| vec![Value::I32(v)]),
                (_, Some(e)) => eval64(e, &p).map(|v| vec![Value::I64(v)]),
                _ => unreachable!(),
            };
            assert_eq!(got, want, "case {case} args {args:?}");
            traps += got.is_err() as usize;
        }
    }
    (cases * 2, traps)
}

// ===================== tests =====================
fn leb_tests() {
    let mut r = Reader {
        b: &[0xE5, 0x8E, 0x26],
        pos: 0,
    };
    assert_eq!(r.leb_u(32), Ok(624485)); // the classic example from the LEB128 article
    let mut r = Reader {
        b: &[0xC0, 0xBB, 0x78],
        pos: 0,
    };
    assert_eq!(r.leb_s(32), Ok(-123456));
    let overlong = Reader {
        b: &[0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
        pos: 0,
    }
    .leb_u(32);
    assert!(overlong.is_err(), "6-byte u32 is rejected");
    assert!(
        Reader {
            b: &[0xFF, 0xFF, 0xFF, 0xFF, 0x7F],
            pos: 0
        }
        .leb_u(32)
        .is_err(),
        "5th byte sets bits beyond 32"
    );
    assert_eq!(
        Reader {
            b: &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F],
            pos: 0
        }
        .leb_u(32),
        Ok(u32::MAX as u64)
    );
    assert!(
        Reader { b: &[0x80], pos: 0 }.leb_u(32).is_err(),
        "truncated"
    );
    assert_eq!(
        Reader { b: &[0x80], pos: 0 }.leb_u(32).unwrap_err().offset,
        1,
        "error carries the byte offset"
    );
    assert_eq!(Reader { b: &[0x7F], pos: 0 }.leb_s(32), Ok(-1));
    assert!(
        Reader {
            b: &[0xFF, 0xFF, 0xFF, 0xFF, 0x4F],
            pos: 0
        }
        .leb_s(32)
        .is_err(),
        "bad sign extension in last byte"
    );
    let mut rng = Rng(99);
    for _ in 0..5000 {
        let (u, s32, s64) = (
            rng.next() >> rng.below(64),
            rng.next() as i32 >> rng.below(31),
            rng.next() as i64 >> rng.below(63),
        );
        let mut b = Vec::new();
        uleb(&mut b, u);
        assert_eq!(Reader { b: &b, pos: 0 }.leb_u(64), Ok(u));
        b.clear();
        sleb(&mut b, s32 as i64);
        assert_eq!(Reader { b: &b, pos: 0 }.leb_s(32), Ok(s32 as i64));
        b.clear();
        sleb(&mut b, s64);
        assert_eq!(Reader { b: &b, pos: 0 }.leb_s(64), Ok(s64));
    }
    println!(
        "LEB128: known vectors, over-long/over-large rejection, 15000 encode/decode round trips ok"
    );
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn program_tests() {
    assert_eq!(
        factorial_module(),
        FACTORIAL_WASM,
        "assembler output equals the hand-assembled bytes"
    );
    let mut cases = 0;
    let m = decode(&FACTORIAL_WASM).unwrap();
    validate(&m).unwrap();
    let mut inst = Instance::new(&m).unwrap();
    for n in 0..=25i64 {
        let want = (1..=n as u64).fold(1u64, |a, b| a.wrapping_mul(b)) as i64; // wraps past 20!, like i64.mul
        assert_eq!(
            inst.call_export("fac", &[Value::I64(n)]),
            Ok(vec![Value::I64(want)])
        );
        cases += 1;
    }
    let g = decode(&GCD_WASM).unwrap();
    validate(&g).unwrap();
    let mut gi = Instance::new(&g).unwrap();
    let mut rng = Rng(5);
    for _ in 0..500 {
        let (a, b) = (
            rng.next() as u32 >> rng.below(30),
            rng.next() as u32 >> rng.below(30),
        );
        assert_eq!(
            gi.call_export("gcd", &[Value::I32(a as i32), Value::I32(b as i32)]),
            Ok(vec![Value::I32(gcd(a, b) as i32)])
        );
        cases += 1;
    }
    let fib = |n: u64| {
        (0..n)
            .fold((0u64, 1u64), |(a, b), _| (b, a.wrapping_add(b)))
            .0
    };
    for n in 0..=90i64 {
        assert_eq!(
            run(&fib_iter_module(), "fib", &[Value::I64(n)]),
            Ok(vec![Value::I64(fib(n as u64) as i64)])
        );
        cases += 1;
    }
    let bytes = fib_rec_module();
    let m = decode(&bytes).unwrap();
    validate(&m).unwrap();
    let mut inst = Instance::new(&m).unwrap();
    for n in 0..=22i32 {
        assert_eq!(
            inst.call_export("fib", &[Value::I32(n)]),
            Ok(vec![Value::I32(fib(n as u64) as i32)])
        );
        cases += 1;
    }
    println!(
        "programs: factorial (hand bytes == assembler), gcd, fib iterative+recursive match Rust ({cases} cases)"
    );
}

fn trap_tests() {
    let bytes = trap_module();
    let m = decode(&bytes).unwrap();
    validate(&m).unwrap();
    let mut i = Instance::new(&m).unwrap();
    let i32s = |a: i32, b: i32| [Value::I32(a), Value::I32(b)];
    let ok = |v: i32| Ok(vec![Value::I32(v)]);
    assert_eq!(i.call_export("unreachable", &[]), Err(Trap::Unreachable));
    assert_eq!(i.call_export("div_s", &i32s(7, 0)), Err(Trap::DivByZero));
    assert_eq!(i.call_export("div_u", &i32s(7, 0)), Err(Trap::DivByZero));
    assert_eq!(i.call_export("rem_s", &i32s(7, 0)), Err(Trap::DivByZero));
    assert_eq!(i.call_export("rem_u", &i32s(7, 0)), Err(Trap::DivByZero));
    assert_eq!(
        i.call_export("div_s", &i32s(i32::MIN, -1)),
        Err(Trap::IntOverflow)
    );
    assert_eq!(
        i.call_export("div_s64", &[Value::I64(i64::MIN), Value::I64(-1)]),
        Err(Trap::IntOverflow)
    );
    assert_eq!(
        i.call_export("div_s64", &[Value::I64(1), Value::I64(0)]),
        Err(Trap::DivByZero)
    );
    assert_eq!(
        i.call_export("rem_s", &i32s(i32::MIN, -1)),
        ok(0),
        "rem_s(MIN, -1) is 0, not a trap"
    );
    assert_eq!(
        i.call_export("div_s", &i32s(-7, 2)),
        ok(-3),
        "division truncates toward zero"
    );
    assert_eq!(i.call_export("rem_s", &i32s(-7, 2)), ok(-1));
    assert_eq!(i.call_export("div_u", &i32s(-1, 2)), ok(0x7FFF_FFFF));
    assert_eq!(
        i.call_export("shl", &i32s(1, 33)),
        ok(2),
        "shift count is taken mod 32"
    );
    assert_eq!(i.call_export("shl", &i32s(1, 32)), ok(1));
    assert_eq!(
        i.call_export("rotl", &i32s(0x8000_0001u32 as i32, 1)),
        ok(3)
    );
    // memory: 1 page = 65536 bytes; load has offset 16
    assert_eq!(i.call_export("load", &[Value::I32(65536 - 16 - 4)]), ok(0));
    assert_eq!(
        i.call_export("load", &[Value::I32(65536 - 16 - 3)]),
        Err(Trap::OutOfBoundsMemory)
    );
    assert_eq!(
        i.call_export("load", &[Value::I32(-1)]),
        Err(Trap::OutOfBoundsMemory),
        "addr + offset must not wrap at 2^32"
    );
    assert_eq!(
        i.call_export("store", &[Value::I32(65533)]),
        Err(Trap::OutOfBoundsMemory)
    );
    assert_eq!(i.call_export("store", &[Value::I32(65532)]), Ok(vec![]));
    assert_eq!(i.call_export("grow", &[Value::I32(1)]), ok(1));
    assert_eq!(
        i.call_export("grow", &[Value::I32(1)]),
        ok(-1),
        "max is 2 pages: grow fails with -1"
    );
    assert_eq!(i.call_export("recurse", &[]), Err(Trap::CallStackExhausted));
    i.budget = 50_000;
    assert_eq!(
        i.call_export("spin", &[]),
        Err(Trap::OutOfFuel),
        "fuel stops an infinite loop"
    );
    assert_eq!(i.call_export("nope", &[]), Err(Trap::NoSuchExport));
    assert_eq!(
        i.call_export("div_s", &[Value::I32(1)]),
        Err(Trap::BadArguments)
    );
    assert_eq!(
        i.call_export("div_s", &[Value::I64(1), Value::I64(1)]),
        Err(Trap::BadArguments)
    );
    println!(
        "traps: unreachable, div0 (4 ops), div_s overflow, memory OOB (+offset wrap), recursion, fuel, bad calls ok"
    );
}

fn memory_and_table_tests() {
    let bytes = memory_module();
    let m = decode(&bytes).unwrap();
    validate(&m).unwrap();
    let mut i = Instance::new(&m).unwrap();
    let ok = |v: i32| Ok(vec![Value::I32(v)]);
    assert_eq!(i.call_export("load8_u", &[Value::I32(16)]), ok(b'H' as i32));
    assert_eq!(i.call_export("load8_s", &[Value::I32(100)]), ok(-2));
    assert_eq!(i.call_export("load8_u", &[Value::I32(100)]), ok(254));
    assert_eq!(
        i.call_export("load16_s", &[Value::I32(100)]),
        ok(-2),
        "16-bit sign extension of 0xFFFE"
    );
    let want: i32 = b"Hello, Wasm!".iter().map(|&b| b as i32).sum();
    assert_eq!(
        i.call_export("sum", &[Value::I32(16), Value::I32(12)]),
        ok(want)
    );
    assert_eq!(
        i.call_export(
            "roundtrip",
            &[Value::I32(4096), Value::I64(-0x1234_5678_9ABC)]
        ),
        Ok(vec![Value::I64(-0x1234_5678_9ABC)])
    );
    assert_eq!(i.call_export("size", &[]), ok(1));
    assert_eq!(i.call_export("grow", &[Value::I32(1)]), ok(1));
    assert_eq!(i.call_export("size", &[]), ok(2));
    assert_eq!(
        i.call_export("load8_u", &[Value::I32(70000)]),
        ok(0),
        "grown memory is zeroed and usable"
    );
    assert_eq!(i.call_export("rem_min", &[]), Ok(vec![Value::I64(0)]));

    let bytes = table_module();
    let m = decode(&bytes).unwrap();
    validate(&m).unwrap();
    let mut t = Instance::new(&m).unwrap();
    assert_eq!(
        t.call_export("bump", &[]),
        ok(16),
        "start ran (10 + 5), then bump adds 1"
    );
    assert_eq!(
        t.call_export("call_ind", &[Value::I32(0), Value::I32(21)]),
        ok(42)
    );
    assert_eq!(
        t.call_export("call_ind", &[Value::I32(1), Value::I32(21)]),
        ok(63)
    );
    assert_eq!(
        t.call_export("call_ind", &[Value::I32(2), Value::I32(21)]),
        Err(Trap::IndirectCallTypeMismatch)
    );
    assert_eq!(
        t.call_export("call_ind", &[Value::I32(3), Value::I32(21)]),
        Err(Trap::UninitializedElement)
    );
    assert_eq!(
        t.call_export("call_ind", &[Value::I32(4), Value::I32(21)]),
        Err(Trap::UndefinedElement)
    );
    for (sel, want) in [(0, 100), (1, 200), (2, 300), (99, 300)] {
        assert_eq!(
            t.call_export("dispatch", &[Value::I32(sel)]),
            ok(want),
            "br_table {sel}"
        );
    }
    println!(
        "memory/data segments, sub-word loads, grow, globals + start, call_indirect, br_table ok"
    );
}

fn validation_tests() {
    // each case: function params/results/locals, body, expected error substring
    type Case = (
        &'static str,
        &'static [u8],
        &'static [u8],
        Box<dyn Fn(&mut Asm) -> &mut Asm>,
        &'static str,
    );
    let cases: Vec<Case> = vec![
        (
            "i64 where i32 result expected",
            &[],
            &[I32],
            Box::new(|a: &mut Asm| a.i64c(1)),
            "type mismatch",
        ),
        (
            "add on an empty stack",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.op(I32_ADD)),
            "underflow",
        ),
        (
            "missing result",
            &[],
            &[I32],
            Box::new(|a: &mut Asm| a),
            "underflow",
        ),
        (
            "leftover value",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.i32c(1)),
            "values remaining",
        ),
        (
            "branch to unknown label",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.br(5)),
            "unknown label",
        ),
        (
            "unknown local",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.local_get(3).op(0x1A)),
            "unknown local",
        ),
        (
            "if with result and no else",
            &[],
            &[I32],
            Box::new(|a: &mut Asm| a.i32c(1).if_(I32).i32c(2).end()),
            "requires an else",
        ),
        (
            "block result of wrong type",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.block(I32).i64c(1).end().op(0x1A)),
            "type mismatch",
        ),
        (
            "call of unknown function",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.call(9)),
            "unknown function",
        ),
        (
            "load without memory",
            &[],
            &[I32],
            Box::new(|a: &mut Asm| a.i32c(0).mem(0x28, 2, 0)),
            "without a memory",
        ),
        (
            "call_indirect without table",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.i32c(0).call_indirect(0)),
            "without a table",
        ),
        (
            "br_table with differing types",
            &[],
            &[],
            Box::new(|a: &mut Asm| {
                a.block(I32)
                    .block(VOID)
                    .i32c(0)
                    .br_table(&[0], 1)
                    .end()
                    .i32c(0)
                    .end()
                    .op(0x1A)
            }),
            "different types",
        ),
        (
            "select on mixed types",
            &[],
            &[],
            Box::new(|a: &mut Asm| a.i32c(1).i64c(2).i32c(0).op(0x1B).op(0x1A)),
            "type mismatch",
        ),
    ];
    let mut rejected = 0;
    for (name, p, r, body, want) in &cases {
        let mut a = Asm::default();
        body(&mut a);
        let mut mb = ModuleBuilder::default();
        mb.func(p, r, &[], &a);
        let m = decode(&mb.build()).unwrap_or_else(|e| panic!("{name}: should decode: {e:?}"));
        let err = validate(&m).expect_err(name);
        assert!(err.msg.contains(want), "{name}: got {err:?}");
        rejected += 1;
    }
    // module-level checks
    let mut mb = ModuleBuilder::default();
    mb.mem = Some((1, None));
    mb.globals.push((I32, false, 0));
    mb.func(&[], &[], &[], Asm::default().i32c(1).global_set(0));
    assert!(
        validate(&decode(&mb.build()).unwrap())
            .unwrap_err()
            .msg
            .contains("immutable")
    );
    let mut mb = ModuleBuilder::default();
    mb.mem = Some((1, None));
    mb.func(&[], &[I32], &[], Asm::default().i32c(0).mem(0x28, 3, 0)); // align 8 on a 4-byte load
    assert!(
        validate(&decode(&mb.build()).unwrap())
            .unwrap_err()
            .msg
            .contains("alignment")
    );
    let mut mb = ModuleBuilder::default();
    let f = mb.func(&[I32], &[], &[], &Asm::default());
    mb.start = Some(f);
    assert!(
        validate(&decode(&mb.build()).unwrap())
            .unwrap_err()
            .msg
            .contains("start function")
    );
    let mut mb = ModuleBuilder::default();
    mb.exports.push(("x".into(), 0, 7));
    assert!(
        validate(&decode(&mb.build()).unwrap())
            .unwrap_err()
            .msg
            .contains("export")
    );
    rejected += 4;
    // valid despite looking odd: code after `unreachable`/`br` is type-checked polymorphically
    for (r, body) in [
        (I32, vec![0x00u8]),
        (I32, vec![0x00, I32_ADD]),
        (I32, vec![0x41, 0x05, 0x0C, 0x00, 0x6A]),
    ] {
        let mut mb = ModuleBuilder::default();
        mb.func(&[], &[r], &[], &Asm(body));
        let m = decode(&mb.build()).unwrap();
        validate(&m).expect("polymorphic stack after unreachable code is valid");
    }
    // decode-level errors carry offsets
    assert_eq!(
        decode(b"\x01asm\x01\0\0\0").unwrap_err(),
        Error {
            offset: 0,
            msg: "bad magic".into()
        }
    );
    assert_eq!(decode(b"\0asm\x02\0\0\0").unwrap_err().offset, 4);
    let bytes = factorial_module();
    for cut in 0..bytes.len() {
        if let Err(e) = decode(&bytes[..cut]) {
            assert!(
                e.offset <= cut,
                "error offset {} beyond input of {cut} bytes",
                e.offset
            );
        }
    }
    println!(
        "validation rejected {rejected} ill-typed modules with the expected diagnostics; polymorphic-stack cases accepted"
    );
}

fn mutate(r: &mut Rng, corpus: &[Vec<u8>]) -> Vec<u8> {
    let header = b"\0asm\x01\0\0\0";
    match r.below(5) {
        0 => (0..r.below(80)).map(|_| r.next() as u8).collect(),
        1 => {
            let mut v = header.to_vec();
            v.extend((0..r.below(120)).map(|_| r.next() as u8));
            v
        }
        _ => {
            let mut v = corpus[r.below(corpus.len())].clone();
            for _ in 0..1 + r.below(4) {
                match r.below(4) {
                    0 if !v.is_empty() => {
                        let i = r.below(v.len());
                        v[i] = r.next() as u8;
                    }
                    1 if !v.is_empty() => {
                        let n = r.below(v.len());
                        v.truncate(n);
                    }
                    2 => {
                        let i = r.below(v.len() + 1);
                        v.insert(i, r.next() as u8);
                    }
                    _ if !v.is_empty() => {
                        let i = r.below(v.len());
                        v.remove(i);
                    }
                    _ => {}
                }
            }
            v
        }
    }
}

/// Feed hostile bytes through the whole pipeline; nothing may panic, hang or allocate wildly.
fn fuzz(n: usize) {
    let corpus = vec![
        factorial_module(),
        GCD_WASM.to_vec(),
        fib_iter_module(),
        fib_rec_module(),
        trap_module(),
        memory_module(),
        table_module(),
    ];
    let mut rng = Rng(0xF00D);
    let (mut decoded, mut valid, mut ran, mut traps) = (0, 0, 0, 0);
    for _ in 0..n {
        let bytes = mutate(&mut rng, &corpus);
        let Ok(m) = decode(&bytes) else { continue };
        decoded += 1;
        if validate(&m).is_err() {
            continue;
        }
        valid += 1;
        let Ok(mut inst) = Instance::new(&m) else {
            continue;
        };
        inst.budget = 20_000;
        for (name, kind, idx) in &m.exports {
            if *kind != 0 {
                continue;
            }
            let ty = &m.types[m.func_types[*idx as usize] as usize];
            let args: Vec<Value> = ty
                .params
                .iter()
                .map(|&t| Value::from_raw(t, rng.next() >> 40))
                .collect();
            ran += 1;
            traps += inst.call_export(name, &args).is_err() as usize;
        }
    }
    assert!(
        decoded > 0 && valid > 0 && ran > 0,
        "the fuzzer must reach deep stages"
    );
    println!(
        "fuzz: {n} inputs, no panic: {decoded} decoded, {valid} valid, {ran} exports run ({traps} trapped or out of fuel)"
    );
}

fn main() {
    leb_tests();
    program_tests();
    trap_tests();
    memory_and_table_tests();
    validation_tests();
    let (n, traps) = differential(1500, 0xC0FFEE);
    println!(
        "differential: {n} random i32/i64 expression evaluations agree with Rust ({traps} of them trap identically)"
    );
    assert!(n >= 3000 && traps > 0);
    fuzz(20_000);
    // Timing is machine-dependent: printed, never asserted.
    let m = decode(&fib_rec_module()).unwrap();
    let mut inst = Instance::new(&m).unwrap();
    inst.budget = u64::MAX;
    let t = std::time::Instant::now();
    let r = inst.call_export("fib", &[Value::I32(27)]);
    println!(
        "fib_rec(27) = {r:?} in {:?} ({} instructions executed)",
        t.elapsed(),
        u64::MAX - inst.fuel
    );
}
