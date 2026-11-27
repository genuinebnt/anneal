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
fn black_white_and_grey() {
    check!("0", srgb_byte(0.0), 0);
    check!("1", srgb_byte(1.0), 255);
    check!("half the light is not half the byte", srgb_byte(0.5), 188);
    check!("a dark value on the straight part", srgb_byte(0.002), 7);
}

#[test]
fn out_of_range_values() {
    check!(
        "below, above, NaN, infinities",
        [-3.0, 7.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY].map(srgb_byte),
        [0, 255, 0, 255, 0]
    );
}

#[test]
fn pixels_in_order() {
    let bytes = to_rgb8(&[v3(0.0, 1.0, 0.5), v3(1.0, 0.0, 0.0)]);
    check!("r g b r g b", bytes, vec![0, 255, 188, 255, 0, 0]);
}

#[test]
fn a_two_by_two_picture() {
    let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9];
    let png = encode_png(2, 2, &rgb).unwrap();
    let got = decode(&png);
    check!("decodes", got, Ok((2, 2, rgb.to_vec())));
}

#[test]
fn sizes_that_cannot_be_written() {
    check!("no width", encode_png(0, 4, &[]), None);
    check!("no height", encode_png(4, 0, &[]), None);
    check!("too few bytes", encode_png(2, 2, &[0; 11]), None);
    check!("too many bytes", encode_png(2, 2, &[0; 13]), None);
}
