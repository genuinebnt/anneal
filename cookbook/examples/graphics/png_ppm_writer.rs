//! RECIPE 251 (graphics): write and read images without crates: PPM (P6) and a minimal PNG encoder + decoder, sRGB, ASCII preview
//! LEVEL: intermediate
//! Use in: renderers and simulators that must dump pictures, dataset generation, debugging pixel buffers.   Run: cargo run --release --example png_ppm_writer
#![allow(dead_code)]
//!
//! WHAT: PPM P6 is `P6\nW H\n255\n` + raw RGB bytes: the simplest image format. PNG is a signature, then chunks
//! [length u32 BE][type 4 bytes][data][CRC-32 of type+data]. We write IHDR, IDAT, IEND. The IDAT payload is a ZLIB stream:
//! 2 header bytes (0x78 0x01), DEFLATE blocks, then the Adler-32 of the uncompressed data (big endian).
//! WHY: knowing the byte layout lets you emit pictures from any program, and a decoder for the same subset checks the
//! encoder by round trip (including CRC and Adler checks) instead of "it opens in a viewer". A round trip only shows that
//! the two agree, so the 2x2 PNG is also compared byte for byte and the Huffman decoder reads a stream made by zlib.
//! LIMITS: the decoder reads only what is described here (8-bit RGB, no interlace, stored and fixed-Huffman blocks), so
//! most PNG files from other programs are refused. Both decoders refuse images above 2^26 pixels, and inflate stops as
//! soon as there is more data than the header's size needs.
//! HOW (plain text):
//!   scanline = [filter type byte] + row bytes. Filters: 0 None, 1 Sub (x - left), 2 Up (x - above), 3 Average
//!     (x - floor((left+above)/2)), 4 Paeth (x - predictor(left, above, upper-left) choosing the closest to left+above-upleft).
//!     Undoing them is the same arithmetic modulo 256 in the reverse direction.
//!   DEFLATE STORED block: 1 byte (BFINAL bit | BTYPE=00 << 1), LEN u16 LE, NLEN = !LEN u16 LE, then LEN raw bytes (<= 65535).
//!   DEFLATE FIXED-HUFFMAN block: BTYPE=01; literals 0-143 use 8-bit codes 00110000+n, 144-255 use 9-bit codes, end-of-block
//!     (256) is the 7-bit code 0; length/distance pairs (LZ77 matches, length 3..258, distance 1..32768) use tables plus
//!     extra bits. Huffman codes are packed MSB-first into an LSB-first bit stream; extra bits are LSB-first.
//!   CRC-32: reflected polynomial 0xEDB88320, init 0xFFFFFFFF, final xor. Adler-32: a = 1 + sum(bytes), b = sum(a) (mod 65521),
//!     result = b << 16 | a.
//!   sRGB: encode(c) = 12.92 c if c <= 0.0031308 else 1.055 c^(1/2.4) - 0.055. Linear 0.5 -> 0.7354 -> 188/255.
//! COMMON USES: ray/path tracers and simulators dumping frames, ML data generation (synthetic images), game tools,
//!   test fixtures; PPM is also popular as a zero-dependency debug output.
//! Read: https://raytracing.github.io/ and https://www.pbr-book.org/
use std::path::PathBuf;

// ---------- checksums ----------
fn crc32_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, slot) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    t
}
fn crc32(data: &[u8]) -> u32 {
    let t = crc32_table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

// ---------- seeded RNG ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ---------- image type ----------
#[derive(Clone, PartialEq, Debug)]
struct Image {
    w: usize,
    h: usize,
    rgb: Vec<u8>, // w*h*3
}

/// The largest image either decoder accepts (192 MB of pixels).
const MAX_PIXELS: usize = 1 << 26;
fn pixel_bytes(w: usize, h: usize) -> Result<usize, String> {
    match w.checked_mul(h) {
        Some(n) if n <= MAX_PIXELS && w <= MAX_PIXELS && h <= MAX_PIXELS => Ok(n * 3),
        _ => Err(format!("{w}x{h} is too large")),
    }
}

// ---------- PPM ----------
fn write_ppm(img: &Image) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", img.w, img.h).into_bytes();
    out.extend_from_slice(&img.rgb);
    out
}
fn read_ppm(data: &[u8]) -> Result<Image, String> {
    let mut pos = 0;
    // tokens separated by whitespace; '#' starts a comment to end of line
    let token = |pos: &mut usize| -> Result<String, String> {
        loop {
            while *pos < data.len() && data[*pos].is_ascii_whitespace() {
                *pos += 1;
            }
            if *pos < data.len() && data[*pos] == b'#' {
                while *pos < data.len() && data[*pos] != b'\n' {
                    *pos += 1;
                }
            } else {
                break;
            }
        }
        let start = *pos;
        while *pos < data.len() && !data[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        if start == *pos {
            Err("unexpected end of header".into())
        } else {
            Ok(String::from_utf8_lossy(&data[start..*pos]).into_owned())
        }
    };
    if token(&mut pos)? != "P6" {
        return Err("not a P6 file".into());
    }
    let w: usize = token(&mut pos)?.parse().map_err(|_| "bad width")?;
    let h: usize = token(&mut pos)?.parse().map_err(|_| "bad height")?;
    let max: usize = token(&mut pos)?.parse().map_err(|_| "bad maxval")?;
    if max != 255 {
        return Err("only maxval 255 supported".into());
    }
    pos += 1; // exactly one whitespace byte after maxval
    let need = pixel_bytes(w, h)?; // the header is untrusted: `w * h * 3` can wrap to a small number
    if data.len() < pos || data.len() - pos < need {
        return Err("truncated pixel data".into());
    }
    Ok(Image {
        w,
        h,
        rgb: data[pos..pos + need].to_vec(),
    })
}

// ---------- bit writer / reader (DEFLATE order) ----------
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}
impl BitWriter {
    fn new() -> Self {
        BitWriter {
            out: vec![],
            acc: 0,
            n: 0,
        }
    }
    /// LSB-first
    fn bits(&mut self, v: u32, count: u32) {
        self.acc |= v << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman codes go MSB-first: reverse then write LSB-first.
    fn huff(&mut self, code: u32, len: u32) {
        let mut r = 0;
        for i in 0..len {
            r |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.bits(r, len);
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize, // bit position
}
impl BitReader<'_> {
    fn bit(&mut self) -> Result<u32, String> {
        let byte = *self
            .data
            .get(self.pos / 8)
            .ok_or("deflate stream truncated")?;
        let b = (byte >> (self.pos % 8)) & 1;
        self.pos += 1;
        Ok(b as u32)
    }
    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }
    fn align(&mut self) {
        self.pos = self.pos.div_ceil(8) * 8;
    }
}

const LBASE: [u32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXTRA: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DBASE: [u32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXTRA: [u32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

// ---------- DEFLATE encoders ----------
/// Stored blocks of at most 65535 bytes each. Always valid, never smaller.
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.chunks(65535).collect()
    };
    for (i, c) in chunks.iter().enumerate() {
        out.push(if i + 1 == chunks.len() { 1 } else { 0 }); // BFINAL, BTYPE=00
        out.extend_from_slice(&(c.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        out.extend_from_slice(c);
    }
    out
}

fn fixed_literal(w: &mut BitWriter, sym: u32) {
    match sym {
        0..=143 => w.huff(0b0011_0000 + sym, 8),
        144..=255 => w.huff(0b1_1001_0000 + (sym - 144), 9),
        256..=279 => w.huff(sym - 256, 7),
        _ => w.huff(0b1100_0000 + (sym - 280), 8),
    }
}

/// One fixed-Huffman block with greedy LZ77 (hash chains, 16 probes, 32 KiB window).
fn deflate_fixed(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.bits(1, 1); // BFINAL
    w.bits(1, 2); // BTYPE = 01
    let n = data.len();
    let mut head = vec![usize::MAX; 1 << 15];
    let mut prev = vec![usize::MAX; n];
    let hash = |i: usize| {
        ((data[i] as usize) << 10 ^ (data[i + 1] as usize) << 5 ^ data[i + 2] as usize) & 0x7FFF
    };
    let mut i = 0;
    while i < n {
        let (mut best_len, mut best_dist) = (0usize, 0usize);
        if i + 3 <= n {
            let mut cand = head[hash(i)];
            let mut probes = 0;
            while cand != usize::MAX && i - cand <= 32768 && probes < 16 {
                let max = (n - i).min(258);
                let mut l = 0;
                while l < max && data[cand + l] == data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = i - cand;
                }
                cand = prev[cand];
                probes += 1;
            }
        }
        let advance = if best_len >= 3 {
            let lc = LBASE.iter().rposition(|&b| b <= best_len as u32).unwrap();
            fixed_literal(&mut w, 257 + lc as u32);
            w.bits(best_len as u32 - LBASE[lc], LEXTRA[lc]);
            let dc = DBASE.iter().rposition(|&b| b <= best_dist as u32).unwrap();
            w.huff(dc as u32, 5);
            w.bits(best_dist as u32 - DBASE[dc], DEXTRA[dc]);
            best_len
        } else {
            fixed_literal(&mut w, data[i] as u32);
            1
        };
        for k in i..i + advance {
            if k + 3 <= n {
                let h = hash(k);
                prev[k] = head[h];
                head[h] = k;
            }
        }
        i += advance;
    }
    fixed_literal(&mut w, 256);
    w.finish()
}

// ---------- INFLATE (stored + fixed Huffman only) ----------
const TOO_MUCH: &str = "more data than expected";
fn read_fixed_symbol(r: &mut BitReader) -> Result<u32, String> {
    let mut code = 0u32;
    for len in 1..=9 {
        code = (code << 1) | r.bit()?;
        match len {
            7 if code <= 0b001_0111 => return Ok(256 + code),
            8 if (0b0011_0000..=0b1011_1111).contains(&code) => return Ok(code - 0b0011_0000),
            8 if (0b1100_0000..=0b1100_0111).contains(&code) => return Ok(280 + code - 0b1100_0000),
            9 if code >= 0b1_1001_0000 => return Ok(144 + code - 0b1_1001_0000),
            _ => {}
        }
    }
    Err("bad fixed Huffman code".into())
}
/// Refuses to produce more than `limit` bytes: a small stream can expand more than a hundredfold.
fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut r = BitReader { data, pos: 0 };
    let mut out: Vec<u8> = vec![];
    loop {
        let last = r.bit()?;
        match r.bits(2)? {
            0 => {
                r.align();
                let p = r.pos / 8;
                if p + 4 > data.len() {
                    return Err("truncated stored header".into());
                }
                let len = u16::from_le_bytes([data[p], data[p + 1]]);
                let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]);
                if len != !nlen {
                    return Err("stored LEN/NLEN mismatch".into());
                }
                let s = p + 4;
                let e = s + len as usize;
                if e > data.len() {
                    return Err("truncated stored block".into());
                }
                if len as usize > limit - out.len() {
                    return Err(TOO_MUCH.into());
                }
                out.extend_from_slice(&data[s..e]);
                r.pos = e * 8;
            }
            1 => loop {
                let sym = read_fixed_symbol(&mut r)?;
                match sym {
                    0..=255 if out.len() < limit => out.push(sym as u8),
                    0..=255 => return Err(TOO_MUCH.into()),
                    256 => break,
                    257..=285 => {
                        let li = (sym - 257) as usize;
                        let len = (LBASE[li] + r.bits(LEXTRA[li])?) as usize;
                        let mut dc = 0u32;
                        for _ in 0..5 {
                            dc = (dc << 1) | r.bit()?;
                        }
                        if dc >= 30 {
                            return Err("bad distance code".into());
                        }
                        let dist = (DBASE[dc as usize] + r.bits(DEXTRA[dc as usize])?) as usize;
                        if dist > out.len() {
                            return Err("distance too far back".into());
                        }
                        if len > limit - out.len() {
                            return Err(TOO_MUCH.into());
                        }
                        for _ in 0..len {
                            out.push(out[out.len() - dist]); // byte by byte: matches may overlap themselves
                        }
                    }
                    _ => return Err("bad symbol".into()),
                }
            },
            t => {
                return Err(format!(
                    "unsupported deflate block type {t} (dynamic Huffman not implemented)"
                ));
            }
        }
        if last == 1 {
            r.align();
            return Ok(out);
        }
    }
}

// ---------- PNG ----------
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (a as i32, b as i32, c as i32);
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}
/// Predictor value for filter `ft` at byte `i` of a row. `left`, `up`, `ul` are the neighbours (0 outside the image).
fn predict(ft: u8, left: u8, up: u8, ul: u8) -> u8 {
    match ft {
        0 => 0,
        1 => left,
        2 => up,
        3 => ((left as u16 + up as u16) / 2) as u8,
        _ => paeth(left, up, ul),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Compression {
    Stored,
    FixedHuffman,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Filtering {
    None,
    Adaptive,
}

fn png_encode(img: &Image, comp: Compression, filt: Filtering) -> Vec<u8> {
    let stride = img.w * 3;
    let mut raw = Vec::with_capacity((stride + 1) * img.h);
    for y in 0..img.h {
        let row = &img.rgb[y * stride..(y + 1) * stride];
        let prev: Option<&[u8]> = if y > 0 {
            Some(&img.rgb[(y - 1) * stride..y * stride])
        } else {
            None
        };
        let filtered = |ft: u8| -> Vec<u8> {
            (0..stride)
                .map(|i| {
                    let left = if i >= 3 { row[i - 3] } else { 0 };
                    let up = prev.map_or(0, |p| p[i]);
                    let ul = if i >= 3 {
                        prev.map_or(0, |p| p[i - 3])
                    } else {
                        0
                    };
                    row[i].wrapping_sub(predict(ft, left, up, ul))
                })
                .collect()
        };
        let (ft, bytes) = match filt {
            Filtering::None => (0u8, filtered(0)),
            // the standard heuristic: pick the filter whose output, read as signed bytes, has the smallest absolute sum
            Filtering::Adaptive => (0u8..5)
                .map(|ft| (ft, filtered(ft)))
                .min_by_key(|(_, b)| {
                    b.iter()
                        .map(|&v| (v as i8).unsigned_abs() as u64)
                        .sum::<u64>()
                })
                .unwrap(),
        };
        raw.push(ft);
        raw.extend_from_slice(&bytes);
    }
    let mut z = vec![0x78, 0x01]; // CMF/FLG: deflate, 32K window; (0x78*256 + 0x01) % 31 == 0
    z.extend(match comp {
        Compression::Stored => deflate_stored(&raw),
        Compression::FixedHuffman => deflate_fixed(&raw),
    });
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = PNG_SIG.to_vec();
    let mut ihdr = vec![];
    ihdr.extend_from_slice(&(img.w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(img.h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, colour type 2 (RGB), deflate, filter method 0, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn png_decode(data: &[u8]) -> Result<Image, String> {
    if data.len() < 8 || data[..8] != PNG_SIG {
        return Err("bad PNG signature".into());
    }
    let mut pos = 8;
    let (mut w, mut h) = (0usize, 0usize);
    let mut idat = vec![];
    let mut seen_end = false;
    while pos + 12 <= data.len() && !seen_end {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        if pos + 12 + len > data.len() {
            return Err("chunk exceeds file".into());
        }
        let body = &data[pos + 4..pos + 8 + len];
        let stored_crc =
            u32::from_be_bytes(data[pos + 8 + len..pos + 12 + len].try_into().unwrap());
        if crc32(body) != stored_crc {
            return Err(format!(
                "CRC mismatch in chunk {}",
                String::from_utf8_lossy(&body[..4])
            ));
        }
        let payload = &body[4..];
        match &body[..4] {
            b"IHDR" => {
                if payload.len() != 13 || payload[8..] != [8, 2, 0, 0, 0] {
                    return Err("only 8-bit RGB, non-interlaced PNG supported".into());
                }
                w = u32::from_be_bytes(payload[0..4].try_into().unwrap()) as usize;
                h = u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize;
            }
            b"IDAT" => idat.extend_from_slice(payload),
            b"IEND" => seen_end = true,
            _ => {} // ancillary chunks are ignored
        }
        pos += 12 + len;
    }
    if !seen_end || w == 0 || h == 0 {
        return Err("missing IHDR/IEND".into());
    }
    if idat.len() < 6 || ((idat[0] as u32) << 8 | idat[1] as u32) % 31 != 0 || idat[0] & 0x0F != 8 {
        return Err("bad zlib header".into());
    }
    pixel_bytes(w, h)?;
    let stride = w * 3; // cannot overflow, nor can the size below: both sides are at most MAX_PIXELS
    let size = (stride + 1) * h;
    let body = &idat[2..idat.len() - 4];
    let raw = inflate(body, size)?;
    let adler = u32::from_be_bytes(idat[idat.len() - 4..].try_into().unwrap());
    if adler != adler32(&raw) {
        return Err("Adler-32 mismatch".into());
    }
    if raw.len() != size {
        return Err("decompressed size does not match IHDR".into());
    }
    let mut rgb = vec![0u8; stride * h];
    for y in 0..h {
        let ft = raw[y * (stride + 1)];
        if ft > 4 {
            return Err(format!("bad filter type {ft}"));
        }
        for i in 0..stride {
            let left = if i >= 3 { rgb[y * stride + i - 3] } else { 0 };
            let up = if y > 0 { rgb[(y - 1) * stride + i] } else { 0 };
            let ul = if y > 0 && i >= 3 {
                rgb[(y - 1) * stride + i - 3]
            } else {
                0
            };
            rgb[y * stride + i] =
                raw[y * (stride + 1) + 1 + i].wrapping_add(predict(ft, left, up, ul));
        }
    }
    Ok(Image { w, h, rgb })
}

// ---------- colour ----------
fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn quantize(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Terminal preview: average luminance per cell, mapped onto a dark-to-light character ramp.
fn ascii_preview(img: &Image, cols: usize, rows: usize) -> String {
    const RAMP: &[u8] = b" .:-=+*#%@";
    let mut s = String::new();
    for ry in 0..rows {
        for rx in 0..cols {
            let (x0, x1) = (
                rx * img.w / cols,
                ((rx + 1) * img.w / cols).max(rx * img.w / cols + 1),
            );
            let (y0, y1) = (
                ry * img.h / rows,
                ((ry + 1) * img.h / rows).max(ry * img.h / rows + 1),
            );
            let (mut sum, mut cnt) = (0.0, 0.0);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &img.rgb[(y * img.w + x) * 3..][..3];
                    sum += 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
                    cnt += 1.0;
                }
            }
            s.push(RAMP[((sum / cnt / 255.0) * (RAMP.len() - 1) as f64).round() as usize] as char);
        }
        s.push('\n');
    }
    s
}

fn gallery_dir() -> PathBuf {
    let d = std::env::var_os("COOKBOOK_GALLERY")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("flowforge_gallery"));
    std::fs::create_dir_all(&d).expect("create gallery dir");
    d
}

fn gradient(w: usize, h: usize) -> Image {
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f64 / (w - 1) as f64, y as f64 / (h - 1) as f64);
            let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
            let ring = 0.5 + 0.5 * (d * 40.0).cos();
            // computed in LINEAR light, then sRGB-encoded for display
            for lin in [
                u * ring,
                v * (1.0 - d).max(0.0),
                (1.0 - u) * 0.5 + 0.1 * ring,
            ] {
                rgb.push(quantize(linear_to_srgb(lin)));
            }
        }
    }
    Image { w, h, rgb }
}

fn main() {
    // --- checksum known answers
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(
        crc32(b"IEND"),
        0xAE42_6082,
        "every PNG ends with the same IEND CRC"
    );
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);

    // --- PNG layout, byte for byte, for a 2x2 image (stored deflate, filter 0)
    let img2 = Image {
        w: 2,
        h: 2,
        rgb: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
    };
    let png = png_encode(&img2, Compression::Stored, Filtering::None);
    let raw = [
        0u8, 255, 0, 0, 0, 255, 0, /* row 2 */ 0, 0, 0, 255, 255, 255, 255,
    ]; // filter byte + 6 bytes per row
    let mut zlib = vec![0x78, 0x01, 0x01, 14, 0, !14u8, 0xFF];
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut expect = PNG_SIG.to_vec();
    let ihdr = [0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0];
    expect.extend_from_slice(&[0, 0, 0, 13]);
    expect.extend_from_slice(b"IHDR");
    expect.extend_from_slice(&ihdr);
    let mut ih = b"IHDR".to_vec();
    ih.extend_from_slice(&ihdr);
    expect.extend_from_slice(&crc32(&ih).to_be_bytes());
    expect.extend_from_slice(&(zlib.len() as u32).to_be_bytes());
    expect.extend_from_slice(b"IDAT");
    expect.extend_from_slice(&zlib);
    let mut id = b"IDAT".to_vec();
    id.extend_from_slice(&zlib);
    expect.extend_from_slice(&crc32(&id).to_be_bytes());
    expect.extend_from_slice(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
    assert_eq!(png, expect, "2x2 PNG must match the hand-derived bytes");
    assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert_eq!(png_decode(&png).unwrap(), img2);
    println!(
        "2x2 PNG: {} bytes, layout verified byte for byte (IHDR CRC {:08X})",
        png.len(),
        crc32(&ih)
    );

    // --- corruption is detected
    let mut bad = png.clone();
    bad[40] ^= 1; // inside IDAT payload
    assert!(png_decode(&bad).unwrap_err().contains("CRC"));
    let mut bad2 = png.clone();
    let n = bad2.len();
    bad2[0] = 0;
    assert!(png_decode(&bad2).unwrap_err().contains("signature"));
    // flip a pixel byte AND repair the chunk CRC: the Adler-32 must still catch it
    let mut bad3 = png.clone();
    let idat_start = 8 + 25 + 8; // signature + IHDR chunk + IDAT length/type
    bad3[idat_start + 8] ^= 0x10;
    let idat_len = u32::from_be_bytes(bad3[8 + 25..8 + 29].try_into().unwrap()) as usize;
    let crc = crc32(&bad3[8 + 29..8 + 29 + 4 + idat_len]);
    let at = 8 + 29 + 4 + idat_len;
    bad3[at..at + 4].copy_from_slice(&crc.to_be_bytes());
    assert!(png_decode(&bad3).unwrap_err().contains("Adler"));
    assert!(n > 0);
    println!("corruption detection ok (CRC, signature, Adler)");

    // --- PPM round trip with a comment in the header
    let rd =
        read_ppm(b"P6\n# a comment\n2 2\n255\n\xff\x00\x00\x00\xff\x00\x00\x00\xff\xff\xff\xff")
            .unwrap();
    assert_eq!(rd, img2);
    assert_eq!(read_ppm(&write_ppm(&img2)).unwrap(), img2);
    assert!(read_ppm(b"P3\n1 1\n255\n").is_err());

    // --- headers and streams that lie are errors, not panics, wraps or huge allocations
    // 3 * this width is 2 modulo 2^64, so unchecked arithmetic accepts two bytes of pixels
    assert!(read_ppm(b"P6 6148914691236517206 1 255\nab").is_err());
    assert!(read_ppm(b"P6 4294967296 4294967296 255\n").is_err());
    assert!(read_ppm(b"P6 0 0 255").is_err(), "no byte after maxval");
    assert!(read_ppm(b"P6 0 0 255\n").unwrap().rgb.is_empty());
    let claim = |w: u32, h: u32, raw: &[u8]| {
        let mut z = vec![0x78, 0x01];
        z.extend(deflate_fixed(raw));
        z.extend(adler32(raw).to_be_bytes());
        let mut ihdr = [w.to_be_bytes(), h.to_be_bytes()].concat();
        ihdr.extend([8, 2, 0, 0, 0]);
        let mut out = PNG_SIG.to_vec();
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        out
    };
    let huge = png_decode(&claim(u32::MAX, u32::MAX, b"x"));
    assert!(huge.unwrap_err().contains("too large"));
    let bomb = png_decode(&claim(1, 1, &[0; 100_000])); // a 1x1 image is 4 bytes
    assert_eq!(bomb.unwrap_err(), TOO_MUCH);
    let stored_bomb = deflate_stored(&[7; 100]);
    assert_eq!(inflate(&stored_bomb, 99).unwrap_err(), TOO_MUCH);
    assert_eq!(inflate(&stored_bomb, 100).unwrap().len(), 100);
    let literals = deflate_fixed(b"abc");
    assert_eq!(inflate(&literals, 2).unwrap_err(), TOO_MUCH);

    // --- a fixed-Huffman stream made by ANOTHER implementation (Python 3 zlib, strategy Z_FIXED, raw deflate). A round
    // trip alone cannot catch a mistake that the encoder and decoder here share, such as a wrong table entry.
    let mut other = vec![0u8, 143, 144, 200, 255];
    other.extend(b"abc".repeat(5));
    other.extend([b'z'; 300]);
    other.extend([0u8, 143, 144, 200, 255]);
    let from_zlib = [
        0x63, 0xe8, 0x9f, 0x70, 0xe2, 0x7f, 0x62, 0x52, 0x32, 0x32, 0xaa, 0x1a, 0x05, 0x44, 0x03,
        0x06, 0x50, 0xf8, 0x01, 0x00,
    ];
    assert_eq!(inflate(&from_zlib, other.len()).unwrap(), other);

    // and every table entry, from RFC 1951's rule: each code's range of lengths or distances starts where the one before
    // ends (the last two length codes are the exception: 284 stops at 258, which also has its own code 285)
    assert!((LBASE[0], LBASE[28], DBASE[0]) == (3, 258, 1) && LBASE[27] + 31 == 258);
    assert!((0..27).all(|i| LBASE[i + 1] == LBASE[i] + (1 << LEXTRA[i])));
    assert!((0..29).all(|i| DBASE[i + 1] == DBASE[i] + (1 << DEXTRA[i])));
    assert_eq!(DBASE[29] + (1 << DEXTRA[29]), 32769);

    // --- stored blocks are split at 65535 bytes
    let big = Image {
        w: 300,
        h: 100,
        rgb: (0..300 * 100 * 3).map(|i| (i * 7 % 251) as u8).collect(),
    };
    let png_big = png_encode(&big, Compression::Stored, Filtering::None);
    let raw_len = (300 * 3 + 1) * 100; // 90100 bytes -> 2 stored blocks
    assert_eq!(png_big.len(), 8 + 25 + (12 + 2 + raw_len + 2 * 5 + 4) + 12);
    assert_eq!(png_decode(&png_big).unwrap(), big);

    // --- random round trips over every compression/filter combination
    let mut rng = Rng(2024);
    let mut trials = 0;
    for _ in 0..60 {
        let (w, h) = (1 + rng.below(40) as usize, 1 + rng.below(30) as usize);
        let style = rng.below(3);
        let rgb: Vec<u8> = (0..w * h * 3)
            .map(|i| match style {
                0 => rng.next() as u8,                            // noise: incompressible
                1 => ((i / 3 % w) * 6 + (i / (3 * w)) * 3) as u8, // smooth gradient: filters help
                _ => {
                    if rng.below(8) == 0 {
                        rng.next() as u8
                    } else {
                        200
                    }
                } // flat with speckles: LZ77 helps
            })
            .collect();
        let img = Image { w, h, rgb };
        for comp in [Compression::Stored, Compression::FixedHuffman] {
            for filt in [Filtering::None, Filtering::Adaptive] {
                let p = png_encode(&img, comp, filt);
                assert_eq!(
                    png_decode(&p).expect("decode"),
                    img,
                    "{comp:?} {filt:?} {w}x{h}"
                );
                trials += 1;
            }
        }
    }
    println!("{trials} random PNG round trips ok (stored/fixed-Huffman x none/adaptive filters)");

    // --- sRGB
    for i in 0..=255 {
        let lin = srgb_to_linear(i as f64 / 255.0);
        assert_eq!(
            quantize(linear_to_srgb(lin)),
            i as u8,
            "sRGB round trip at {i}"
        );
    }
    let mid = quantize(linear_to_srgb(0.5));
    assert!((187..=189).contains(&mid));
    println!(
        "sRGB: linear 0.5 -> {mid}/255; 8-bit round trip exact for all 256 codes; sRGB 128 -> linear {:.4}",
        srgb_to_linear(128.0 / 255.0)
    );

    // --- gradient image: sizes of the different encodings, files, ASCII preview
    let g = gradient(256, 192);
    let stored = png_encode(&g, Compression::Stored, Filtering::None);
    let comp = png_encode(&g, Compression::FixedHuffman, Filtering::Adaptive);
    let ppm = write_ppm(&g);
    assert_eq!(png_decode(&stored).unwrap(), g);
    assert_eq!(png_decode(&comp).unwrap(), g);
    assert!(comp.len() < stored.len());
    println!(
        "256x192 gradient: PPM {} B, PNG stored {} B, PNG fixed-Huffman+filters {} B ({:.1}x smaller)",
        ppm.len(),
        stored.len(),
        comp.len(),
        stored.len() as f64 / comp.len() as f64
    );
    let dir = gallery_dir();
    for (name, bytes) in [
        ("r251_gradient_stored.png", &stored),
        ("r251_gradient_compressed.png", &comp),
        ("r251_gradient.ppm", &ppm),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write image");
        println!("wrote {}", path.display());
    }
    let back = read_ppm(&std::fs::read(dir.join("r251_gradient.ppm")).unwrap()).unwrap();
    assert_eq!(back, g);
    print!("{}", ascii_preview(&g, 64, 20));
    println!("all checks passed");
}
