use solution::*;

#[test]
fn sequence_numbers_wrap() {
    check!(r#"isn near u32::MAX"#, { let isn = u32::MAX - 2; let mut r = Reassembler::new(isn, 100); r.segment(isn, b"abc", false); r.segment(isn.wrapping_add(3), b"def", false); r.segment(isn.wrapping_add(6), b"g", true); (r.read(), r.is_finished()) }, (b"abcdefg".to_vec(), true));
}

#[test]
fn wrapped_segment_arrives_before_the_gap_fills() {
    check!(r#"later segment past the wrap, earlier one late"#, { let isn = u32::MAX - 1; let mut r = Reassembler::new(isn, 100); r.segment(isn.wrapping_add(4), b"ef", false); r.segment(isn, b"abcd", false); r.read() }, b"abcdef".to_vec());
}

#[test]
fn first_arrival_wins_on_conflict() {
    check!(r#"overlapping segments disagree"#, { let mut r = Reassembler::new(0, 100); r.segment(2, b"XXXX", false); r.segment(0, b"abcdef", false); r.read() }, b"abXXXX".to_vec());
}

#[test]
fn old_data_is_ignored() {
    check!(r#"a stale retransmit after delivery"#, { let mut r = Reassembler::new(0, 100); r.segment(0, b"abcdef", false); r.read(); r.segment(1, b"bcd", false); (r.read(), r.buffered()) }, (vec![], 0));
}

#[test]
fn bytes_beyond_the_window_are_dropped() {
    check!(r#"window 4"#, { let mut r = Reassembler::new(0, 4); r.segment(2, b"cdefgh", false); let held = r.buffered(); r.segment(0, b"ab", false); (held, r.read(), r.buffered()) }, (2, b"abcd".to_vec(), 0));
}

#[test]
fn window_slides_as_bytes_are_delivered() {
    check!(r#"window 4 again"#, { let mut r = Reassembler::new(0, 4); r.segment(0, b"abcd", false); r.segment(4, b"efgh", false); (r.read(), r.buffered()) }, (b"abcdefgh".to_vec(), 0));
}

#[test]
fn fin_beyond_the_window_is_ignored() {
    check!(r#"the sender will retransmit it"#, { let mut r = Reassembler::new(0, 3); r.segment(0, b"abcdef", true); let a = (r.read(), r.is_finished()); r.segment(3, b"def", true); (a, r.read(), r.is_finished()) }, ((b"abc".to_vec(), false), b"def".to_vec(), true));
}

#[test]
fn fin_with_no_data() {
    check!(r#"an empty final segment"#, { let mut r = Reassembler::new(5, 10); r.segment(5, b"ab", false); r.segment(7, b"", true); (r.read(), r.is_finished()) }, (b"ab".to_vec(), true));
}

#[test]
fn empty_stream() {
    check!(r#"fin at offset 0"#, { let mut r = Reassembler::new(9, 10); r.segment(9, b"", true); (r.read(), r.is_finished()) }, (vec![], true));
}

#[test]
fn random_delivery_reconstructs_the_stream() {
    let mut rng = anneal_prelude::Rng::new(6802);
    for round in 0..60 {
        let n = 1 + rng.below(20_000);
        let stream: Vec<u8> = (0..n).map(|_| rng.below(256) as u8).collect();
        let isn = if round % 3 == 0 { u32::MAX - rng.below(5000) as u32 } else { rng.below(1 << 30) as u32 };
        // cut the stream into consecutive pieces, then add overlapping and duplicate copies
        let mut segs: Vec<(usize, usize)> = Vec::new();
        let mut i = 0;
        while i < n {
            let len = (1 + rng.below(700)).min(n - i);
            segs.push((i, i + len));
            i += len;
        }
        for _ in 0..segs.len() {
            let s = rng.below(n);
            let e = (s + 1 + rng.below(900)).min(n);
            segs.push((s, e));
        }
        for k in (1..segs.len()).rev() {
            segs.swap(k, rng.below(k + 1));
        }
        let mut r = Reassembler::new(isn, n + 10);
        let mut got = Vec::new();
        for (k, &(s, e)) in segs.iter().enumerate() {
            let fin = e == n && rng.below(2) == 0;
            r.segment(isn.wrapping_add(s as u32), &stream[s..e], fin);
            got.extend(r.read());
            if k % 97 == 0 {
                assert!(!r.is_finished() || got.len() == n, "finished with bytes missing");
            }
        }
        r.segment(isn.wrapping_add(n as u32), b"", true);
        got.extend(r.read());
        check!("the stream was rebuilt exactly", got == stream, true);
        check!("finished, nothing buffered", (r.is_finished(), r.buffered()), (true, 0));
    }
}

#[test]
fn a_small_window_still_delivers_when_the_sender_retransmits() {
    let mut rng = anneal_prelude::Rng::new(6803);
    let n = 5000;
    let stream: Vec<u8> = (0..n).map(|_| rng.below(256) as u8).collect();
    let window = 300;
    let mut r = Reassembler::new(77, window);
    let mut got = Vec::new();
    let mut guard = 0;
    while got.len() < n && guard < 100_000 {
        guard += 1;
        // the sender sends a random segment starting somewhere in what it thinks is unacknowledged
        let s = (got.len() + rng.below(2 * window)).min(n - 1);
        let e = (s + 1 + rng.below(120)).min(n);
        r.segment(77u32.wrapping_add(s as u32), &stream[s..e], false);
        got.extend(r.read());
        assert!(r.buffered() <= window, "buffered {} bytes with a window of {}", r.buffered(), window);
    }
    check!("delivered everything in order despite drops", got == stream, true);
}
