use solution::*;

#[test]
fn encode_layout() {
    check!(r#"3 bytes"#, { let mut out = Vec::new(); encode_frame(b"abc", &mut out); out }, vec![0, 0, 0, 3, b'a', b'b', b'c']);
}

#[test]
fn decode_one() {
    check!(r#"one whole frame"#, { let mut d = Decoder::new(1024); d.feed(&[0, 0, 0, 2, 9, 8]); (d.next_frame(), d.next_frame()) }, (Ok(Some(vec![9, 8])), Ok(None)));
}

#[test]
fn partial_then_rest() {
    check!(r#"header split from payload"#, { let mut d = Decoder::new(1024); d.feed(&[0, 0]); let a = d.next_frame(); d.feed(&[0, 3, 1]); let b = d.next_frame(); d.feed(&[2, 3]); (a, b, d.next_frame()) }, (Ok(None), Ok(None), Ok(Some(vec![1, 2, 3]))));
}

#[test]
fn two_frames_in_one_chunk() {
    check!(r#"pipelined"#, { let mut d = Decoder::new(1024); d.feed(&[0, 0, 0, 1, 7, 0, 0, 0, 1, 8]); (d.next_frame(), d.next_frame(), d.next_frame()) }, (Ok(Some(vec![7])), Ok(Some(vec![8])), Ok(None)));
}

#[test]
fn oversized_is_rejected_early() {
    check!(r#"header only"#, { let mut d = Decoder::new(16); d.feed(&[0, 0, 1, 0]); d.next_frame() }, Err(FrameError::TooLarge { len: 256, max: 16 }));
}
