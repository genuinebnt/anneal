#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

/// Adler-32 with wide numbers, reduced only at the end of each step.
#[allow(dead_code)]
fn reference(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u128, 0u128);
    for &byte in data {
        a += u128::from(byte);
        b += a;
    }
    ((b % 65521) as u32) * 65536 + (a % 65521) as u32
}

/// A stream built from the given block lengths.
#[allow(dead_code)]
fn build(data: &[u8], lengths: &[usize]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut at = 0;
    for (i, &n) in lengths.iter().enumerate() {
        out.push(u8::from(i + 1 == lengths.len()));
        out.extend_from_slice(&[n as u8, (n >> 8) as u8, !(n as u8), !((n >> 8) as u8)]);
        out.extend_from_slice(&data[at..at + n]);
        at += n;
    }
    assert_eq!(at, data.len());
    out.extend_from_slice(&reference(data).to_be_bytes());
    out
}

#[allow(dead_code)]
fn bytes(rng: &mut Rng, n: usize) -> Vec<u8> {
    rng.vec(n, 0, 255)
}

#[allow(dead_code)]
fn shortest(n: usize) -> usize {
    n + 6 + 5 * n.div_ceil(65535).max(1)
}

#[test]
fn around_the_block_size() {
    let mut rng = Rng::new(1630);
    for n in [1, 100, 65534, 65535, 65536, 65537, 131070, 131071, 200_000] {
        let data = bytes(&mut rng, n);
        let stream = wrap(&data);
        check!(
            format!("{n} bytes: as few blocks as possible"),
            stream.len(),
            shortest(n)
        );
        check!(
            format!("{n} bytes: header"),
            &stream[..2],
            &[0x78u8, 0x01][..]
        );
        // Walk the blocks.
        let (mut at, mut got) = (2, Vec::new());
        loop {
            let len = usize::from(stream[at + 1]) + 256 * usize::from(stream[at + 2]);
            let check = usize::from(stream[at + 3]) + 256 * usize::from(stream[at + 4]);
            check!(format!("{n}: complement"), len + check, 65535);
            let last = stream[at];
            got.extend_from_slice(&stream[at + 5..at + 5 + len]);
            at += 5 + len;
            check!(
                format!("{n}: only the final block says last"),
                last,
                u8::from(at == stream.len() - 4)
            );
            if last == 1 {
                break;
            }
        }
        check!(format!("{n}: data"), got == data, true);
        check!(
            format!("{n}: checksum"),
            &stream[at..],
            &reference(&data).to_be_bytes()[..]
        );
        check!(
            format!("{n}: round trip"),
            unwrap(&stream) == Some(data),
            true
        );
    }
}

#[test]
fn long_bright_data() {
    for n in [5000, 70_000, 400_000] {
        let data = vec![255u8; n];
        check!(
            format!("{n} bytes of 255"),
            adler32(&data),
            reference(&data)
        );
    }
}

#[test]
fn every_cut_is_refused() {
    let stream = build(b"some longer piece of text", &[10, 15]);
    for n in 0..stream.len() {
        check!(format!("first {n} bytes"), unwrap(&stream[..n]), None);
    }
}

#[test]
fn extra_bytes_are_refused() {
    let mut stream = wrap(b"abc");
    stream.push(0);
    check!("one more", unwrap(&stream), None);
    // A second stream after the first is also extra.
    let mut two = wrap(b"abc");
    two.extend(wrap(b"abc"));
    check!("two streams", unwrap(&two), None);
}

#[test]
fn headers() {
    let body = &wrap(b"x")[2..];
    let with = |a: u8, b: u8| {
        let mut s = vec![a, b];
        s.extend_from_slice(body);
        unwrap(&s)
    };
    let x = Some(b"x".to_vec());
    check!("the usual", with(0x78, 0x01), x.clone());
    check!("other level hint", with(0x78, 0x9C), x.clone());
    check!("a small window", with(0x08, 0x1D), x);
    check!("check value wrong", with(0x78, 0x02), None);
    check!("another method", with(0x77, 0x09), None);
    check!("a window too large", with(0x88, 0x1C), None);
    check!("wants a dictionary", with(0x78, 0x20), None);
}

#[test]
fn block_headers() {
    let good = wrap(b"abc");
    for (what, at, value) in [
        ("a packed block", 2, 0x03),
        ("a reserved kind", 2, 0x07),
        ("set padding bits", 2, 0x09),
        ("complement low", 5, 0x00),
        ("complement high", 6, 0x00),
        ("length too long", 3, 200),
    ] {
        let mut s = good.clone();
        s[at] = value;
        check!(what, unwrap(&s), None);
    }
    // No block says it is the last.
    let mut s = good.clone();
    s[2] = 0;
    check!("never ends", unwrap(&s), None);
}

#[test]
fn random_noise_never_panics() {
    let mut rng = Rng::new(1631);
    for case in 0..20_000 {
        let n = rng.below(40);
        let mut data = bytes(&mut rng, n);
        if rng.bool() && n >= 2 {
            data[0] = 0x78;
            data[1] = 0x01;
            if n > 2 {
                data[2] = rng.below(2) as u8;
            }
        }
        let got = unwrap(&data);
        if let Some(inner) = got {
            check!(
                format!("case {case}: {data:?}"),
                reference(&inner),
                u32::from_be_bytes(data[data.len() - 4..].try_into().unwrap())
            );
        }
    }
}

#[test]
fn random_splits() {
    let mut rng = Rng::new(1632);
    for case in 0..500 {
        let parts = rng.int(1, 6) as usize;
        let lengths: Vec<usize> = (0..parts).map(|_| rng.below(50)).collect();
        let data = bytes(&mut rng, lengths.iter().sum());
        check!(
            format!("case {case}: {lengths:?}"),
            unwrap(&build(&data, &lengths)) == Some(data),
            true
        );
    }
}
