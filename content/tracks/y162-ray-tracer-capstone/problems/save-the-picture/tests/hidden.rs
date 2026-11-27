#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;

/// CRC-32, bit by bit.
#[allow(dead_code)]
fn crc(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &byte in data {
        c ^= u32::from(byte);
        for _ in 0..8 {
            c = if c & 1 == 1 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

#[allow(dead_code)]
fn adler(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u64, 0u64);
    for &byte in data {
        a = (a + u64::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b * 65536 + a) as u32
}

#[allow(dead_code)]
fn be(b: &[u8]) -> u32 {
    u32::from_be_bytes(b.try_into().unwrap())
}

/// A strict reader for the kind of PNG the statement asks for. Returns width, height and the colour bytes.
#[allow(dead_code)]
fn decode(png: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    fn need(ok: bool, what: &str) -> Result<(), String> {
        if ok {
            Ok(())
        } else {
            Err(what.to_string())
        }
    }
    need(
        png.len() >= 8 && png[..8] == [137, 80, 78, 71, 13, 10, 26, 10],
        "signature",
    )?;
    let mut at = 8;
    let mut kinds = Vec::new();
    let (mut size, mut zlib) = (None, Vec::new());
    while at < png.len() {
        need(at + 12 <= png.len(), "a chunk is cut short")?;
        let len = be(&png[at..at + 4]) as usize;
        need(len <= png.len() - at - 12, "a chunk length is too large")?;
        let kind = png[at + 4..at + 8].to_vec();
        let data = &png[at + 8..at + 8 + len];
        need(
            be(&png[at + 8 + len..at + 12 + len]) == crc(&png[at + 4..at + 8 + len]),
            "chunk checksum",
        )?;
        match kind.as_slice() {
            b"IHDR" => {
                need(
                    kinds.is_empty() && len == 13,
                    "IHDR comes first and has 13 bytes",
                )?;
                need(
                    data[8..] == [8, 2, 0, 0, 0],
                    "depth 8, colour type 2, no other options",
                )?;
                size = Some((be(&data[0..4]) as usize, be(&data[4..8]) as usize));
            }
            b"IDAT" => {
                need(
                    kinds
                        .last()
                        .is_some_and(|k: &Vec<u8>| k == b"IHDR" || k == b"IDAT"),
                    "IDAT chunks follow each other after IHDR",
                )?;
                zlib.extend_from_slice(data);
            }
            b"IEND" => need(len == 0 && at + 12 == png.len(), "IEND is empty and last")?,
            _ => return Err("only IHDR, IDAT and IEND".to_string()),
        }
        kinds.push(kind);
        at += 12 + len;
    }
    need(
        kinds.last().is_some_and(|k| k == b"IEND"),
        "IEND at the end",
    )?;
    let (width, height) = size.ok_or("no IHDR")?;
    need(width > 0 && height > 0, "sides above zero")?;
    need(zlib.len() >= 6, "zlib stream too short")?;
    need(
        zlib[0] & 15 == 8
            && zlib[0] >> 4 <= 7
            && (u32::from(zlib[0]) * 256 + u32::from(zlib[1])) % 31 == 0
            && zlib[1] & 32 == 0,
        "zlib header",
    )?;
    let (mut at, mut raw) = (2, Vec::new());
    loop {
        need(at + 5 <= zlib.len() - 4, "a block is cut short")?;
        let head = zlib[at];
        need(head <= 1, "stored blocks only")?;
        let len = u16::from_le_bytes([zlib[at + 1], zlib[at + 2]]);
        let check = u16::from_le_bytes([zlib[at + 3], zlib[at + 4]]);
        need(check == !len, "block length check")?;
        at += 5;
        need(
            at + len as usize <= zlib.len() - 4,
            "block data is cut short",
        )?;
        raw.extend_from_slice(&zlib[at..at + len as usize]);
        at += len as usize;
        if head == 1 {
            break;
        }
    }
    need(at == zlib.len() - 4, "bytes after the last block")?;
    need(be(&zlib[at..]) == adler(&raw), "zlib checksum")?;
    need(
        raw.len() == (width * 3 + 1) * height,
        "one filter byte and width * 3 bytes per line",
    )?;
    let mut rgb = Vec::new();
    for line in raw.chunks(width * 3 + 1) {
        need(line[0] == 0, "filter type 0 on every line")?;
        rgb.extend_from_slice(&line[1..]);
    }
    Ok((width, height, rgb))
}

/// The byte the statement asks for, before rounding.
#[allow(dead_code)]
fn exact(linear: f64) -> f64 {
    let c = linear.clamp(0.0, 1.0);
    255.0
        * if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
}

#[allow(dead_code)]
fn round_trip(width: usize, height: usize, rng: &mut Rng) {
    let rgb: Vec<u8> = rng.vec(width * height * 3, 0, 255);
    let png = encode_png(width, height, &rgb);
    check!(
        format!("{width} x {height}: something comes back"),
        png.is_some(),
        true
    );
    let got = decode(&png.unwrap());
    check!(
        format!("{width} x {height}: readable"),
        got.as_ref().err(),
        None
    );
    let (w, h, pixels) = got.unwrap();
    check!(format!("{width} x {height}: size"), (w, h), (width, height));
    check!(format!("{width} x {height}: pixels"), pixels == rgb, true);
}

#[test]
fn every_byte_is_the_nearest() {
    let mut rng = Rng::new(1641);
    for case in 0..20_000 {
        let linear = if case % 2 == 0 {
            rng.int(0, 1_000_000) as f64 / 1e6
        } else {
            rng.int(0, 10_000) as f64 / 1e6
        };
        let want = exact(linear);
        let got = f64::from(srgb_byte(linear));
        check!(
            format!("linear {linear}: got {got}, the curve gives {want}"),
            (got - want).abs() <= 0.5 + 1e-9,
            true
        );
    }
}

#[test]
fn small_pictures_of_every_shape() {
    let mut rng = Rng::new(1642);
    for width in 1..=9 {
        for height in 1..=9 {
            round_trip(width, height, &mut rng);
        }
    }
}

#[test]
fn a_picture_larger_than_one_block() {
    let mut rng = Rng::new(1643);
    round_trip(200, 200, &mut rng);
}

#[test]
fn data_that_exactly_fills_blocks() {
    let mut rng = Rng::new(1644);
    // One line of 21844 pixels is 65533 bytes of data. 21845 pixels is 65536: one more than a block.
    for width in [21844, 21845] {
        round_trip(width, 1, &mut rng);
    }
    // 3 * 1456 + 1 = 4369, and 4369 * 15 = 65535: exactly one full block. Times 30: exactly two.
    round_trip(1456, 15, &mut rng);
    round_trip(1456, 30, &mut rng);
}

#[test]
fn bright_data_does_not_overflow_a_checksum() {
    let rgb = vec![255u8; 150 * 150 * 3];
    let png = encode_png(150, 150, &rgb).unwrap();
    check!("decodes", decode(&png) == Ok((150, 150, rgb)), true);
}

#[test]
fn tall_and_wide() {
    let mut rng = Rng::new(1645);
    round_trip(1, 5000, &mut rng);
    round_trip(5000, 1, &mut rng);
}

#[test]
fn sizes_that_overflow() {
    for (w, h) in [
        (usize::MAX, 1),
        (1, usize::MAX),
        (usize::MAX, usize::MAX),
        (1 << 31, 1),
        (1 << 62, 4),
        (usize::MAX / 3 + 1, 1),
    ] {
        check!(
            format!("{w} x {h} with no bytes"),
            encode_png(w, h, &[]),
            None
        );
        check!(
            format!("{w} x {h} with three bytes"),
            encode_png(w, h, &[1, 2, 3]),
            None
        );
    }
}

#[test]
fn many_pixels() {
    let mut rng = Rng::new(1646);
    let pixels: Vec<Vec3> = (0..500)
        .map(|_| {
            v3(
                rng.int(-20, 120) as f64 / 100.0,
                rng.int(0, 100) as f64 / 100.0,
                rng.int(0, 100) as f64 / 100.0,
            )
        })
        .collect();
    let bytes = to_rgb8(&pixels);
    check!("three bytes each", bytes.len(), 1500);
    let same = pixels
        .iter()
        .enumerate()
        .all(|(i, p)| bytes[3 * i..3 * i + 3] == [srgb_byte(p.x), srgb_byte(p.y), srgb_byte(p.z)]);
    check!("the same as one by one", same, true);
}
