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
fn data_may_contain_line_ends_and_zero_lines() {
    let data = b"\r\n0\r\n\r\n;".to_vec();
    waits_then_decodes(&encode(std::slice::from_ref(&data)), 100, &data);
}

#[test]
fn the_empty_body() {
    waits_then_decodes(b"0\r\n\r\n", 0, b"");
}

#[test]
fn hex_in_both_cases_and_leading_zeros() {
    let mut bytes = b"0000000a\r\n0123456789\r\n".to_vec();
    bytes.extend_from_slice(b"B\r\nabcdefghijk\r\n000\r\n\r\n");
    waits_then_decodes(&bytes, 100, b"0123456789abcdefghijk");
}

#[test]
fn sizes_that_are_malformed() {
    for size in ["", " 5", "5 ", "+5", "-1", "0x5", "000000005", ";x", "5g"] {
        let bytes = format!("{size}\r\nhello\r\n0\r\n\r\n").into_bytes();
        check!(show(&bytes), decode(&bytes, 100), Err(ChunkError::Malformed));
    }
}

#[test]
fn a_wrong_byte_after_the_data_is_malformed() {
    for bytes in [&b"2\r\nabcd"[..], b"2\r\nab\n\n0\r\n\r\n", b"2\r\nab\rX", b"1\r\nab\r\n"] {
        check!(show(bytes), decode(bytes, 100), Err(ChunkError::Malformed));
    }
}

#[test]
fn the_body_limit_is_exact_and_counts_all_chunks() {
    let two = encode(&[vec![1; 6], vec![2; 4]]);
    waits_then_decodes(&two, 10, &[1, 1, 1, 1, 1, 1, 2, 2, 2, 2]);
    check!("limit 9, second chunk announced", decode(b"6\r\nxxxxxx\r\n4\r\n", 9), Err(ChunkError::TooLarge));
    check!("one past, at once", decode(b"B\r\n", 10), Err(ChunkError::TooLarge));
    check!("all eight digits", decode(b"FFFFFFFF\r\n", 10), Err(ChunkError::TooLarge));
    check!("after some data", decode(b"1\r\na\r\nFFFFFFFF\r\n", usize::MAX), Ok(None));
}

#[test]
fn a_long_size_line_is_refused_complete_or_not() {
    let long = format!("1;{}", "e".repeat(63));
    assert_eq!(long.len(), 65);
    check!("65 bytes, complete", decode(format!("{long}\r\nz\r\n0\r\n\r\n").as_bytes(), 10), Err(ChunkError::Malformed));
    check!("65 bytes, no end", decode(long.as_bytes(), 10), Err(ChunkError::Malformed));
    let most = format!("1;{}\r\nz\r\n0\r\n\r\n", "e".repeat(62));
    waits_then_decodes(most.as_bytes(), 10, b"z");
}

#[test]
fn a_trailer_section_of_exactly_the_limit() {
    // one trailer line and the empty line: 256 bytes together
    let bytes = format!("0\r\nT: {}\r\n\r\n", "x".repeat(MAX_TRAILER - 7));
    assert_eq!(bytes.len() - 3, MAX_TRAILER);
    waits_then_decodes(bytes.as_bytes(), 10, b"");
    let over = format!("0\r\nT: {}\r\n\r\n", "x".repeat(MAX_TRAILER - 6));
    check!("one byte more", decode(over.as_bytes(), 10), Err(ChunkError::TrailerTooLong));
}

#[test]
fn many_short_trailers_are_refused_without_the_end() {
    let mut bytes = b"1\r\na\r\n0\r\n".to_vec();
    for _ in 0..100 {
        bytes.extend_from_slice(b"T: 1\r\n");
    }
    check!("100 trailer lines, no empty line", decode(&bytes, 10), Err(ChunkError::TrailerTooLong));
}

#[test]
fn one_endless_trailer_is_refused() {
    let mut bytes = b"0\r\n".to_vec();
    bytes.extend_from_slice(&[b'a'; 400]);
    check!("400 bytes and no line end", decode(&bytes, 10), Err(ChunkError::TrailerTooLong));
}

#[test]
fn what_follows_is_left_alone() {
    let bytes = b"1\r\na\r\n0\r\n\r\n1\r\nb\r\n0\r\n\r\n";
    check!("two encodings", decode(bytes, 10), Ok(Some((b"a".to_vec(), 11))));
}

#[test]
fn random_bodies_cut_anywhere() {
    let mut rng = anneal_prelude::Rng::new(70801);
    for _ in 0..150 {
        let mut chunks = Vec::new();
        for _ in 0..rng.below(4) {
            let len = 1 + rng.below(20);
            let c: Vec<u8> = if rng.bool() { rng.vec(len, 0, 255) } else { rng.string(len, "\r\n0;a").into_bytes() };
            chunks.push(c);
        }
        let want = chunks.concat();
        waits_then_decodes(&encode(&chunks), want.len() + rng.below(2), &want);
        if !want.is_empty() {
            let bytes = encode(&chunks);
            check!(format!("{} with a limit one short", show(&bytes)), decode(&bytes, want.len() - 1), Err(ChunkError::TooLarge));
        }
    }
}

#[test]
fn random_junk_never_changes_its_mind() {
    let mut rng = anneal_prelude::Rng::new(70802);
    let pieces = ["0", "1", "5", "f", ";", "\r\n", "\r", "\n", "abc", "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", " "];
    for _ in 0..500 {
        let mut bytes = Vec::new();
        for _ in 0..rng.below(30) {
            bytes.extend_from_slice(rng.pick(&pieces).as_bytes());
        }
        let mut refused = None;
        for cut in 0..=bytes.len() {
            let got = decode(&bytes[..cut], 40);
            if let Some(e) = refused {
                check!(format!("{} was refused earlier; now {cut} bytes", show(&bytes)), got, Err(e));
            } else if let Err(e) = got {
                refused = Some(e);
            } else if matches!(got, Ok(Some(_))) {
                break;
            }
        }
    }
}
