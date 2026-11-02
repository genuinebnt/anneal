use solution::*;

#[test]
fn empty_frame() {
    check!(r#"len 0"#, { let mut d = Decoder::new(8); d.feed(&[0, 0, 0, 0, 0, 0, 0, 1, 5]); (d.next_frame(), d.next_frame(), d.next_frame()) }, (Ok(Some(vec![])), Ok(Some(vec![5])), Ok(None)));
}

#[test]
fn exactly_the_limit_is_fine() {
    check!(r#"max 4, len 4"#, { let mut d = Decoder::new(4); d.feed(&[0, 0, 0, 4, 1, 2, 3, 4]); d.next_frame() }, Ok(Some(vec![1, 2, 3, 4])));
}

#[test]
fn one_over_the_limit() {
    check!(r#"max 4, len 5"#, { let mut d = Decoder::new(4); d.feed(&[0, 0, 0, 5]); d.next_frame() }, Err(FrameError::TooLarge { len: 5, max: 4 }));
}

#[test]
fn four_gigabyte_header_costs_nothing() {
    check!(r#"len = u32::MAX"#, { let mut d = Decoder::new(1 << 20); d.feed(&[0xFF, 0xFF, 0xFF, 0xFF]); (d.next_frame(), d.buffered()) }, (Err(FrameError::TooLarge { len: u32::MAX as usize, max: 1 << 20 }), 4));
}

#[test]
fn error_is_sticky() {
    check!(r#"the stream cannot recover"#, { let mut d = Decoder::new(2); d.feed(&[0, 0, 0, 9, 1, 2, 3]); let a = d.next_frame(); let b = d.next_frame(); (a == b, a.is_err()) }, (true, true));
}

#[test]
fn buffered_counts_the_leftover() {
    check!(r#"partial frame"#, { let mut d = Decoder::new(64); d.feed(&[0, 0, 0, 5, 1, 2]); let a = d.buffered(); d.next_frame().unwrap(); (a, d.buffered()) }, (6, 6));
}

#[test]
fn buffered_is_zero_after_whole_frames() {
    check!(r#"everything consumed"#, { let mut d = Decoder::new(64); d.feed(&[0, 0, 0, 1, 1, 0, 0, 0, 2, 2, 2]); d.next_frame().unwrap(); d.next_frame().unwrap(); d.buffered() }, 0);
}

#[test]
fn big_endian_on_the_wire() {
    check!(r#"len 258 = 0x0102"#, { let mut out = Vec::new(); encode_frame(&vec![0u8; 258], &mut out); out[..4].to_vec() }, vec![0, 0, 1, 2]);
}

#[test]
fn one_byte_at_a_time() {
    let mut stream = Vec::new();
    let payloads: Vec<Vec<u8>> = vec![b"hello".to_vec(), vec![], vec![7; 300], b"x".to_vec()];
    for p in &payloads {
        encode_frame(p, &mut stream);
    }
    let mut d = Decoder::new(1000);
    let mut got = Vec::new();
    for b in stream {
        d.feed(&[b]);
        while let Some(f) = d.next_frame().unwrap() {
            got.push(f);
        }
    }
    check!("every frame came out, in order, exactly once", got, payloads);
    check!("nothing left over", d.buffered(), 0);
}

#[test]
fn random_chunking_round_trip() {
    let mut rng = anneal_prelude::Rng::new(6801);
    for _ in 0..200 {
        let payloads: Vec<Vec<u8>> = (0..rng.below(20)).map(|_| (0..rng.below(50)).map(|_| rng.below(256) as u8).collect()).collect();
        let mut stream = Vec::new();
        for p in &payloads {
            encode_frame(p, &mut stream);
        }
        let mut d = Decoder::new(64);
        let mut got = Vec::new();
        let mut i = 0;
        while i < stream.len() {
            let n = 1 + rng.below(17);
            let end = (i + n).min(stream.len());
            d.feed(&stream[i..end]);
            i = end;
            while let Some(f) = d.next_frame().unwrap() {
                got.push(f);
            }
        }
        check!("round trip through random read sizes", got, payloads);
    }
}
