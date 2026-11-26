//! RECIPE 158 (messaging): a Kafka-style record batch codec: zigzag varints, CRC32C, delta-encoded records, zero-copy decode.
//! LEVEL: intermediate
//! Use in: message brokers, log/event storage, any length-prefixed binary wire format.   Run: cargo run --release --example record_batch_codec
#![allow(dead_code)]
//!
//! WHAT: a simplified version of Kafka's v2 record batch. Layout (big endian header, varint body):
//!   base_offset i64 | batch_length i32 | magic u8 | crc u32 | attributes u16 | count u32 | base_timestamp i64 | records...
//! batch_length counts the bytes AFTER the length field (so a reader can skip a batch it does not understand), and the
//! CRC32C covers everything after the crc field. base_offset is deliberately NOT covered, as in Kafka: a broker assigns
//! offsets after the producer computed the checksum, so it may rewrite that field without re-checksumming the payload.
//!
//! WHY deltas + varints: consecutive records have nearly identical timestamps and consecutive offsets, so storing
//! (offset - base_offset) and (timestamp - base_timestamp) as zigzag varints costs 1 byte each instead of 16. Zigzag maps
//! small negative numbers to small unsigned ones (0,-1,1,-2 -> 0,1,2,3) so they stay short in LEB128 varint form.
//! Lengths are zigzag too, which lets -1 mean "null" (a null key is not an empty key; a null value is a tombstone).
//!
//! HOW it stays safe: every read goes through `Reader`, which bounds-checks and returns a typed `CodecError`; counts and
//! lengths coming off the wire are never trusted for allocation. Decoding is split in two phases so lifetimes are honest:
//! `parse_batch` verifies framing and CRC and borrows the payload; `decode_records` yields `Record<'b>` whose key, value
//! and header slices point INTO the input buffer (zero copy). If the payload was compressed, the decompressed bytes live in
//! a caller-owned scratch buffer and the records borrow from that instead.
//!
//! COMMON USES: Kafka-compatible brokers and clients batch records exactly like this to amortize per-message overhead;
//! protobuf and many columnar/log formats use zigzag varints; storage engines checksum blocks with CRC32C (also used
//! by iSCSI and ext4 metadata); the borrow-from-input pattern is how parsers like `httparse` avoid allocation.
//! Read: https://kafka.apache.org/documentation/#recordbatch , https://protobuf.dev/programming-guides/encoding/ (varint, zigzag),
//! https://en.wikipedia.org/wiki/Cyclic_redundancy_check

const MAGIC: u8 = 2;
const HEADER_LEN: usize = 31; // fixed bytes before the payload
const LEN_FIELD_END: usize = 12; // batch_length counts bytes after this position
const CRC_FIELD: std::ops::Range<usize> = 13..17;
const ATTR_RLE: u16 = 1;
const MAX_DECOMPRESSED: usize = 64 << 20; // refuse decompression bombs

#[derive(Debug, PartialEq, Clone)]
enum CodecError {
    Truncated { needed: usize, have: usize },
    VarintOverflow,
    BadLength(i64),
    BadMagic(u8),
    CrcMismatch { stored: u32, computed: u32 },
    BadAttributes(u16),
    BadCompression(&'static str),
    OffsetOverflow,
    RecordLengthMismatch,
}
type Result<T> = std::result::Result<T, CodecError>;

// ---------------------------------------------------------------- CRC32C (Castagnoli)

/// Table for the reflected polynomial 0x82F63B78; built at compile time so there is no runtime init.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 == 1 {
                (c >> 1) ^ 0x82F6_3B78
            } else {
                c >> 1
            };
            bit += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

fn crc32c(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

// ---------------------------------------------------------------- zigzag + varint

fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}
fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}
fn put_varint(mut n: u64, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push((n & 0x7f) as u8 | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}
fn put_signed(n: i64, out: &mut Vec<u8>) {
    put_varint(zigzag(n), out);
}
fn put_nullable(b: Option<&[u8]>, out: &mut Vec<u8>) {
    match b {
        None => put_signed(-1, out),
        Some(b) => {
            put_signed(b.len() as i64, out);
            out.extend_from_slice(b);
        }
    }
}

// ---------------------------------------------------------------- bounds-checked reader

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    /// the only place that slices the buffer; returns a slice with the INPUT's lifetime, not the reader's
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        let end = end.ok_or(CodecError::Truncated {
            needed: n,
            have: self.remaining(),
        })?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn varint(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for i in 0..10 {
            let b = self.u8()?;
            if i == 9 && b > 1 {
                return Err(CodecError::VarintOverflow); // would not fit in 64 bits
            }
            v |= ((b & 0x7f) as u64) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(CodecError::VarintOverflow)
    }
    fn signed(&mut self) -> Result<i64> {
        Ok(unzigzag(self.varint()?))
    }
    /// a non-negative length that must also fit in what is left (stops huge-allocation attacks)
    fn length(&mut self) -> Result<usize> {
        let n = self.signed()?;
        usize::try_from(n)
            .ok()
            .filter(|&n| n <= self.remaining())
            .ok_or(CodecError::BadLength(n))
    }
    fn nullable(&mut self) -> Result<Option<&'a [u8]>> {
        let n = self.signed()?;
        if n == -1 {
            return Ok(None);
        }
        let n = usize::try_from(n).ok().filter(|&n| n <= self.remaining());
        Ok(Some(self.take(n.ok_or(CodecError::BadLength(
            n.map_or(-2, |x| x as i64),
        ))?)?))
    }
}

// ---------------------------------------------------------------- compression stand-in (PackBits-style RLE)

/// token < 128: copy token+1 literal bytes; token >= 128: repeat the next byte (token-128+3) times. Prefixed by the length.
fn rle_compress(src: &[u8]) -> Vec<u8> {
    let mut out = (src.len() as u32).to_be_bytes().to_vec();
    let mut lit: Vec<u8> = vec![];
    let flush = |lit: &mut Vec<u8>, out: &mut Vec<u8>| {
        if !lit.is_empty() {
            out.push((lit.len() - 1) as u8);
            out.append(lit);
        }
    };
    let mut i = 0;
    while i < src.len() {
        let run = src[i..]
            .iter()
            .take(130)
            .take_while(|&&b| b == src[i])
            .count();
        if run >= 3 {
            flush(&mut lit, &mut out);
            out.extend([128 + (run - 3) as u8, src[i]]);
            i += run;
        } else {
            lit.push(src[i]);
            i += 1;
            if lit.len() == 128 {
                flush(&mut lit, &mut out);
            }
        }
    }
    flush(&mut lit, &mut out);
    out
}

fn rle_decompress(src: &[u8]) -> Result<Vec<u8>> {
    let bad = CodecError::BadCompression;
    let mut r = Reader::new(src);
    let want = r.u32()? as usize;
    if want > MAX_DECOMPRESSED {
        return Err(bad("declared size too large"));
    }
    let mut out = Vec::with_capacity(want.min(src.len().saturating_mul(130)));
    while r.remaining() > 0 {
        let t = r.u8()? as usize;
        if t < 128 {
            out.extend_from_slice(r.take(t + 1)?);
        } else {
            let b = r.u8()?;
            out.resize(out.len() + t - 128 + 3, b);
        }
        if out.len() > want {
            return Err(bad("output exceeds declared size"));
        }
    }
    if out.len() != want {
        return Err(bad("output shorter than declared size"));
    }
    Ok(out)
}

// ---------------------------------------------------------------- records and the encoder

#[derive(Debug, PartialEq, Clone)]
struct Record<'a> {
    offset: i64,
    timestamp: i64,
    key: Option<&'a [u8]>,
    value: Option<&'a [u8]>,
    headers: Vec<(&'a [u8], &'a [u8])>,
}

fn encode_record(r: &Record, base_offset: i64, base_ts: i64, out: &mut Vec<u8>) {
    let mut body = vec![0u8]; // per-record attributes, unused
    put_signed(r.timestamp - base_ts, &mut body);
    put_signed(r.offset - base_offset, &mut body);
    put_nullable(r.key, &mut body);
    put_nullable(r.value, &mut body);
    put_signed(r.headers.len() as i64, &mut body);
    for (k, v) in &r.headers {
        put_nullable(Some(k), &mut body);
        put_nullable(Some(v), &mut body);
    }
    put_signed(body.len() as i64, out); // length prefix lets a reader skip a record
    out.extend_from_slice(&body);
}

/// offsets are taken from the records; the first record defines both bases. `compress` only sticks if it actually helps.
fn encode_batch(records: &[Record], compress: bool) -> Vec<u8> {
    let (base_offset, base_ts) = records.first().map_or((0, 0), |r| (r.offset, r.timestamp));
    let mut payload = vec![];
    for r in records {
        encode_record(r, base_offset, base_ts, &mut payload);
    }
    let mut attributes = 0u16;
    if compress {
        let packed = rle_compress(&payload);
        if packed.len() < payload.len() {
            payload = packed;
            attributes |= ATTR_RLE;
        }
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&base_offset.to_be_bytes());
    out.extend_from_slice(&0i32.to_be_bytes()); // length, patched below
    out.push(MAGIC);
    out.extend_from_slice(&0u32.to_be_bytes()); // crc, patched below
    out.extend_from_slice(&attributes.to_be_bytes());
    out.extend_from_slice(&(records.len() as u32).to_be_bytes());
    out.extend_from_slice(&base_ts.to_be_bytes());
    out.extend_from_slice(&payload);
    let len = (out.len() - LEN_FIELD_END) as i32;
    out[8..12].copy_from_slice(&len.to_be_bytes());
    reseal(&mut out);
    out
}

/// recompute the CRC in place (used by the encoder and by the fuzzer to get past the checksum)
fn reseal(batch: &mut [u8]) {
    let crc = crc32c(&batch[CRC_FIELD.end..]);
    batch[CRC_FIELD].copy_from_slice(&crc.to_be_bytes());
}

// ---------------------------------------------------------------- the decoder

struct BatchView<'a> {
    base_offset: i64,
    attributes: u16,
    count: u32,
    base_timestamp: i64,
    payload: &'a [u8],
    total_len: usize, // bytes this batch occupies, so callers can step to the next one in a segment
}

/// phase 1: framing, magic, CRC, attributes. Nothing is allocated.
fn parse_batch(buf: &[u8]) -> Result<BatchView<'_>> {
    let mut r = Reader::new(buf);
    let base_offset = r.i64()?;
    let len = i32::from_be_bytes(r.take(4)?.try_into().unwrap());
    let min = HEADER_LEN - LEN_FIELD_END;
    let len = usize::try_from(len)
        .ok()
        .filter(|&l| l >= min)
        .ok_or(CodecError::BadLength(len as i64))?;
    let mut body = Reader::new(r.take(len)?);
    let magic = body.u8()?;
    if magic != MAGIC {
        return Err(CodecError::BadMagic(magic));
    }
    let stored = body.u32()?;
    let computed = crc32c(&body.buf[body.pos..]);
    if stored != computed {
        return Err(CodecError::CrcMismatch { stored, computed });
    }
    let attributes = body.u16()?;
    if attributes & !ATTR_RLE != 0 {
        return Err(CodecError::BadAttributes(attributes));
    }
    let (count, base_timestamp) = (body.u32()?, body.i64()?);
    let payload = body.take(body.remaining())?;
    Ok(BatchView {
        base_offset,
        attributes,
        count,
        base_timestamp,
        payload,
        total_len: LEN_FIELD_END + len,
    })
}

impl<'a> BatchView<'a> {
    /// the record bytes: borrowed straight from the input when uncompressed, else decompressed into `scratch`
    fn record_bytes<'s>(&self, scratch: &'s mut Vec<u8>) -> Result<&'s [u8]>
    where
        'a: 's,
    {
        if self.attributes & ATTR_RLE == 0 {
            return Ok(self.payload);
        }
        *scratch = rle_decompress(self.payload)?;
        Ok(scratch)
    }

    /// phase 2: parse records out of `bytes`; the results borrow from `bytes`
    fn records<'b>(&self, bytes: &'b [u8]) -> Result<Vec<Record<'b>>> {
        let mut r = Reader::new(bytes);
        // every record needs at least one byte, so a lying `count` cannot make us over-allocate
        let mut out = Vec::with_capacity((self.count as usize).min(bytes.len()));
        for _ in 0..self.count {
            let n = r.length()?;
            out.push(self.record(&mut Reader::new(r.take(n)?))?);
        }
        if r.remaining() != 0 {
            return Err(CodecError::RecordLengthMismatch);
        }
        Ok(out)
    }

    fn record<'b>(&self, r: &mut Reader<'b>) -> Result<Record<'b>> {
        r.u8()?; // attributes
        let ts_delta = r.signed()?;
        let off_delta = r.signed()?;
        let (key, value) = (r.nullable()?, r.nullable()?);
        let nh = r.length()?;
        let mut headers = Vec::with_capacity(nh.min(16));
        for _ in 0..nh {
            let (k, v) = (r.nullable()?, r.nullable()?);
            headers.push((k.unwrap_or_default(), v.unwrap_or_default()));
        }
        if r.remaining() != 0 {
            return Err(CodecError::RecordLengthMismatch);
        }
        let offset = self
            .base_offset
            .checked_add(off_delta)
            .ok_or(CodecError::OffsetOverflow)?;
        let timestamp = self
            .base_timestamp
            .checked_add(ts_delta)
            .ok_or(CodecError::OffsetOverflow)?;
        Ok(Record {
            offset,
            timestamp,
            key,
            value,
            headers,
        })
    }
}

/// convenience: both phases. Records borrow from `buf` (uncompressed) or from `scratch` (compressed).
fn decode_batch<'b>(buf: &'b [u8], scratch: &'b mut Vec<u8>) -> Result<Vec<Record<'b>>> {
    let view = parse_batch(buf)?;
    let bytes = view.record_bytes(scratch)?;
    view.records(bytes)
}

// ---------------------------------------------------------------- test helpers

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// random bytes; with some probability a long run so compression has something to find
    fn bytes(&mut self, max: u64) -> Vec<u8> {
        let n = self.below(max + 1) as usize;
        if self.below(3) == 0 {
            vec![self.next() as u8; n]
        } else {
            (0..n).map(|_| self.next() as u8).collect()
        }
    }
}

/// owned storage for one random batch; `view()` borrows it as records
struct Owned {
    base_offset: i64,
    rows: Vec<(
        i64,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        Vec<(Vec<u8>, Vec<u8>)>,
    )>,
}
fn random_owned(rng: &mut Rng) -> Owned {
    let base_offset = rng.below(1 << 40) as i64;
    let mut ts = rng.below(1 << 41) as i64;
    let rows = (0..1 + rng.below(40))
        .map(|_| {
            ts += rng.below(50) as i64 - 5; // mostly forward, sometimes slightly backwards (zigzag handles it)
            let opt = |rng: &mut Rng, m| {
                if rng.below(5) == 0 {
                    None
                } else {
                    Some(rng.bytes(m))
                }
            };
            let headers = (0..rng.below(3))
                .map(|_| (rng.bytes(8), rng.bytes(20)))
                .collect();
            (ts, opt(rng, 16), opt(rng, 120), headers)
        })
        .collect();
    Owned { base_offset, rows }
}
impl Owned {
    fn view(&self) -> Vec<Record<'_>> {
        let mut off = self.base_offset;
        let mut out = vec![];
        for (ts, k, v, h) in &self.rows {
            let headers = h.iter().map(|(a, b)| (&a[..], &b[..])).collect();
            out.push(Record {
                offset: off,
                timestamp: *ts,
                key: k.as_deref(),
                value: v.as_deref(),
                headers,
            });
            off += 1;
        }
        out
    }
}

fn test_primitives() {
    assert_eq!(
        crc32c(b"123456789"),
        0xE306_9283,
        "standard CRC32C check value"
    );
    assert_eq!(crc32c(b""), 0);
    let zz: Vec<u64> = [0i64, -1, 1, -2, 2].iter().map(|&n| zigzag(n)).collect();
    assert_eq!(zz, [0, 1, 2, 3, 4]);
    let mut rng = Rng::new(1);
    for _ in 0..10_000 {
        let n = (rng.next() >> rng.below(64)) as i64 * if rng.below(2) == 0 { -1 } else { 1 };
        let mut buf = vec![];
        put_signed(n, &mut buf);
        let mut r = Reader::new(&buf);
        assert_eq!(r.signed(), Ok(n));
        assert_eq!(r.remaining(), 0);
    }
    for n in [i64::MIN, i64::MAX, 0] {
        let mut buf = vec![];
        put_signed(n, &mut buf);
        assert_eq!(Reader::new(&buf).signed(), Ok(n));
    }
    assert_eq!(
        Reader::new(&[0xff; 11]).varint(),
        Err(CodecError::VarintOverflow)
    );
    assert_eq!(
        Reader::new(&[0x80]).varint(),
        Err(CodecError::Truncated { needed: 1, have: 0 })
    );
    // RLE round trip + malformed input
    for _ in 0..500 {
        let data = rng.bytes(300);
        assert_eq!(rle_decompress(&rle_compress(&data)).unwrap(), data);
    }
    assert!(rle_decompress(&[0xff, 0xff, 0xff, 0xff]).is_err()); // bomb header
    println!("primitives ok: CRC32C check value, zigzag, varint, RLE");
}

fn test_round_trip_and_truncation() {
    let mut rng = Rng::new(2);
    let mut compressed_seen = 0;
    for i in 0..300 {
        let owned = random_owned(&mut rng);
        let records = owned.view();
        let enc = encode_batch(&records, i % 2 == 0);
        let mut scratch = vec![];
        assert_eq!(decode_batch(&enc, &mut scratch).unwrap(), records);
        compressed_seen += (parse_batch(&enc).unwrap().attributes & ATTR_RLE) as usize;
        // every strict prefix must be rejected cleanly: never a panic, never a bogus success
        for cut in 0..enc.len() {
            assert!(
                decode_batch(&enc[..cut], &mut vec![]).is_err(),
                "prefix {cut}/{} accepted",
                enc.len()
            );
        }
    }
    assert!(
        compressed_seen > 0,
        "some batches should have used compression"
    );
    println!(
        "round trip ok on 300 random batches ({compressed_seen} compressed); every truncation rejected"
    );
}

fn test_bit_flips() {
    let mut rng = Rng::new(3);
    for i in 0..60 {
        let owned = random_owned(&mut rng);
        let enc = encode_batch(&owned.view(), i % 2 == 0);
        for byte in 8..enc.len() {
            for bit in 0..8 {
                let mut bad = enc.clone();
                bad[byte] ^= 1 << bit;
                let err = decode_batch(&bad, &mut vec![]).expect_err("flip must be detected");
                match byte {
                    12 => assert!(matches!(err, CodecError::BadMagic(_))),
                    8..12 => {} // length field: any framing error is fine
                    _ => assert!(
                        matches!(err, CodecError::CrcMismatch { .. }),
                        "byte {byte} bit {bit}: {err:?}"
                    ),
                }
            }
        }
        // base_offset is outside the CRC on purpose: the broker may rewrite it
        let mut moved = enc.clone();
        moved[..8].copy_from_slice(&999i64.to_be_bytes());
        assert!(decode_batch(&moved, &mut vec![]).is_ok());
    }
    println!(
        "every single-bit flip after the offset field is caught (CRC32C detects all single-bit errors)"
    );
}

/// corrupt the payload then FIX the CRC: now only the structural checks stand between us and a panic
fn fuzz() {
    let mut rng = Rng::new(4);
    let (mut ok, mut err) = (0, 0);
    for i in 0..20_000 {
        let owned = random_owned(&mut rng);
        let mut enc = encode_batch(&owned.view(), i % 3 == 0);
        for _ in 0..1 + rng.below(3) {
            let at = HEADER_LEN + rng.below((enc.len() - HEADER_LEN) as u64 + 1) as usize;
            if at < enc.len() {
                enc[at] = rng.next() as u8;
            }
        }
        reseal(&mut enc);
        match decode_batch(&enc, &mut vec![]) {
            Ok(_) => ok += 1,
            Err(_) => err += 1,
        }
        let junk = rng.bytes(80);
        let _ = decode_batch(&junk, &mut vec![]);
    }
    assert!(err > 0);
    println!(
        "fuzz ok: 20000 corrupted-but-resealed batches -> {ok} still valid, {err} rejected, 0 panics"
    );
}

fn test_zero_copy() {
    let (k, v) = (b"user-42".to_vec(), b"payload-bytes".to_vec());
    let rec = Record {
        offset: 7,
        timestamp: 100,
        key: Some(&k),
        value: Some(&v),
        headers: vec![(b"h", b"x")],
    };
    let enc = encode_batch(&[rec], false);
    let mut scratch = vec![];
    let out = decode_batch(&enc, &mut scratch).unwrap();
    let range = enc.as_ptr_range();
    for slice in [
        out[0].key.unwrap(),
        out[0].value.unwrap(),
        out[0].headers[0].1,
    ] {
        assert!(
            range.contains(&slice.as_ptr()),
            "slice must point into the input buffer"
        );
    }
    assert!(scratch.is_empty(), "uncompressed decode needs no scratch");
    println!("zero copy ok: key/value/header slices point inside the input buffer");
}

fn size_benchmark() {
    let mut rng = Rng::new(5);
    let n = 10_000;
    let mut ts = 1_700_000_000_000i64;
    let owned: Vec<(i64, Vec<u8>, Vec<u8>)> = (0..n)
        .map(|i| {
            ts += rng.below(4) as i64; // events arrive milliseconds apart
            let key = format!("user-{:04}", rng.below(5000)).into_bytes();
            let value =
                format!("{{\"id\":{i},\"status\":\"ok\",\"note\":\"{:<24}\"}}", "").into_bytes();
            (ts, key, value)
        })
        .collect();
    let records: Vec<Record> = owned
        .iter()
        .enumerate()
        .map(|(i, (ts, k, v))| Record {
            offset: 50_000 + i as i64,
            timestamp: *ts,
            key: Some(k),
            value: Some(v),
            headers: vec![],
        })
        .collect();
    // fixed-width: i64 offset + i64 timestamp + i32 key len + key + i32 value len + value + i32 header count
    let fixed: usize = owned
        .iter()
        .map(|(_, k, v)| 8 + 8 + 4 + k.len() + 4 + v.len() + 4)
        .sum();
    let plain = encode_batch(&records, false);
    let packed = encode_batch(&records, true);
    let pct = |x: usize| 100.0 - 100.0 * x as f64 / fixed as f64;
    println!(
        "size for {n} records: fixed-width {fixed} B | delta+varint {} B ({:.1}% smaller) | + RLE {} B ({:.1}% smaller)",
        plain.len(),
        pct(plain.len()),
        packed.len(),
        pct(packed.len())
    );
    assert!(plain.len() < fixed && packed.len() < plain.len());
    let mut scratch = vec![];
    assert_eq!(decode_batch(&packed, &mut scratch).unwrap(), records);
    assert_eq!(decode_batch(&plain, &mut vec![]).unwrap(), records);
}

fn main() {
    test_primitives();
    test_round_trip_and_truncation();
    test_bit_flips();
    test_zero_copy();
    fuzz();
    size_benchmark();
    println!("all record batch codec checks passed");
}
