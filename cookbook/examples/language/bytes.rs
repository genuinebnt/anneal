//! RECIPE 42 (language): bytes: `&[u8]`, endianness, bit tricks, hex, UTF-8 validation, and a varint codec.
//! LEVEL: intermediate
//! Use in: file/wire formats (logs, WAL, indexes), checksums, compact ids, binary protocols.   Run: cargo run --example bytes
//! COMMON USES: binary file formats, network protocols, checksums, index keys, compact ids (varints in protobuf/Kafka/SQLite).
//!
//! `&[u8]` / `Vec<u8>` are the raw currency of IO. Convert to text only when needed (`from_utf8` validates).
//! Integers have `to_le_bytes/from_le_bytes` (little endian), `_be_` (big endian, network order), `_ne_` (native).
//! For high-performance byte scanning see BurntSushi's memchr https://github.com/BurntSushi/memchr
//! and the `bytes` crate https://docs.rs/bytes for cheap shared buffers.
#![allow(dead_code)]

/// LEB128 varint: 7 bits per byte, high bit = "more bytes follow". Small numbers take 1 byte.
/// (Used in protobuf and for compressing posting lists in search indexes.)
fn encode_varint(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}
fn decode_varint(buf: &[u8]) -> Result<(u64, usize), &'static str> {
    let mut value = 0u64;
    for (i, &b) in buf.iter().enumerate() {
        if i >= 10 {
            return Err("varint too long");
        }
        value |= ((b & 0x7f) as u64) << (7 * i);
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err("truncated varint")
}

fn main() {
    // endianness
    let n: u32 = 0x0102_0304;
    println!("le={:?} be={:?}", n.to_le_bytes(), n.to_be_bytes());
    let buf = [0x04, 0x03, 0x02, 0x01, 0xff];
    let back = u32::from_le_bytes(buf[..4].try_into().unwrap()); // slice -> array via try_into
    println!("{back:#x}");

    // slicing helpers
    let data = b"HEAD\x00\x00\x00\x05hello-tail";
    let (head, rest) = data.split_at(4);
    let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
    let body = &rest[4..4 + len];
    println!(
        "{:?} {len} {:?}",
        std::str::from_utf8(head),
        std::str::from_utf8(body)
    );
    println!("{:?}", data.chunks(5).map(|c| c.len()).collect::<Vec<_>>());
    println!(
        "{:?}",
        [1u8, 2, 3, 4, 5, 6]
            .chunks_exact(4)
            .map(|c| c.iter().map(|&x| x as u32).sum::<u32>())
            .collect::<Vec<_>>()
    );
    println!(
        "{:?} {}",
        data.iter().position(|&b| b == 0),
        data.starts_with(b"HEAD")
    );
    println!("{:?}", data.windows(2).filter(|w| w == b"he").count());

    // UTF-8 validation
    let bad = [0x68, 0x69, 0xff];
    println!("{:?}", String::from_utf8(bad.to_vec()).is_err());
    println!("{}", String::from_utf8_lossy(&bad)); // invalid bytes become U+FFFD

    // hex formatting and bit tricks
    let hex: String = [0xde_u8, 0xad, 0xbe, 0xef]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("{hex}");
    let mut flags = 0u8;
    flags |= 1 << 3; // set bit 3
    flags |= 1 << 0;
    flags &= !(1 << 0); // clear bit 0
    println!(
        "{flags:08b} bit3={} ones={} lz={} tz={}",
        flags & (1 << 3) != 0,
        flags.count_ones(),
        flags.leading_zeros(),
        flags.trailing_zeros()
    );
    println!(
        "{} {}",
        255u8.checked_add(1).is_none(),
        250u8.saturating_add(10)
    );
    println!("{} {}", 250u8.wrapping_add(10), 7u32.is_power_of_two());

    // varint round trip
    let mut enc = Vec::new();
    for v in [0u64, 1, 127, 128, 300, 1 << 40] {
        encode_varint(v, &mut enc);
    }
    println!("{} bytes for 6 numbers: {enc:?}", enc.len());
    let mut at = 0;
    let mut decoded = vec![];
    while at < enc.len() {
        let (v, used) = decode_varint(&enc[at..]).unwrap();
        decoded.push(v);
        at += used;
    }
    println!("{decoded:?}");
    assert_eq!(decoded, [0, 1, 127, 128, 300, 1 << 40]);
    println!("{:?}", decode_varint(&[0x80, 0x80]));
}
