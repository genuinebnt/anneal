from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
# 1. The binary decoder
DEC_TYPES = """
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
"""

DEC_STARTER = DEC_TYPES + """
/// Reads one unsigned LEB128 `u32` from the front of `bytes`: the value and how many bytes it used.
/// At most five bytes; in the fifth only the low four bits may be set. Padded encodings (`0x80 0x00` for 0) are legal.
pub fn read_u32(bytes: &[u8]) -> Result<(u32, usize), DecodeError> {
    todo!()
}

/// Decodes a whole binary module.
pub fn decode(bytes: &[u8]) -> Result<Module, DecodeError> {
    todo!()
}
"""

DEC = DEC_TYPES + """
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
    if &bytes[..4] != b"\\0asm" {
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
"""

ADD = "[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, 0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B]"
HDR = "[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00"

DEC_HELPERS = """
        const TYPE: [u8; 9] = [0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F];
        const FUNC: [u8; 4] = [0x03, 0x02, 0x01, 0x00];
        const EXPORT: [u8; 9] = [0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00];
        const CODE: [u8; 11] = [0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B];

        fn module(parts: &[&[u8]]) -> Vec<u8> {
            let mut v = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
            for p in parts {
                v.extend_from_slice(p);
            }
            v
        }
"""

P.append(dict(
    slug="wasm-binary-decoder", title="Decode a WebAssembly binary module", level="medium", stage="read-it",
    tags=["WebAssembly", "binary format", "LEB128", "sections", "validation", "bounded reads", "wasmparser"],
    teaches=["A `.wasm` file is a header plus a list of `[id][size][payload]` sections: the size lets a reader skip what it does not understand.",
             "LEB128: seven payload bits per byte, high bit means 'more'. The spec bounds a `u32` to five bytes and zeroes the unused bits, so a hostile file cannot make you loop or overflow.",
             "Read each section through a reader limited to its own payload: that one decision turns 'ran past the end' and 'left bytes over' into clean errors."],
    statement="""
        A WebAssembly module is a binary file: the 8-byte header `00 61 73 6D` (`\\0asm`) `01 00 00 00`, then **sections**, each
        `[id: u8][size: u32 LEB128][payload of size bytes]`. Write the decoder real engines and tools (`wasmparser`, `wasmtime`,
        `wasm-tools`) start with.

        `read_u32` decodes one **LEB128** `u32` (7 bits per byte, least significant group first, bit 7 means another byte follows):
        at most five bytes, and in the fifth only the low four bits may be set (anything else is `BadLeb`). Padded encodings such
        as `80 00` for 0 are legal. Running out of bytes is `Truncated`.

        `decode` builds a `Module` from sections `1` (types: `0x60`, parameter list, result list), `3` (functions: a type index each),
        `7` (exports: name, kind `0..=3`, index) and `10` (code: per function a size, runs of `(count, type)` locals, then the
        instruction bytes, which must end with `0x0B`). Sections `2, 4, 5, 6, 8, 9, 11` are valid but ignored (skip them by their
        size). Section `0` is a custom section (a name, then raw bytes) and may appear anywhere, any number of times. Every other
        section may appear at most once and in increasing id order (`SectionOutOfOrder`). Any other id is `BadSection(id)`.

        Each section is read **inside its own payload**: ending early is `Truncated`, and leaving bytes unread is `SectionSize`.
        The function and code sections must have the same length (`CountMismatch`), and a function may declare at most 50 000
        locals in total (`TooManyLocals`).
    """,
    starter=DEC_STARTER,
    solution=DEC,
    visible=[
        T("leb_classic", "the textbook example", "read_u32(&[0xE5, 0x8E, 0x26, 0xFF])", "Ok((624485, 3))"),
        T("leb_padded_is_legal", "80 00 is 0", "read_u32(&[0x80, 0x00, 0x7F])", "Ok((0, 2))"),
        T("decode_add_module", "the classic add(i32, i32)", "{ let m = decode(&" + ADD + ").unwrap(); (m.types, m.funcs, m.exports, m.bodies) }",
          "(vec![FuncType { params: vec![0x7F, 0x7F], results: vec![0x7F] }], vec![0], vec![Export { name: \"add\".to_string(), kind: 0, index: 0 }], vec![Body { locals: vec![], code: vec![0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B] }])"),
        T("header_only", "an empty module", "decode(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00])", "Ok(Module::default())"),
        T("bad_magic", "not wasm", "decode(&[0x00, 0x61, 0x73, 0x78, 0x01, 0x00, 0x00, 0x00])", "Err(DecodeError::BadMagic)"),
        T("bad_version", "version 2", "decode(&[0x00, 0x61, 0x73, 0x6D, 0x02, 0x00, 0x00, 0x00])", "Err(DecodeError::BadVersion)"),
        T("section_runs_past_the_end", "size says 7, one byte left", "decode(&" + HDR + ", 0x01, 0x07, 0x01])", "Err(DecodeError::Truncated)"),
    ],
    hidden=[
        T("leb_five_byte_limit", "u32::MAX fits, one more bit does not", "(read_u32(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]), read_u32(&[0xFF, 0xFF, 0xFF, 0xFF, 0x1F]), read_u32(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]))", "(Ok((u32::MAX, 5)), Err(DecodeError::BadLeb), Err(DecodeError::BadLeb))"),
        T("leb_truncated", "ends mid-integer", "(read_u32(&[0x80]), read_u32(&[]), read_u32(&[0x81, 0x80]))", "(Err(DecodeError::Truncated), Err(DecodeError::Truncated), Err(DecodeError::Truncated))"),
        T("leb_small_values", "one byte and two bytes", "(read_u32(&[0x00]), read_u32(&[0x7F]), read_u32(&[0x80, 0x01]), read_u32(&[0xFF, 0x7F]))", "(Ok((0, 1)), Ok((127, 1)), Ok((128, 2)), Ok((16383, 2)))"),
        T("truncated_header", "too short for the magic or the version", "(decode(&[]), decode(&[0x00, 0x61]), decode(&[0x00, 0x61, 0x73, 0x6D, 0x01]))", "(Err(DecodeError::Truncated), Err(DecodeError::Truncated), Err(DecodeError::Truncated))"),
        DEC_HELPERS + """
        #[test]
        fn sections_must_ascend_and_not_repeat() {
            check!("function before type", decode(&module(&[&FUNC, &TYPE])), Err(DecodeError::SectionOutOfOrder));
            check!("type twice", decode(&module(&[&TYPE, &TYPE])), Err(DecodeError::SectionOutOfOrder));
            check!("ascending is fine", decode(&module(&[&TYPE, &FUNC, &EXPORT, &CODE])).is_ok(), true);
        }

        #[test]
        fn custom_sections_go_anywhere() {
            let c1: [u8; 9] = [0x00, 0x07, 0x04, b'n', b'a', b'm', b'e', 0x09, 0x09];
            let c2: [u8; 4] = [0x00, 0x02, 0x01, b'x'];
            let m = decode(&module(&[&c1, &TYPE, &c2, &FUNC, &EXPORT, &CODE, &c2])).unwrap();
            check!("three customs in file order", m.customs, vec![("name".to_string(), vec![9, 9]), ("x".to_string(), vec![]), ("x".to_string(), vec![])]);
            check!("the rest is still parsed", (m.funcs, m.bodies.len()), (vec![0], 1));
        }

        #[test]
        fn known_but_ignored_sections_are_skipped_by_size() {
            let memory: [u8; 5] = [0x05, 0x03, 0x01, 0x00, 0x01];
            let m = decode(&module(&[&TYPE, &FUNC, &memory, &EXPORT, &CODE])).unwrap();
            check!("exports after the memory section", m.exports.len(), 1);
            check!("memory after export is out of order", decode(&module(&[&EXPORT, &memory])), Err(DecodeError::SectionOutOfOrder));
        }

        #[test]
        fn unknown_section_ids() {
            check!("id 13", decode(&module(&[&[13, 0]])), Err(DecodeError::BadSection(13)));
            check!("id 12 (datacount) is not supported here", decode(&module(&[&[12, 1, 0]])), Err(DecodeError::BadSection(12)));
        }

        #[test]
        fn a_section_is_read_inside_its_own_bytes() {
            let trailing: [u8; 10] = [0x01, 0x08, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F, 0x00];
            check!("a stray byte inside the section", decode(&module(&[&trailing])), Err(DecodeError::SectionSize));
            let short: [u8; 9] = [0x01, 0x06, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F];
            check!("content longer than the declared size", decode(&module(&[&short])), Err(DecodeError::Truncated));
        }

        #[test]
        fn type_errors() {
            let form: [u8; 6] = [0x01, 0x04, 0x01, 0x61, 0x00, 0x00];
            check!("function type must start with 0x60", decode(&module(&[&form])), Err(DecodeError::BadType));
            let valtype: [u8; 7] = [0x01, 0x05, 0x01, 0x60, 0x01, 0x70, 0x00];
            check!("0x70 is not a number type", decode(&module(&[&valtype])), Err(DecodeError::BadType));
            let kind: [u8; 9] = [0x07, 0x07, 0x01, 0x03, b'a', b'd', b'd', 0x04, 0x00];
            check!("export kind 4", decode(&module(&[&kind])), Err(DecodeError::BadType));
        }

        #[test]
        fn utf8_in_names() {
            let bad: [u8; 7] = [0x07, 0x05, 0x01, 0x01, 0xFF, 0x00, 0x00];
            check!("0xFF is not UTF-8", decode(&module(&[&bad])), Err(DecodeError::BadUtf8));
            let ok: [u8; 10] = [0x07, 0x08, 0x01, 0x04, 0xF0, 0x9F, 0xA6, 0x80, 0x00, 0x00];
            check!("a four-byte code point", decode(&module(&[&ok])).unwrap().exports[0].name.clone(), "\\u{1F980}".to_string());
        }

        #[test]
        fn function_and_code_sections_must_pair_up() {
            check!("no code", decode(&module(&[&TYPE, &FUNC, &EXPORT])), Err(DecodeError::CountMismatch));
            check!("code without functions", decode(&module(&[&TYPE, &CODE])), Err(DecodeError::CountMismatch));
        }

        #[test]
        fn locals_and_bodies() {
            let locals: [u8; 10] = [0x0A, 0x08, 0x01, 0x06, 0x02, 0x02, 0x7F, 0x01, 0x7E, 0x0B];
            let m = decode(&module(&[&TYPE, &FUNC, &locals])).unwrap();
            check!("locals", m.bodies[0].locals.clone(), vec![(2, 0x7F), (1, 0x7E)]);
            check!("code is just the end", m.bodies[0].code.clone(), vec![0x0B]);
            let no_end: [u8; 7] = [0x0A, 0x05, 0x01, 0x03, 0x00, 0x20, 0x00];
            check!("a body must finish with end", decode(&module(&[&TYPE, &FUNC, &no_end])), Err(DecodeError::BadBody));
        }

        #[test]
        fn local_count_limit() {
            let exactly: [u8; 10] = [0x0A, 0x08, 0x01, 0x06, 0x01, 0xD0, 0x86, 0x03, 0x7F, 0x0B];
            check!("50 000 locals is allowed", decode(&module(&[&TYPE, &FUNC, &exactly])).is_ok(), true);
            let over: [u8; 12] = [0x0A, 0x0A, 0x01, 0x08, 0x02, 0xD0, 0x86, 0x03, 0x7F, 0x01, 0x7F, 0x0B];
            check!("50 001 is not", decode(&module(&[&TYPE, &FUNC, &over])), Err(DecodeError::TooManyLocals));
        }
        """,
    ],
    wrong=dict(
        no_order_check=sub(DEC, "                if id <= last {\n                    return Err(DecodeError::SectionOutOfOrder);\n                }\n", ""),
        fifth_leb_byte_unchecked=sub(DEC, "            if i == 4 && b & 0xF0 != 0 {\n                return Err(DecodeError::BadLeb); // a 5th byte may carry only 4 payload bits and no continuation\n            }\n", ""),
        padded_leb_rejected=sub(DEC, "            result |= ((b & 0x7F) as u32) << (7 * i);\n", "            if i > 0 && b == 0 {\n                return Err(DecodeError::BadLeb);\n            }\n            result |= ((b & 0x7F) as u32) << (7 * i);\n"),
        trailing_bytes_ignored=sub(DEC, "        if s.p != s.b.len() {\n            return Err(DecodeError::SectionSize);\n        }\n", ""),
        reads_past_the_section=sub(DEC, "        let mut s = Reader { b: payload, p: 0 };", "        let _ = payload;\n        let mut s = Reader { b: bytes, p: r.p - size };"),
        count_mismatch_ignored=sub(DEC, "    if m.funcs.len() != m.bodies.len() {\n        return Err(DecodeError::CountMismatch);\n    }\n", ""),
        body_end_unchecked=sub(DEC, "                            if code.last() != Some(&0x0B) {\n                                return Err(DecodeError::BadBody);\n                            }\n", ""),
        locals_unlimited=sub(DEC, "                                if total > 50_000 {\n                                    return Err(DecodeError::TooManyLocals);\n                                }\n", ""),
        export_kind_unchecked=sub(DEC, "                            if kind > 3 {\n                                return Err(DecodeError::BadType);\n                            }\n", ""),
        magic_unchecked=sub(DEC, "    if &bytes[..4] != b\"\\0asm\" {\n        return Err(DecodeError::BadMagic);\n    }\n", ""),
    ),
    hints=[("approach", "Write a small `Reader` over a slice with `byte`, `u32` (LEB), `take(n)`, `name`. For the file, one reader; for each section, `take(size)` and make a **second** reader over just that payload. Then 'did it end early' and 'is anything left' are both a comparison with the payload's length."),
           ("rust", "In the LEB loop, shift by `7 * i` in a `u32`; on `i == 4` reject `b & 0xF0 != 0` (that covers the continuation bit and the bits above bit 31). `(0..n).map(|_| r.valtype()).collect::<Result<Vec<_>, _>>()` collects a counted vector and stops at the first error. Compare the header with `bytes[4..8] != [1, 0, 0, 0]`."),
           ("edge", "Check `count <= bytes_left` before looping: a hostile file can claim four billion elements in a two-byte section, and you should fail without allocating for them. Sum the locals in a `u64`, then compare with 50 000, so two enormous runs cannot wrap a `u32` back under the limit.")],
    notes=("This is the front door of every WebAssembly tool: `wasmparser` is the same loop with more sections. The size-prefixed section is the format's extension mechanism (engines skip custom sections such as `name` and debug info), and bounded LEB128 plus count-before-allocate is the difference between a decoder and an attack surface: the real-world CVEs here are all 'a length or count was trusted'.", "O(n) in the module size", "O(n) for the decoded module"),
    follow_up="1) Decode the import section and the memory section (limits) instead of skipping them, and add the data section so you can read a module's initial memory. 2) Add the `name` custom section parser (function names by index) and print a call-graph-free disassembly of `add`. 3) Add the section-12 `datacount` rule (it sits between the element and code sections in the order). 4) Fuzz: generate random byte strings and mutations of valid modules; the decoder must never panic or allocate more than the input size times a small constant. 5) Write the encoder (`Module -> Vec<u8>`, signed and unsigned LEB128) and check `decode(encode(m)) == m` as a property.",
    related=["B4", "F5", "Y2"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. The interpreter
INTERP_TYPES = """
/// One function: how many parameters and results it has, how many extra (zero-initialised) locals, and its instruction
/// bytes, which end with the `end` opcode 0x0B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub params: usize,
    pub results: usize,
    pub locals: usize,
    pub code: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
    Unreachable,
    DivideByZero,
    IntegerOverflow,
    OutOfFuel,
    /// More than 100 active calls.
    StackOverflow,
    /// A call (or the entry point) named a function that does not exist.
    BadFunction(usize),
    /// Unknown opcode, bad immediate, stack underflow, bad local or label index, unbalanced blocks, wrong argument count.
    Malformed,
}

/// Reads an unsigned LEB128 immediate at `*pc` and moves `*pc` past it. (Provided.)
fn read_u32(code: &[u8], pc: &mut usize) -> Result<u32, Trap> {
    let mut result: u32 = 0;
    for i in 0..5 {
        let b = *code.get(*pc).ok_or(Trap::Malformed)?;
        *pc += 1;
        result |= ((b & 0x7F) as u32).checked_shl(7 * i).ok_or(Trap::Malformed)?;
        if b & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(Trap::Malformed)
}

/// Reads a signed LEB128 immediate (an `i32.const` operand) at `*pc` and moves `*pc` past it. (Provided.)
fn read_i32(code: &[u8], pc: &mut usize) -> Result<i32, Trap> {
    let mut result: i64 = 0;
    let mut shift = 0;
    loop {
        let b = *code.get(*pc).ok_or(Trap::Malformed)?;
        *pc += 1;
        result |= ((b & 0x7F) as i64) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            if b & 0x40 != 0 {
                result |= -1i64 << shift;
            }
            return Ok(result as i32);
        }
        if shift >= 35 {
            return Err(Trap::Malformed);
        }
    }
}
"""

INTERP_STARTER = INTERP_TYPES + """
/// Runs function `entry` with `args` and returns its results. Every instruction executed costs one unit of `fuel`;
/// running out is `Trap::OutOfFuel`.
pub fn run(funcs: &[Func], entry: usize, args: &[i32], fuel: u64) -> Result<Vec<i32>, Trap> {
    todo!()
}
"""

INTERP = INTERP_TYPES + """
use std::collections::HashMap;

/// For each `block`/`loop`/`if` pc: where its `end` is and where its `else` is. For each `else` pc: where its `end` is.
type Jumps = HashMap<usize, (usize, Option<usize>)>;

/// The pc after the instruction at `pc` (skipping its immediates), or `Malformed` for an unknown opcode.
fn next_pc(code: &[u8], pc: usize) -> Result<usize, Trap> {
    let op = code[pc];
    let mut q = pc + 1;
    match op {
        0x00 | 0x01 | 0x05 | 0x0B | 0x0F | 0x1A | 0x1B | 0x45..=0x4F | 0x6A..=0x6D | 0x6F | 0x71..=0x76 => {}
        0x02..=0x04 => q += 1,
        0x0C | 0x0D | 0x10 | 0x20..=0x22 => {
            read_u32(code, &mut q)?;
        }
        0x41 => {
            read_i32(code, &mut q)?;
        }
        _ => return Err(Trap::Malformed),
    }
    if q > code.len() {
        return Err(Trap::Malformed);
    }
    Ok(q)
}

fn analyze(code: &[u8]) -> Result<Jumps, Trap> {
    let mut jumps = Jumps::new();
    let mut open: Vec<(usize, Option<usize>)> = Vec::new(); // (block pc, else pc)
    let mut pc = 0;
    let mut closed = false;
    while pc < code.len() {
        if closed {
            return Err(Trap::Malformed); // bytes after the function's own end
        }
        match code[pc] {
            0x02..=0x04 => open.push((pc, None)),
            0x05 => {
                let top = open.last_mut().ok_or(Trap::Malformed)?;
                if code[top.0] != 0x04 || top.1.is_some() {
                    return Err(Trap::Malformed);
                }
                top.1 = Some(pc);
            }
            0x0B => match open.pop() {
                Some((start, els)) => {
                    jumps.insert(start, (pc, els));
                    if let Some(e) = els {
                        jumps.insert(e, (pc, None));
                    }
                }
                None => closed = true,
            },
            _ => {}
        }
        pc = next_pc(code, pc)?;
    }
    if !closed || !open.is_empty() {
        return Err(Trap::Malformed);
    }
    Ok(jumps)
}

struct Label {
    is_loop: bool,
    cont: usize,
    height: usize,
    arity: usize,
}

fn block_arity(code: &[u8], pc: usize) -> Result<usize, Trap> {
    match code.get(pc + 1) {
        Some(0x40) => Ok(0),
        Some(0x7F) => Ok(1),
        _ => Err(Trap::Malformed),
    }
}

fn pop(stack: &mut Vec<i32>) -> Result<i32, Trap> {
    stack.pop().ok_or(Trap::Malformed)
}

/// Branch to label `depth`: keep the label's arity values, drop the rest of that block's stack, and continue at its target.
fn branch(labels: &mut Vec<Label>, stack: &mut Vec<i32>, depth: usize, pc: &mut usize) -> Result<(), Trap> {
    if depth >= labels.len() {
        return Err(Trap::Malformed);
    }
    let li = labels.len() - 1 - depth;
    let (is_loop, cont, height, arity) = {
        let l = &labels[li];
        (l.is_loop, l.cont, l.height, l.arity)
    };
    if stack.len() < height + arity {
        return Err(Trap::Malformed);
    }
    let keep = stack.split_off(stack.len() - arity);
    stack.truncate(height);
    stack.extend(keep);
    labels.truncate(if is_loop { li + 1 } else { li });
    *pc = cont;
    Ok(())
}

fn binop(op: u8, a: i32, b: i32) -> Result<i32, Trap> {
    Ok(match op {
        0x46 => (a == b) as i32,
        0x47 => (a != b) as i32,
        0x48 => (a < b) as i32,
        0x49 => ((a as u32) < (b as u32)) as i32,
        0x4A => (a > b) as i32,
        0x4B => ((a as u32) > (b as u32)) as i32,
        0x4C => (a <= b) as i32,
        0x4D => ((a as u32) <= (b as u32)) as i32,
        0x4E => (a >= b) as i32,
        0x4F => ((a as u32) >= (b as u32)) as i32,
        0x6A => a.wrapping_add(b),
        0x6B => a.wrapping_sub(b),
        0x6C => a.wrapping_mul(b),
        0x6D => {
            if b == 0 {
                return Err(Trap::DivideByZero);
            }
            if a == i32::MIN && b == -1 {
                return Err(Trap::IntegerOverflow);
            }
            a / b
        }
        0x6F => {
            if b == 0 {
                return Err(Trap::DivideByZero);
            }
            a.wrapping_rem(b)
        }
        0x71 => a & b,
        0x72 => a | b,
        0x73 => a ^ b,
        0x74 => a.wrapping_shl(b as u32),
        0x75 => a.wrapping_shr(b as u32),
        0x76 => (a as u32).wrapping_shr(b as u32) as i32,
        _ => return Err(Trap::Malformed),
    })
}

fn exec(funcs: &[Func], all: &[Jumps], idx: usize, args: &[i32], fuel: &mut u64, depth: usize) -> Result<Vec<i32>, Trap> {
    let f = funcs.get(idx).ok_or(Trap::BadFunction(idx))?;
    if depth > 100 {
        return Err(Trap::StackOverflow);
    }
    if args.len() != f.params {
        return Err(Trap::Malformed);
    }
    let code = &f.code;
    let jumps = &all[idx];
    let mut locals: Vec<i32> = args.to_vec();
    locals.resize(f.params + f.locals, 0);
    let mut stack: Vec<i32> = Vec::new();
    let mut labels = vec![Label { is_loop: false, cont: code.len(), height: 0, arity: f.results }];
    let mut pc = 0;
    while pc < code.len() {
        if *fuel == 0 {
            return Err(Trap::OutOfFuel);
        }
        *fuel -= 1;
        let op = code[pc];
        let mut next = next_pc(code, pc)?;
        match op {
            0x00 => return Err(Trap::Unreachable),
            0x01 => {}
            0x02 | 0x03 => {
                let arity = block_arity(code, pc)?;
                let (end, _) = jumps[&pc];
                labels.push(if op == 0x03 {
                    Label { is_loop: true, cont: next, height: stack.len(), arity: 0 }
                } else {
                    Label { is_loop: false, cont: end + 1, height: stack.len(), arity }
                });
            }
            0x04 => {
                let arity = block_arity(code, pc)?;
                let cond = pop(&mut stack)?;
                let (end, els) = jumps[&pc];
                labels.push(Label { is_loop: false, cont: end + 1, height: stack.len(), arity });
                if cond == 0 {
                    next = match els {
                        Some(e) => e + 1,
                        None => end,
                    };
                }
            }
            0x05 => {
                next = jumps[&pc].0; // the then-branch is done: run the `end`
            }
            0x0B => {
                labels.pop();
            }
            0x0C | 0x0D => {
                let mut q = pc + 1;
                let d = read_u32(code, &mut q)? as usize;
                if op == 0x0C || pop(&mut stack)? != 0 {
                    branch(&mut labels, &mut stack, d, &mut next)?;
                }
            }
            0x0F => {
                let d = labels.len() - 1;
                branch(&mut labels, &mut stack, d, &mut next)?;
            }
            0x10 => {
                let mut q = pc + 1;
                let callee = read_u32(code, &mut q)? as usize;
                let n = funcs.get(callee).ok_or(Trap::BadFunction(callee))?.params;
                if stack.len() < n {
                    return Err(Trap::Malformed);
                }
                let a = stack.split_off(stack.len() - n);
                let r = exec(funcs, all, callee, &a, fuel, depth + 1)?;
                stack.extend(r);
            }
            0x1A => {
                pop(&mut stack)?;
            }
            0x1B => {
                let c = pop(&mut stack)?;
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(if c != 0 { a } else { b });
            }
            0x20..=0x22 => {
                let mut q = pc + 1;
                let i = read_u32(code, &mut q)? as usize;
                if i >= locals.len() {
                    return Err(Trap::Malformed);
                }
                match op {
                    0x20 => stack.push(locals[i]),
                    0x21 => locals[i] = pop(&mut stack)?,
                    _ => {
                        let v = *stack.last().ok_or(Trap::Malformed)?;
                        locals[i] = v;
                    }
                }
            }
            0x41 => {
                let mut q = pc + 1;
                stack.push(read_i32(code, &mut q)?);
            }
            0x45 => {
                let a = pop(&mut stack)?;
                stack.push((a == 0) as i32);
            }
            _ => {
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(binop(op, a, b)?);
            }
        }
        pc = next;
    }
    if stack.len() < f.results {
        return Err(Trap::Malformed);
    }
    Ok(stack.split_off(stack.len() - f.results))
}

pub fn run(funcs: &[Func], entry: usize, args: &[i32], fuel: u64) -> Result<Vec<i32>, Trap> {
    let all: Vec<Jumps> = funcs.iter().map(|f| analyze(&f.code)).collect::<Result<_, _>>()?;
    let mut fuel = fuel;
    exec(funcs, &all, entry, args, &mut fuel, 1)
}
"""


def hexs(s):
    return ", ".join("0x" + b for b in s.split())


def F(p, r, l, code):
    return f"Func {{ params: {p}, results: {r}, locals: {l}, code: vec![{hexs(code)}] }}"


def R(funcs, entry, args, fuel=100000):
    return f"run(&[{', '.join(funcs)}], {entry}, &[{args}], {fuel})"


ADDF = F(2, 1, 0, "20 00 20 01 6A 0B")
SUM = F(1, 1, 1, "02 40 03 40 20 00 45 0D 01 20 01 20 00 6A 21 01 20 00 41 01 6B 21 00 0C 00 0B 0B 20 01 0B")
ABS = F(1, 1, 0, "20 00 41 00 48 04 7F 41 00 20 00 6B 05 20 00 0B 0B")
FACT = F(1, 1, 0, "20 00 45 04 7F 41 01 05 20 00 20 00 41 01 6B 10 00 6C 0B 0B")
COUNTDOWN = F(1, 1, 0, "20 00 45 04 7F 41 00 05 20 00 41 01 6B 10 00 0B 0B")


def ONE(code, results=1, locals_=0, args="", fuel=100000):
    return R([F(len(args.split(",")) if args else 0, results, locals_, code)], 0, args, fuel)


INTERP_HELPERS = """
        fn func(params: usize, results: usize, locals: usize, code: &[u8]) -> Func {
            Func { params, results, locals, code: code.to_vec() }
        }

        fn sleb(mut v: i64, out: &mut Vec<u8>) {
            loop {
                let b = (v & 0x7F) as u8;
                v >>= 7;
                if (v == 0 && b & 0x40 == 0) || (v == -1 && b & 0x40 != 0) {
                    out.push(b);
                    return;
                }
                out.push(b | 0x80);
            }
        }
"""

P.append(dict(
    slug="wasm-stack-interpreter", title="A WebAssembly stack machine with structured control flow", level="hard", stage="run-it",
    tags=["WebAssembly", "interpreter", "stack machine", "structured control flow", "labels", "traps", "fuel", "i32 semantics"],
    teaches=["WebAssembly has no goto: `block`, `loop` and `if` are labels, and `br n` means 'leave (or, for a loop, restart) the n-th enclosing block'. A `br` also has to cut the operand stack back to the block's entry height.",
             "i32 arithmetic is two's complement everywhere and the spec is exact about the corners: wrapping add/mul, shifts that take the count mod 32, `MIN / -1` as a trap but `MIN % -1` as 0.",
             "A sandbox is the combination of traps (an error, never a crash), a call-depth cap and a fuel counter, all enforced by the interpreter itself."],
    statement="""
        Write the heart of a WebAssembly engine for the `i32` subset: an interpreter over **raw instruction bytes**, the way the
        spec describes it. A `Func` has `params`, `results`, extra zero-initialised `locals`, and `code` ending in `end` (`0x0B`).
        `run(funcs, entry, args, fuel)` calls function `entry` and returns its results, or a `Trap`.

        Instructions (opcode, immediate): `unreachable` 00, `nop` 01, `block bt` 02, `loop bt` 03, `if bt` 04, `else` 05, `end` 0B,
        `br l` 0C, `br_if l` 0D, `return` 0F, `call f` 10, `drop` 1A, `select` 1B (pops `c`, then `b`, then `a`; pushes `a` if
        `c != 0`, else `b`), `local.get/set/tee i` 20/21/22, `i32.const v` 41 (**signed** LEB128), `eqz` 45, `eq ne lt_s lt_u gt_s
        gt_u le_s le_u ge_s ge_u` 46..=4F, `add sub mul` 6A 6B 6C, `div_s` 6D, `rem_s` 6F, `and or xor` 71 72 73, `shl shr_s shr_u`
        74 75 76. A block type `bt` is `0x40` (no result) or `0x7F` (one `i32`). Helpers to read the LEB immediates are provided.

        Rules that are easy to get subtly wrong: arithmetic **wraps**; shift counts are taken **mod 32**; `div_s` and `rem_s` trap with
        `DivideByZero` on a zero divisor, `div_s` also traps `IntegerOverflow` for `i32::MIN / -1`, while `i32::MIN % -1` is `0`.
        `br l` to a `block`/`if` jumps past its `end`; to a `loop` it jumps back to the loop's start (loops take no values); either way
        the stack is cut back to the height at block entry plus the values the block yields. The function body is itself the outermost
        block, so `br` to it and `return` both finish the function. `if` with a false condition and no `else` skips to `end`.

        Every executed instruction costs one unit of fuel (including `end` and `else`); with none left the next instruction traps
        `OutOfFuel`. More than 100 active calls (the entry counts as one) is `StackOverflow`. All functions are scanned before running:
        an unknown opcode, an unbalanced `block`/`end`, bytes after the final `end`, a bad local or label index, a stack underflow or
        a wrong argument count is `Trap::Malformed`; calling a function that does not exist is `BadFunction(index)`.
    """,
    starter=INTERP_STARTER,
    solution=INTERP,
    visible=[
        T("add_two_params", "local.get 0, local.get 1, i32.add", R([ADDF], 0, "2, 3"), "Ok(vec![5])"),
        T("constants", "positive and negative i32.const", "(" + ONE("41 2A 0B") + ", " + ONE("41 7F 0B") + ")", "(Ok(vec![42]), Ok(vec![-1]))"),
        T("loop_sums_one_to_n", "block + loop + br_if + br", "(" + R([SUM], 0, "10") + ", " + R([SUM], 0, "0") + ")", "(Ok(vec![55]), Ok(vec![0]))"),
        T("if_else_abs", "if (result i32) ... else ... end", "(" + R([ABS], 0, "-5") + ", " + R([ABS], 0, "7") + ", " + R([ABS], 0, "0") + ")", "(Ok(vec![5]), Ok(vec![7]), Ok(vec![0]))"),
        T("recursion", "factorial through call 0", "(" + R([FACT], 0, "5") + ", " + R([FACT], 0, "10") + ")", "(Ok(vec![120]), Ok(vec![3628800]))"),
        T("unreachable_traps", "an error, not a panic", ONE("00 0B", results=0), "Err(Trap::Unreachable)"),
        T("infinite_loop_runs_out_of_fuel", "loop { br 0 }", ONE("03 40 0C 00 0B 0B", results=0, fuel=1000), "Err(Trap::OutOfFuel)"),
    ],
    hidden=[
        T("large_and_negative_constants", "sign extension and 5-byte LEBs", "(" + ONE("41 80 7F 0B") + ", " + ONE("41 FF FF FF FF 07 0B") + ", " + ONE("41 80 80 80 80 78 0B") + ", " + ONE("41 E4 00 0B") + ")", "(Ok(vec![-128]), Ok(vec![i32::MAX]), Ok(vec![i32::MIN]), Ok(vec![100]))"),
        T("arithmetic_wraps", "MAX + 1 and 65536 * 65536", "(" + ONE("41 FF FF FF FF 07 41 01 6A 0B") + ", " + ONE("41 80 80 04 41 80 80 04 6C 0B") + ", " + ONE("41 00 41 01 6B 0B") + ")", "(Ok(vec![i32::MIN]), Ok(vec![0]), Ok(vec![-1]))"),
        T("division_and_remainder", "signed, truncating", "(" + ONE("41 79 41 02 6D 0B") + ", " + ONE("41 79 41 03 6F 0B") + ", " + ONE("41 80 80 80 80 78 41 7F 6F 0B") + ")", "(Ok(vec![-3]), Ok(vec![-1]), Ok(vec![0]))"),
        T("division_traps", "zero divisor and MIN / -1", "(" + ONE("41 01 41 00 6D 0B") + ", " + ONE("41 01 41 00 6F 0B") + ", " + ONE("41 80 80 80 80 78 41 7F 6D 0B") + ")", "(Err(Trap::DivideByZero), Err(Trap::DivideByZero), Err(Trap::IntegerOverflow))"),
        T("shifts_take_the_count_mod_32", "1 << 33, -8 >> 1, -1 >>> 28", "(" + ONE("41 01 41 21 74 0B") + ", " + ONE("41 78 41 01 75 0B") + ", " + ONE("41 7F 41 1C 76 0B") + ")", "(Ok(vec![2]), Ok(vec![-4]), Ok(vec![15]))"),
        T("signed_and_unsigned_compare", "-1 vs 1", "(" + ONE("41 7F 41 01 48 0B") + ", " + ONE("41 7F 41 01 49 0B") + ", " + ONE("41 7F 41 01 4B 0B") + ", " + ONE("41 05 41 05 4F 0B") + ")", "(Ok(vec![1]), Ok(vec![0]), Ok(vec![1]), Ok(vec![1]))"),
        T("select_and_drop", "c != 0 picks the first", "(" + ONE("41 0A 41 14 41 01 1B 0B") + ", " + ONE("41 0A 41 14 41 00 1B 0B") + ", " + ONE("41 01 41 02 1A 0B") + ")", "(Ok(vec![10]), Ok(vec![20]), Ok(vec![1]))"),
        T("tee_keeps_the_value_and_locals_start_at_zero", "local.tee and an unset local", "(" + ONE("41 05 22 00 20 00 6A 0B", locals_=1) + ", " + R([F(1, 1, 2, "20 02 0B")], 0, "9") + ")", "(Ok(vec![10]), Ok(vec![0]))"),
        T("br_carries_a_value_and_cuts_the_stack", "values below the block survive, extra ones do not", "(" + ONE("02 7F 41 07 0C 00 41 09 0B 0B") + ", " + ONE("41 E4 00 02 7F 41 01 41 02 0C 00 0B 6A 0B") + ")", "(Ok(vec![7]), Ok(vec![102]))"),
        T("br_out_of_two_blocks", "br 1 skips the outer end", ONE("02 40 02 40 0C 01 0B 00 0B 0B", results=0), "Ok(vec![])"),
        T("br_if_taken_and_not_taken", "same block, two conditions", "(" + ONE("02 7F 41 09 41 01 0D 00 1A 41 04 0B 0B") + ", " + ONE("02 7F 41 09 41 00 0D 00 1A 41 04 0B 0B") + ")", "(Ok(vec![9]), Ok(vec![4]))"),
        T("return_from_inside_a_block", "return keeps the top result", ONE("02 40 41 03 0F 0B 41 04 0B"), "Ok(vec![3])"),
        T("if_without_else", "false skips, true runs", "(" + ONE("41 00 04 40 00 0B 41 08 0B") + ", " + ONE("41 01 04 40 00 0B 41 08 0B") + ")", "(Ok(vec![8]), Err(Trap::Unreachable))"),
        T("call_passes_arguments_in_order", "10 - 3 through call 1", R([F(0, 1, 0, "41 0A 41 03 10 01 0B"), F(2, 1, 0, "20 00 20 01 6B 0B")], 0, ""), "Ok(vec![7])"),
        T("several_results", "returned bottom to top", ONE("41 01 41 02 0B", results=2), "Ok(vec![1, 2])"),
        T("call_depth_is_capped_at_100", "99 recursions are fine, 100 are not", "(" + R([COUNTDOWN], 0, "99") + ", " + R([COUNTDOWN], 0, "100") + ", " + R([F(0, 0, 0, "10 00 0B")], 0, "", 1000000) + ")", "(Ok(vec![0]), Err(Trap::StackOverflow), Err(Trap::StackOverflow))"),
        T("fuel_is_exact", "const, drop, end is three units", "(" + ONE("41 01 1A 0B", results=0, fuel=3) + ", " + ONE("41 01 1A 0B", results=0, fuel=2) + ", " + ONE("0B", results=0, fuel=0) + ")", "(Ok(vec![]), Err(Trap::OutOfFuel), Err(Trap::OutOfFuel))"),
        T("missing_functions", "entry and call targets", "(run(&[], 0, &[], 10), " + ONE("10 05 0B", results=0) + ")", "(Err(Trap::BadFunction(0)), Err(Trap::BadFunction(5)))"),
        INTERP_HELPERS + """
        #[test]
        fn malformed_code_is_a_trap_not_a_panic() {
            let cases: Vec<(&str, Func)> = vec![
                ("stack underflow", func(0, 1, 0, &[0x6A, 0x0B])),
                ("unknown opcode", func(0, 0, 0, &[0xFF, 0x0B])),
                ("no final end", func(0, 1, 0, &[0x41, 0x01])),
                ("bytes after the final end", func(0, 0, 0, &[0x0B, 0x01])),
                ("block never closed", func(0, 0, 0, &[0x02, 0x40, 0x0B])),
                ("else outside an if", func(0, 0, 0, &[0x02, 0x40, 0x05, 0x0B, 0x0B])),
                ("bad local index", func(0, 0, 0, &[0x20, 0x00, 0x1A, 0x0B])),
                ("bad label depth", func(0, 0, 0, &[0x0C, 0x05, 0x0B])),
                ("truncated immediate", func(0, 0, 0, &[0x41, 0x80])),
                ("bad block type", func(0, 0, 0, &[0x02, 0x7E, 0x0B, 0x0B])),
            ];
            for (what, f) in cases {
                check!(what, run(&[f], 0, &[], 1000), Err(Trap::Malformed));
            }
            check!("wrong argument count", run(&[func(2, 0, 0, &[0x0B])], 0, &[1], 1000), Err(Trap::Malformed));
        }

        #[test]
        fn random_straight_line_arithmetic_matches_a_model() {
            let mut rng = anneal_prelude::Rng::new(4242);
            for round in 0..400 {
                let mut code: Vec<u8> = Vec::new();
                let mut model: Vec<i32> = Vec::new();
                for _ in 0..40 {
                    if model.len() < 2 || rng.below(3) == 0 {
                        let v = rng.below(4_000_001) as i64 - 2_000_000;
                        code.push(0x41);
                        sleb(v, &mut code);
                        model.push(v as i32);
                    } else {
                        let b = model.pop().unwrap();
                        let a = model.pop().unwrap();
                        let (op, r) = match rng.below(6) {
                            0 => (0x6A, a.wrapping_add(b)),
                            1 => (0x6B, a.wrapping_sub(b)),
                            2 => (0x6C, a.wrapping_mul(b)),
                            3 => (0x71, a & b),
                            4 => (0x72, a | b),
                            _ => (0x73, a ^ b),
                        };
                        code.push(op);
                        model.push(r);
                    }
                }
                code.push(0x0B);
                let got = run(&[func(0, model.len(), 0, &code)], 0, &[], 100_000);
                check!(format!("round {round}"), got, Ok(model));
            }
        }

        #[test]
        fn random_loops_match_a_model() {
            // sum of i*i for i in 1..=n, with a loop whose body is built from the same pieces every time
            let prog: Vec<u8> = vec![
                0x02, 0x40, 0x03, 0x40, 0x20, 0x00, 0x45, 0x0D, 0x01, // block loop: if n == 0 break
                0x20, 0x01, 0x20, 0x00, 0x20, 0x00, 0x6C, 0x6A, 0x21, 0x01, // acc += n * n
                0x20, 0x00, 0x41, 0x01, 0x6B, 0x21, 0x00, 0x0C, 0x00, 0x0B, 0x0B, // n -= 1; repeat
                0x20, 0x01, 0x0B,
            ];
            let mut rng = anneal_prelude::Rng::new(9);
            for _ in 0..100 {
                let n = rng.below(300) as i32;
                let want: i32 = (1..=n).fold(0i32, |a, i| a.wrapping_add(i.wrapping_mul(i)));
                check!(format!("n = {n}"), run(&[func(1, 1, 1, &prog)], 0, &[n], 1_000_000), Ok(vec![want]));
            }
        }
        """,
    ],
    wrong=dict(
        br_keeps_the_extra_stack=sub(INTERP, "    stack.truncate(height);\n    stack.extend(keep);\n", "    stack.extend(keep);\n"),
        loop_branch_exits_the_loop=sub(INTERP, "Label { is_loop: true, cont: next, height: stack.len(), arity: 0 }", "Label { is_loop: false, cont: end + 1, height: stack.len(), arity: 0 }"),
        min_div_minus_one_wraps=sub(INTERP, "            if a == i32::MIN && b == -1 {\n                return Err(Trap::IntegerOverflow);\n            }\n            a / b", "            a.wrapping_div(b)"),
        shift_count_not_masked=sub(INTERP, "0x74 => a.wrapping_shl(b as u32),", "0x74 => a.checked_shl(b as u32).unwrap_or(0),"),
        lt_u_is_signed=sub(INTERP, "0x49 => ((a as u32) < (b as u32)) as i32,", "0x49 => (a < b) as i32,"),
        fuel_check_one_too_early=sub(INTERP, "        if *fuel == 0 {\n            return Err(Trap::OutOfFuel);", "        if *fuel <= 1 {\n            return Err(Trap::OutOfFuel);"),
        select_picks_the_wrong_side=sub(INTERP, "stack.push(if c != 0 { a } else { b });", "stack.push(if c != 0 { b } else { a });"),
        depth_limit_off_by_one=sub(INTERP, "    if depth > 100 {", "    if depth > 101 {"),
        tee_pops_the_value=sub(INTERP, "let v = *stack.last().ok_or(Trap::Malformed)?;", "let v = pop(&mut stack)?;"),
        else_falls_through=sub(INTERP, "            0x05 => {\n                next = jumps[&pc].0; // the then-branch is done: run the `end`\n            }", "            0x05 => {}"),
        const_is_not_sign_extended=sub(INTERP, "        if b & 0x80 == 0 {\n            if b & 0x40 != 0 {\n                result |= -1i64 << shift;\n            }\n            return Ok(result as i32);", "        if b & 0x80 == 0 {\n            return Ok(result as i32);"),
        rem_min_minus_one_traps=sub(INTERP, "            a.wrapping_rem(b)", "            if a == i32::MIN && b == -1 {\n                return Err(Trap::IntegerOverflow);\n            }\n            a % b"),
    ),
    hints=[("approach", "Scan each function once to find, for every `block`/`loop`/`if`, where its `end` (and `else`) is: keep a stack of open blocks. Then run with an operand stack, locals, and a **label stack**. A label records where `br` goes (after the `end` for a block, back to the loop body for a loop), the stack height at entry and how many values it yields."),
           ("rust", "`branch(depth)`: pick `labels[len - 1 - depth]`, `split_off` its arity values from the stack, `truncate` to its height, push them back, then `labels.truncate(...)` (keep the label for a loop, drop it for a block) and set the pc. The function itself is label 0 with target `code.len()`, so `return` is `br` to the outermost label. Skip over immediates with the provided LEB readers when scanning, or you will misread a `0x0B` inside a constant as an `end`."),
           ("edge", "Decrement fuel *before* executing each instruction, and test for zero first. `if` with a false condition: jump to just after the `else` if there is one, else to the `end`; when a then-branch reaches `else`, jump to the matching `end`. `wrapping_shl`/`wrapping_shr` already take the count mod 32; `i32::MIN / -1` must be checked before dividing, because Rust panics on it.")],
    notes=("This is `wasm3`/`wasmi`-style interpretation: the structured control flow (no arbitrary jumps) is exactly what makes WebAssembly fast to validate and compile, and why a side table of block ends lets even a naive interpreter run it. Real engines also validate types up front, so the `Malformed` cases here become load-time errors and the hot loop never checks the stack. Fuel and depth limits are how a host sandbox bounds untrusted code (Wasmtime exposes both).", "O(instructions executed) after an O(code) scan", "O(stack + locals + labels), call depth capped at 100"),
    follow_up="1) Precompute the jump table into the instruction stream (a decoded `Vec<Op>` with resolved targets) and measure the speedup over re-reading bytes. 2) Add a type-checking validator pass (the abstract stack of types from the spec appendix) so ill-typed code is rejected before running; then delete the runtime underflow checks. 3) Add linear memory with `i32.load`/`i32.store` and bounds checks that trap: what is the cost of the check, and how do real engines avoid it (guard pages)? 4) Add `i64` and a value enum, or `br_table`. 5) Connect it to the decoder: run an exported function of a module decoded by the previous problem.",
    related=["Y6", "C5", "Y2"],
))

STAGES = [("read-it", "Read it", "medium"), ("run-it", "Run it", "hard")]

if __name__ == "__main__":
    n = write_track("y6-webassembly", "Y6", "WebAssembly from scratch", "Y", "sde3", 6,
                    "WebAssembly without an engine: decode the binary format with bounded reads, then run its structured control flow on a stack machine.",
                    STAGES, P)
    print("Y6", n)
