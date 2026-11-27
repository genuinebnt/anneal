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
fn checksum_of_a_word() {
    check!("Wikipedia", adler32(b"Wikipedia"), 0x11E6_0398);
    check!("nothing", adler32(b""), 1);
}

#[test]
fn two_letters() {
    let want = vec![
        0x78, 0x01, 1, 2, 0, 0xFD, 0xFF, b'h', b'i', 0x01, 0x3B, 0x00, 0xD2,
    ];
    check!("hi", wrap(b"hi"), want.clone());
    check!("and back", unwrap(&want), Some(b"hi".to_vec()));
}

#[test]
fn nothing_at_all() {
    check!(
        "still one block",
        wrap(b""),
        vec![0x78, 0x01, 1, 0, 0, 0xFF, 0xFF, 0, 0, 0, 1]
    );
    check!("and back", unwrap(&wrap(b"")), Some(vec![]));
}

#[test]
fn a_damaged_byte_is_noticed() {
    let mut stream = wrap(b"hello, world");
    stream[9] ^= 1;
    check!("checksum", unwrap(&stream), None);
}

#[test]
fn several_blocks_from_elsewhere() {
    let stream = build(b"abcdef", &[2, 0, 4]);
    check!("joined", unwrap(&stream), Some(b"abcdef".to_vec()));
}
