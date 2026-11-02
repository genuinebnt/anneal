/// A function signature. Value types are the wasm bytes: 0x7F i32, 0x7E i64, 0x7D f32, 0x7C f64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<u8>,
    pub results: Vec<u8>,
}

/// One export. `kind`: 0 function, 1 table, 2 memory, 3 global.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: u8,
    pub index: u32,
}

/// One function body: its locals as `(count, type)` runs, and its instruction bytes (including the final 0x0B `end`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub locals: Vec<(u32, u8)>,
    pub code: Vec<u8>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Module {
    pub types: Vec<FuncType>,
    /// The function section: the type index of each function.
    pub funcs: Vec<u32>,
    pub exports: Vec<Export>,
    pub bodies: Vec<Body>,
    /// Custom sections as `(name, payload)`, in file order.
    pub customs: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    BadMagic,
    BadVersion,
    /// The input (or the section being read) ended in the middle of a value.
    Truncated,
    /// A LEB128 integer that is too long or has bits set beyond 32.
    BadLeb,
    BadUtf8,
    /// A wrong function-type form byte, value type, or export kind.
    BadType,
    /// A function body that is empty of `end` (does not finish with 0x0B).
    BadBody,
    /// A section id that does not exist.
    BadSection(u8),
    /// A non-custom section that repeats or comes before one that must precede it.
    SectionOutOfOrder,
    /// A section whose content does not fill exactly the size it declared.
    SectionSize,
    /// The function section and the code section have different lengths.
    CountMismatch,
    /// More than 50 000 locals in one function.
    TooManyLocals,
}

/// Reads one unsigned LEB128 `u32` from the front of `bytes`: the value and how many bytes it used.
/// At most five bytes; in the fifth only the low four bits may be set. Padded encodings (`0x80 0x00` for 0) are legal.
pub fn read_u32(bytes: &[u8]) -> Result<(u32, usize), DecodeError> {
    todo!()
}

/// Decodes a whole binary module.
pub fn decode(bytes: &[u8]) -> Result<Module, DecodeError> {
    todo!()
}
