use solution::*;

#[test]
fn leb_five_byte_limit() {
    check!(r#"u32::MAX fits, one more bit does not"#, (read_u32(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]), read_u32(&[0xFF, 0xFF, 0xFF, 0xFF, 0x1F]), read_u32(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00])), (Ok((u32::MAX, 5)), Err(DecodeError::BadLeb), Err(DecodeError::BadLeb)));
}

#[test]
fn leb_truncated() {
    check!(r#"ends mid-integer"#, (read_u32(&[0x80]), read_u32(&[]), read_u32(&[0x81, 0x80])), (Err(DecodeError::Truncated), Err(DecodeError::Truncated), Err(DecodeError::Truncated)));
}

#[test]
fn leb_small_values() {
    check!(r#"one byte and two bytes"#, (read_u32(&[0x00]), read_u32(&[0x7F]), read_u32(&[0x80, 0x01]), read_u32(&[0xFF, 0x7F])), (Ok((0, 1)), Ok((127, 1)), Ok((128, 2)), Ok((16383, 2))));
}

#[test]
fn truncated_header() {
    check!(r#"too short for the magic or the version"#, (decode(&[]), decode(&[0x00, 0x61]), decode(&[0x00, 0x61, 0x73, 0x6D, 0x01])), (Err(DecodeError::Truncated), Err(DecodeError::Truncated), Err(DecodeError::Truncated)));
}

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
    check!("a four-byte code point", decode(&module(&[&ok])).unwrap().exports[0].name.clone(), "\u{1F980}".to_string());
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
