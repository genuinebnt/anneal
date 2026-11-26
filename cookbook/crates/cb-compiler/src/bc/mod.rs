//! Register-based bytecode (see `docs/BYTECODE.md` for the opcode table).
//!
//! The instruction set is declared once in the [`define_instrs!`] macro, which
//! generates the `Instr` enum, the mnemonic table, the operand visitor used by
//! the verifier, and the assembler/disassembler parsers, so the pieces cannot
//! drift apart.

pub mod codegen;
pub mod text;
pub mod verify;

use crate::mir::Scope;
use crate::source::Span;
use std::rc::Rc;

// ---- operand newtypes (distinct types give the generated code its structure) ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reg(pub u16);
/// Index into the constant pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kst(pub u32);
/// Jump target: an instruction index in the same function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tgt(pub u32);
/// Function index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fid(pub u32);
/// Index into `BcFunc::lists` (argument or element register lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lst(pub u32);
/// Index into `BcFunc::switches`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swt(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Imm(pub i32);
/// Small unsigned index: a field number or an enum tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Idx(pub u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flag(pub bool);

/// What an operand refers to, as seen by the verifier.
#[derive(Clone, Copy, Debug)]
pub enum Visit {
    Reg(u16),
    Const(u32),
    Target(u32),
    Func(u32),
    List(u32),
    Switch(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BConst {
    Int(i64),
    Str(Rc<str>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SwitchTable {
    pub cases: Vec<(i64, u32)>,
    pub default: u32,
}

#[derive(Clone, Debug)]
pub struct BcFunc {
    pub name: String,
    pub nregs: u16,
    pub nparams: u16,
    pub code: Vec<Instr>,
    pub lists: Vec<Box<[u16]>>,
    pub switches: Vec<SwitchTable>,
    /// Logical-frame scope tree (see `mir::Scope`) and the scope of each instruction.
    pub scopes: Vec<Scope>,
    pub pc_scope: Vec<u32>,
    /// Source span of each instruction (`Span::default()` = unknown). Empty for
    /// hand-assembled chunks; the listing format does not carry spans.
    pub pc_span: Vec<Span>,
    pub is_const: bool,
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub funcs: Vec<BcFunc>,
    pub consts: Vec<BConst>,
    pub main: u32,
}

impl BcFunc {
    pub fn empty(name: &str) -> BcFunc {
        BcFunc {
            name: name.into(),
            nregs: 0,
            nparams: 0,
            code: vec![],
            lists: vec![],
            switches: vec![],
            scopes: vec![Scope {
                name: name.into(),
                parent: None,
                call_site: Span::default(),
            }],
            pc_scope: vec![],
            pc_span: vec![],
            is_const: false,
        }
    }
}

/// Mutable pieces the assembler appends to while parsing one function.
pub struct Builder<'a> {
    pub lists: &'a mut Vec<Box<[u16]>>,
    pub switches: &'a mut Vec<SwitchTable>,
}

/// How an operand kind is printed, parsed and visited.
pub trait OperandKind: Sized {
    fn show(&self, f: &BcFunc) -> String;
    fn parse(tok: &str, b: &mut Builder) -> Option<Self>;
    fn visit(&self, _v: &mut dyn FnMut(Visit)) {}
}

fn num<T: std::str::FromStr>(s: &str, prefix: &str) -> Option<T> {
    s.strip_prefix(prefix)?.parse().ok()
}

impl OperandKind for Reg {
    fn show(&self, _: &BcFunc) -> String {
        format!("r{}", self.0)
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        num(t, "r").map(Reg)
    }
    fn visit(&self, v: &mut dyn FnMut(Visit)) {
        v(Visit::Reg(self.0))
    }
}
impl OperandKind for Kst {
    fn show(&self, _: &BcFunc) -> String {
        format!("k{}", self.0)
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        num(t, "k").map(Kst)
    }
    fn visit(&self, v: &mut dyn FnMut(Visit)) {
        v(Visit::Const(self.0))
    }
}
impl OperandKind for Tgt {
    fn show(&self, _: &BcFunc) -> String {
        format!("@{}", self.0)
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        num(t, "@").map(Tgt)
    }
    fn visit(&self, v: &mut dyn FnMut(Visit)) {
        v(Visit::Target(self.0))
    }
}
impl OperandKind for Fid {
    fn show(&self, _: &BcFunc) -> String {
        format!("f{}", self.0)
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        num(t, "f").map(Fid)
    }
    fn visit(&self, v: &mut dyn FnMut(Visit)) {
        v(Visit::Func(self.0))
    }
}
impl OperandKind for Imm {
    fn show(&self, _: &BcFunc) -> String {
        self.0.to_string()
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        t.parse().ok().map(Imm)
    }
}
impl OperandKind for Idx {
    fn show(&self, _: &BcFunc) -> String {
        self.0.to_string()
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        t.parse().ok().map(Idx)
    }
}
impl OperandKind for Flag {
    fn show(&self, _: &BcFunc) -> String {
        self.0.to_string()
    }
    fn parse(t: &str, _: &mut Builder) -> Option<Self> {
        t.parse().ok().map(Flag)
    }
}
impl OperandKind for Lst {
    fn show(&self, f: &BcFunc) -> String {
        match f.lists.get(self.0 as usize) {
            Some(l) => format!(
                "[{}]",
                l.iter()
                    .map(|r| format!("r{r}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => format!("[bad list {}]", self.0),
        }
    }
    fn parse(t: &str, b: &mut Builder) -> Option<Self> {
        let inner = t.strip_prefix('[')?.strip_suffix(']')?.trim();
        let regs: Option<Vec<u16>> = if inner.is_empty() {
            Some(vec![])
        } else {
            inner.split(',').map(|x| num(x.trim(), "r")).collect()
        };
        b.lists.push(regs?.into_boxed_slice());
        Some(Lst((b.lists.len() - 1) as u32))
    }
    fn visit(&self, v: &mut dyn FnMut(Visit)) {
        v(Visit::List(self.0))
    }
}
impl OperandKind for Swt {
    fn show(&self, f: &BcFunc) -> String {
        match f.switches.get(self.0 as usize) {
            Some(s) => {
                let mut parts: Vec<String> =
                    s.cases.iter().map(|(v, t)| format!("{v}:@{t}")).collect();
                parts.push(format!("_:@{}", s.default));
                format!("{{{}}}", parts.join(", "))
            }
            None => format!("{{bad switch {}}}", self.0),
        }
    }
    fn parse(t: &str, b: &mut Builder) -> Option<Self> {
        let inner = t.strip_prefix('{')?.strip_suffix('}')?;
        let mut cases = Vec::new();
        let mut default = None;
        for part in inner.split(',') {
            let (k, v) = part.trim().split_once(':')?;
            let target: u32 = num(v.trim(), "@")?;
            if k.trim() == "_" {
                default = Some(target);
            } else {
                cases.push((k.trim().parse().ok()?, target));
            }
        }
        b.switches.push(SwitchTable {
            cases,
            default: default?,
        });
        Some(Swt((b.switches.len() - 1) as u32))
    }
    fn visit(&self, v: &mut dyn FnMut(Visit)) {
        v(Visit::Switch(self.0))
    }
}

macro_rules! define_instrs {
    ($( $(#[$doc:meta])* $v:ident = $mn:literal ( $( $f:ident : $t:ty ),* ) ),* $(,)?) => {
        #[derive(Clone, Debug, PartialEq)]
        pub enum Instr { $( $(#[$doc])* $v { $( $f: $t ),* } ),* }

        impl Instr {
            pub fn mnemonic(&self) -> &'static str {
                match self { $( Instr::$v { .. } => $mn ),* }
            }
            pub fn operands(&self, func: &BcFunc) -> Vec<String> {
                match self { $( Instr::$v { $( $f ),* } => vec![ $( OperandKind::show($f, func) ),* ] ),* }
            }
            pub fn visit(&self, v: &mut dyn FnMut(Visit)) {
                match self { $( Instr::$v { $( $f ),* } => { $( OperandKind::visit($f, v); )* } ),* }
            }
            pub fn parse(mn: &str, toks: &[&str], b: &mut Builder) -> Option<Instr> {
                match mn {
                    $( $mn => {
                        #[allow(unused_mut)]
                        let mut it = toks.iter();
                        $( let $f = <$t as OperandKind>::parse(it.next()?, b)?; )*
                        if it.next().is_some() { return None; }
                        Some(Instr::$v { $( $f ),* })
                    } ),*
                    _ => None,
                }
            }
        }
    };
}

define_instrs! {
    /// `d = s`
    Move = "move"(d: Reg, s: Reg),
    /// `d = constant pool[k]` (string or large integer)
    LoadK = "loadk"(d: Reg, k: Kst),
    LoadI = "loadi"(d: Reg, v: Imm),
    LoadB = "loadb"(d: Reg, v: Flag),
    LoadUnit = "loadunit"(d: Reg),
    /// `d = function value`
    LoadFn = "loadfn"(d: Reg, f: Fid),
    // checked integer arithmetic (traps on overflow / division by zero)
    Add = "add"(d: Reg, a: Reg, b: Reg),
    Sub = "sub"(d: Reg, a: Reg, b: Reg),
    Mul = "mul"(d: Reg, a: Reg, b: Reg),
    Div = "div"(d: Reg, a: Reg, b: Reg),
    Rem = "rem"(d: Reg, a: Reg, b: Reg),
    AddI = "addi"(d: Reg, a: Reg, v: Imm),
    SubI = "subi"(d: Reg, a: Reg, v: Imm),
    MulI = "muli"(d: Reg, a: Reg, v: Imm),
    Neg = "neg"(d: Reg, a: Reg),
    // comparisons: integer, string, and structural equality on any value
    EqI = "eqi"(d: Reg, a: Reg, b: Reg),
    NeI = "nei"(d: Reg, a: Reg, b: Reg),
    LtI = "lti"(d: Reg, a: Reg, b: Reg),
    LeI = "lei"(d: Reg, a: Reg, b: Reg),
    GtI = "gti"(d: Reg, a: Reg, b: Reg),
    GeI = "gei"(d: Reg, a: Reg, b: Reg),
    LtS = "lts"(d: Reg, a: Reg, b: Reg),
    LeS = "les"(d: Reg, a: Reg, b: Reg),
    GtS = "gts"(d: Reg, a: Reg, b: Reg),
    GeS = "ges"(d: Reg, a: Reg, b: Reg),
    Eq = "eq"(d: Reg, a: Reg, b: Reg),
    Ne = "ne"(d: Reg, a: Reg, b: Reg),
    Not = "not"(d: Reg, a: Reg),
    /// `bool as int`
    Cast = "cast"(d: Reg, a: Reg),
    Concat = "concat"(d: Reg, a: Reg, b: Reg),
    /// Build a tuple, struct (tag 0) or enum variant (tag = variant index) from the registers of a list.
    MkAgg = "mkagg"(d: Reg, tag: Idx, list: Lst),
    MkArray = "mkarray"(d: Reg, list: Lst),
    GetField = "getfield"(d: Reg, a: Reg, i: Idx),
    /// In-place copy-on-write update of the aggregate in `r`.
    SetField = "setfield"(r: Reg, i: Idx, v: Reg),
    Tag = "tag"(d: Reg, a: Reg),
    /// Bounds-checked array read.
    Index = "index"(d: Reg, a: Reg, i: Reg),
    /// Array read whose bounds the optimiser proved (still memory safe: a miss is an internal error).
    IndexU = "indexu"(d: Reg, a: Reg, i: Reg),
    SetIndex = "setindex"(a: Reg, i: Reg, v: Reg),
    SetIndexU = "setindexu"(a: Reg, i: Reg, v: Reg),
    /// Length of an array or the byte length of a string.
    Len = "len"(d: Reg, a: Reg),
    Push = "push"(a: Reg, v: Reg),
    Pop = "pop"(d: Reg, a: Reg),
    Chars = "chars"(d: Reg, a: Reg),
    ToStr = "tostr"(d: Reg, a: Reg),
    Print = "print"(a: Reg),
    Println = "println"(a: Reg),
    PrintlnEmpty = "println0"(),
    Assert = "assert"(c: Reg),
    AssertMsg = "assertmsg"(c: Reg, m: Reg),
    Panic = "panic"(m: Reg),
    Jmp = "jmp"(t: Tgt),
    JmpIf = "jmpif"(c: Reg, t: Tgt),
    JmpIfNot = "jmpifnot"(c: Reg, t: Tgt),
    /// Fused compare-and-branch on integers: jump when `a < b`.
    JmpLt = "jmplt"(a: Reg, b: Reg, t: Tgt),
    JmpGe = "jmpge"(a: Reg, b: Reg, t: Tgt),
    /// Compare-and-branch against an immediate: jump when `a < v`, `a >= v`, `a > v`, `a <= v`.
    JmpLtI = "jmplti"(a: Reg, v: Imm, t: Tgt),
    JmpGeI = "jmpgei"(a: Reg, v: Imm, t: Tgt),
    JmpGtI = "jmpgti"(a: Reg, v: Imm, t: Tgt),
    JmpLeI = "jmplei"(a: Reg, v: Imm, t: Tgt),
    Switch = "switch"(r: Reg, s: Swt),
    Call = "call"(d: Reg, f: Fid, args: Lst),
    CallInd = "callind"(d: Reg, fr: Reg, args: Lst),
    Ret = "ret"(a: Reg),
    Unreachable = "unreachable"(),
}

impl Instr {
    /// Can control continue to the next instruction?
    pub fn falls_through(&self) -> bool {
        !matches!(
            self,
            Instr::Jmp { .. } | Instr::Ret { .. } | Instr::Switch { .. } | Instr::Unreachable {}
        )
    }
}
