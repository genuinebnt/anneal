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

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, DecodeError> {
        let v = *self.b.get(self.p).ok_or(DecodeError::Truncated)?;
        self.p += 1;
        Ok(v)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        let mut result: u32 = 0;
        for i in 0..5 {
            let b = self.byte()?;
            if i == 4 && b & 0xF0 != 0 {
                return Err(DecodeError::BadLeb); // a 5th byte may carry only 4 payload bits and no continuation
            }
            if i > 0 && b == 0 {
                return Err(DecodeError::BadLeb);
            }
            result |= ((b & 0x7F) as u32) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(result);
            }
        }
        Err(DecodeError::BadLeb)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.p.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self.b.get(self.p..end).ok_or(DecodeError::Truncated)?;
        self.p = end;
        Ok(s)
    }

    /// An element count. Every element is at least one byte, so a count beyond what is left is truncation already.
    fn count(&mut self) -> Result<usize, DecodeError> {
        let n = self.u32()? as usize;
        if n > self.b.len() - self.p {
            return Err(DecodeError::Truncated);
        }
        Ok(n)
    }

    fn name(&mut self) -> Result<String, DecodeError> {
        let n = self.u32()? as usize;
        let raw = self.take(n)?;
        String::from_utf8(raw.to_vec()).map_err(|_| DecodeError::BadUtf8)
    }

    fn valtype(&mut self) -> Result<u8, DecodeError> {
        let b = self.byte()?;
        if (0x7C..=0x7F).contains(&b) {
            Ok(b)
        } else {
            Err(DecodeError::BadType)
        }
    }

    fn valtypes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let n = self.count()?;
        (0..n).map(|_| self.valtype()).collect()
    }

    fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.p..];
        self.p = self.b.len();
        s
    }
}

pub fn read_u32(bytes: &[u8]) -> Result<(u32, usize), DecodeError> {
    let mut r = Reader { b: bytes, p: 0 };
    let v = r.u32()?;
    Ok((v, r.p))
}

pub fn decode(bytes: &[u8]) -> Result<Module, DecodeError> {
    if bytes.len() < 4 {
        return Err(DecodeError::Truncated);
    }
    if &bytes[..4] != b"\0asm" {
        return Err(DecodeError::BadMagic);
    }
    if bytes.len() < 8 {
        return Err(DecodeError::Truncated);
    }
    if bytes[4..8] != [1, 0, 0, 0] {
        return Err(DecodeError::BadVersion);
    }
    let mut r = Reader { b: bytes, p: 8 };
    let mut m = Module::default();
    let mut last = 0u8;
    while r.p < r.b.len() {
        let id = r.byte()?;
        let size = r.u32()? as usize;
        let payload = r.take(size)?; // the section reads only inside its own bytes
        let mut s = Reader { b: payload, p: 0 };
        match id {
            0 => {
                let name = s.name()?;
                m.customs.push((name, s.rest().to_vec()));
            }
            1..=11 => {
                if id <= last {
                    return Err(DecodeError::SectionOutOfOrder);
                }
                last = id;
                match id {
                    1 => {
                        for _ in 0..s.count()? {
                            if s.byte()? != 0x60 {
                                return Err(DecodeError::BadType);
                            }
                            let params = s.valtypes()?;
                            let results = s.valtypes()?;
                            m.types.push(FuncType { params, results });
                        }
                    }
                    3 => {
                        for _ in 0..s.count()? {
                            m.funcs.push(s.u32()?);
                        }
                    }
                    7 => {
                        for _ in 0..s.count()? {
                            let name = s.name()?;
                            let kind = s.byte()?;
                            if kind > 3 {
                                return Err(DecodeError::BadType);
                            }
                            let index = s.u32()?;
                            m.exports.push(Export { name, kind, index });
                        }
                    }
                    10 => {
                        for _ in 0..s.count()? {
                            let size = s.u32()? as usize;
                            let mut body = Reader { b: s.take(size)?, p: 0 };
                            let mut locals = Vec::new();
                            let mut total: u64 = 0;
                            for _ in 0..body.count()? {
                                let n = body.u32()?;
                                let t = body.valtype()?;
                                total += n as u64;
                                if total > 50_000 {
                                    return Err(DecodeError::TooManyLocals);
                                }
                                locals.push((n, t));
                            }
                            let code = body.rest().to_vec();
                            if code.last() != Some(&0x0B) {
                                return Err(DecodeError::BadBody);
                            }
                            m.bodies.push(Body { locals, code });
                        }
                    }
                    _ => {
                        s.rest(); // imports, tables, memories, globals, start, elements, data: skipped by size
                    }
                }
            }
            other => return Err(DecodeError::BadSection(other)),
        }
        if s.p != s.b.len() {
            return Err(DecodeError::SectionSize);
        }
    }
    if m.funcs.len() != m.bodies.len() {
        return Err(DecodeError::CountMismatch);
    }
    Ok(m)
}
