use solution::*;

#[allow(dead_code, unused_imports)]
fn show(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(&bytes[..bytes.len().min(200)]))
}

#[allow(dead_code, unused_imports)]
fn encode(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for c in chunks {
        out.extend_from_slice(format!("{:X}\r\n", c.len()).as_bytes());
        out.extend_from_slice(c);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

#[allow(dead_code, unused_imports)]
fn waits_then_decodes(bytes: &[u8], max: usize, want: &[u8]) {
    for cut in 0..bytes.len() {
        check!(format!("first {cut} bytes of {}", show(bytes)), decode(&bytes[..cut], max), Ok(None));
    }
    check!(show(bytes), decode(bytes, max), Ok(Some((want.to_vec(), bytes.len()))));
}

#[test]
fn two_chunks() {
    let bytes = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
    check!(show(bytes), decode(bytes, 100), Ok(Some((b"hello world".to_vec(), bytes.len()))));
}

#[test]
fn extensions_and_trailers_are_ignored() {
    let bytes = b"3;a=b\r\nabc\r\n0;last\r\nT: 1\r\n\r\nNEXT";
    check!(show(bytes), decode(bytes, 100), Ok(Some((b"abc".to_vec(), bytes.len() - 4))));
}

#[test]
fn unfinished_input_waits() {
    check!("half a chunk", decode(b"5\r\nhel", 100), Ok(None));
}

#[test]
fn a_huge_chunk_is_refused_before_its_data() {
    check!("FFFFFF with a limit of 100", decode(b"FFFFFF\r\n", 100), Err(ChunkError::TooLarge));
}

#[test]
fn a_size_that_is_not_hex_is_malformed() {
    check!("size g", decode(b"g\r\n", 100), Err(ChunkError::Malformed));
}

#[test]
fn data_longer_than_announced_is_malformed() {
    check!("3 announced, 5 sent", decode(b"3\r\nhello\r\n0\r\n\r\n", 100), Err(ChunkError::Malformed));
}
