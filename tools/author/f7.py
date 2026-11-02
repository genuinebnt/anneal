from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
# 1. itoa
ITOA_STARTER = """
/// Formats `n` in decimal into the **end** of `buf` and returns the text (a slice of `buf`). No allocation.
/// 20 bytes are enough for any `u64` (`18446744073709551615` has 20 digits).
pub fn fmt_u64(n: u64, buf: &mut [u8; 20]) -> &str {
    todo!()
}

/// The same for `i64`, with a leading `-` for negatives. `-9223372036854775808` has 20 characters and fits.
pub fn fmt_i64(n: i64, buf: &mut [u8; 20]) -> &str {
    todo!()
}

/// `n` with a comma between each group of three digits (`1234567` -> `"1,234,567"`). 26 bytes always suffice.
pub fn fmt_grouped(n: u64, buf: &mut [u8; 26]) -> &str {
    todo!()
}
"""

ITOA = """
pub fn fmt_u64(n: u64, buf: &mut [u8; 20]) -> &str {
    let mut n = n;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8; // fill from the right: the most significant digit ends up first
        n /= 10;
        if n == 0 {
            break; // a do-while: 0 still prints one digit
        }
    }
    std::str::from_utf8(&buf[i..]).unwrap()
}

pub fn fmt_i64(n: i64, buf: &mut [u8; 20]) -> &str {
    let neg = n < 0;
    let mut m = n.unsigned_abs(); // i64::MIN has no positive i64 counterpart, but it does fit in u64
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (m % 10) as u8;
        m /= 10;
        if m == 0 {
            break;
        }
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    std::str::from_utf8(&buf[i..]).unwrap()
}

pub fn fmt_grouped(n: u64, buf: &mut [u8; 26]) -> &str {
    let mut n = n;
    let mut i = buf.len();
    let mut digits = 0;
    loop {
        if digits > 0 && digits % 3 == 0 {
            i -= 1;
            buf[i] = b',';
        }
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        digits += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    std::str::from_utf8(&buf[i..]).unwrap()
}
"""

P.append(dict(
    slug="itoa-stack-buffer", title="Format integers into a stack buffer, no allocation", level="easy", stage="output",
    tags=["itoa", "integer formatting", "no allocation", "fixed buffer", "i64::MIN", "unsigned_abs", "digit grouping"],
    teaches=["Digits come out least-significant first, so fill the buffer from the **right** and return the slice that starts where you stopped: no reversing, no `String`.",
             "`0` has one digit, so the loop must be a do-while: `while n > 0` prints nothing for zero.",
             "`-i64::MIN` overflows, but `i64::MIN.unsigned_abs()` is exactly `2^63` as a `u64`: convert to unsigned first and the corner case disappears."],
    statement="""
        `n.to_string()` allocates. The `itoa` crate (used by `serde_json`, `tracing` and most fast formatters) writes digits into a
        caller-provided stack buffer instead. Implement three functions that each return a `&str` pointing into `buf`:

        - `fmt_u64`: decimal digits of a `u64`. A buffer of 20 bytes is always enough.
        - `fmt_i64`: the same for an `i64`, with `-` for negatives; this must work for `i64::MIN` (`-9223372036854775808`, 20 characters).
        - `fmt_grouped`: a `u64` with a comma between each group of three digits (`1234567` becomes `1,234,567`, `999` stays `999`).

        No heap allocation, no `format!`. Zero prints as `0`.
    """,
    starter=ITOA_STARTER,
    solution=ITOA,
    visible=[
        T("small_numbers", "0, 7, 10, 99, 100", "{ let mut b = [0u8; 20]; let mut out = Vec::new(); for n in [0u64, 7, 10, 99, 100] { out.push(fmt_u64(n, &mut b).to_string()); } out }", "vec![\"0\", \"7\", \"10\", \"99\", \"100\"].into_iter().map(String::from).collect::<Vec<_>>()"),
        T("the_largest_u64", "20 digits fill the buffer", "{ let mut b = [0u8; 20]; fmt_u64(u64::MAX, &mut b).to_string() }", "\"18446744073709551615\".to_string()"),
        T("negative_numbers", "a leading minus", "{ let mut b = [0u8; 20]; (fmt_i64(-1, &mut b).to_string(), fmt_i64(-4050, &mut b).to_string(), fmt_i64(42, &mut b).to_string()) }", "(\"-1\".to_string(), \"-4050\".to_string(), \"42\".to_string())"),
        T("i64_min_and_max", "the extremes", "{ let mut b = [0u8; 20]; (fmt_i64(i64::MIN, &mut b).to_string(), fmt_i64(i64::MAX, &mut b).to_string()) }", "(\"-9223372036854775808\".to_string(), \"9223372036854775807\".to_string())"),
        T("grouping", "commas every three digits", "{ let mut b = [0u8; 26]; (fmt_grouped(0, &mut b).to_string(), fmt_grouped(999, &mut b).to_string(), fmt_grouped(1000, &mut b).to_string(), fmt_grouped(1234567, &mut b).to_string()) }", "(\"0\".to_string(), \"999\".to_string(), \"1,000\".to_string(), \"1,234,567\".to_string())"),
    ],
    hidden=[
        T("powers_of_ten", "every length from 1 to 20", "{ let mut b = [0u8; 20]; let mut n = 1u64; let mut ok = true; for _ in 0..20 { ok &= fmt_u64(n, &mut b) == n.to_string(); ok &= fmt_u64(n - 1, &mut b) == (n - 1).to_string(); n = n.saturating_mul(10); } ok }", "true"),
        T("the_result_lives_in_the_buffer", "the &str points into buf at the end", "{ let mut b = [b'#'; 20]; let (p, l) = { let s = fmt_u64(305, &mut b); (s.as_ptr() as usize, s.len()) }; (l, p + l == b.as_ptr() as usize + 20, b[..17].iter().all(|&c| c == b'#')) }", "(3, true, true)"),
        T("i64_boundaries", "around zero and the extremes", "{ let mut b = [0u8; 20]; [i64::MIN, i64::MIN + 1, -10, -9, 0, 9, 10, i64::MAX - 1, i64::MAX].iter().all(|&n| fmt_i64(n, &mut b) == n.to_string()) }", "true"),
        T("minus_zero_does_not_exist", "no negative zero", "{ let mut b = [0u8; 20]; fmt_i64(0, &mut b).to_string() }", "\"0\".to_string()"),
        T("grouping_boundaries", "lengths 1 to 20 digits", "{ let mut b = [0u8; 26]; (fmt_grouped(100, &mut b).to_string(), fmt_grouped(1_000_000, &mut b).to_string(), fmt_grouped(12_345, &mut b).to_string(), fmt_grouped(u64::MAX, &mut b).to_string()) }", "(\"100\".to_string(), \"1,000,000\".to_string(), \"12,345\".to_string(), \"18,446,744,073,709,551,615\".to_string())"),
        T("negative_after_positive_in_one_buffer", "no stale minus or digits", "{ let mut b = [0u8; 20]; let a = fmt_i64(-123456789, &mut b).to_string(); let c = fmt_i64(7, &mut b).to_string(); (a, c) }", "(\"-123456789\".to_string(), \"7\".to_string())"),
        T("buffer_reuse_leaves_no_stale_digits", "short after long", "{ let mut b = [0u8; 20]; let long = fmt_u64(u64::MAX, &mut b).len(); let short = fmt_u64(5, &mut b).to_string(); (long, short) }", "(20, \"5\".to_string())"),
        """
        #[test]
        fn matches_to_string_on_random_values() {
            let mut rng = anneal_prelude::Rng::new(808);
            let mut b = [0u8; 20];
            let mut g = [0u8; 26];
            for _ in 0..20_000 {
                let bits = rng.below(64) + 1;
                let u = (rng.below(1 << 20) as u64) << 32 ^ (rng.below(1 << 20) as u64) << 16 ^ rng.below(1 << 16) as u64;
                let u = u >> (64 - bits as u32).min(63);
                let i = u as i64 * if rng.bool() { -1 } else { 1 };
                check!(format!("u64 {u}"), fmt_u64(u, &mut b).to_string(), u.to_string());
                check!(format!("i64 {i}"), fmt_i64(i, &mut b).to_string(), i.to_string());
                let digits = u.to_string();
                let mut want = String::new();
                for (k, c) in digits.chars().enumerate() {
                    if k > 0 && (digits.len() - k) % 3 == 0 {
                        want.push(',');
                    }
                    want.push(c);
                }
                check!(format!("grouped {u}"), fmt_grouped(u, &mut g).to_string(), want);
            }
        }
        """,
    ],
    wrong=dict(
        zero_prints_nothing=sub(ITOA, "        n /= 10;\n        if n == 0 {\n            break; // a do-while: 0 still prints one digit\n        }\n    }\n    std::str::from_utf8(&buf[i..]).unwrap()\n}\n\npub fn fmt_i64", "        n /= 10;\n        if n == 0 {\n            break;\n        }\n    }\n    if buf[i] == b'0' && i == buf.len() - 1 && false {\n        return \"\";\n    }\n    std::str::from_utf8(&buf[i..]).unwrap()\n}\n\npub fn fmt_i64").replace("    let mut n = n;\n    let mut i = buf.len();\n    loop {\n        i -= 1;\n        buf[i] = b'0' + (n % 10) as u8; // fill from the right: the most significant digit ends up first\n        n /= 10;\n        if n == 0 {\n            break;\n        }\n    }\n    if buf[i] == b'0' && i == buf.len() - 1 && false {\n        return \"\";\n    }", "    let mut n = n;\n    let mut i = buf.len();\n    while n > 0 {\n        i -= 1;\n        buf[i] = b'0' + (n % 10) as u8;\n        n /= 10;\n    }"),
        negation_overflows_on_min=sub(ITOA, "    let mut m = n.unsigned_abs(); // i64::MIN has no positive i64 counterpart, but it does fit in u64", "    let mut m = (if neg { -n } else { n }) as u64;"),
        forgets_the_minus_sign=sub(ITOA, "    if neg {\n        i -= 1;\n        buf[i] = b'-';\n    }\n", ""),
        groups_from_the_left=sub(ITOA, "        if digits > 0 && digits % 3 == 0 {\n            i -= 1;\n            buf[i] = b',';\n        }\n        i -= 1;\n        buf[i] = b'0' + (n % 10) as u8;\n        digits += 1;\n        n /= 10;\n        if n == 0 {\n            break;\n        }\n    }\n    std::str::from_utf8(&buf[i..]).unwrap()\n}", "        i -= 1;\n        buf[i] = b'0' + (n % 10) as u8;\n        digits += 1;\n        n /= 10;\n        if n == 0 {\n            break;\n        }\n    }\n    let plain = std::str::from_utf8(&buf[i..]).unwrap().to_string();\n    let mut j = buf.len();\n    let mut k = 0;\n    for (idx, c) in plain.bytes().enumerate() {\n        if idx > 0 && idx % 3 == 0 {\n            j -= 1;\n            buf[buf.len() - 1 - k] = b',';\n            k += 1;\n        }\n        buf[buf.len() - 1 - k] = c;\n        k += 1;\n        let _ = j;\n    }\n    i = buf.len() - k;\n    std::str::from_utf8(&buf[i..]).unwrap()\n}"),
        comma_after_the_last_group=sub(ITOA, "        if digits > 0 && digits % 3 == 0 {", "        if digits > 0 && digits % 3 == 0 && n > 0 && false {"),
    ),
    hints=[("approach", "Divide by ten repeatedly: `n % 10` is the next digit from the right, `n / 10` removes it. Write each digit at `buf[i]` after decrementing `i` from the end, and return `&buf[i..]`. A `loop` that breaks after the division when `n == 0` handles zero without a special case."),
           ("rust", "`std::str::from_utf8(&buf[i..]).unwrap()` is cheap (ASCII) and keeps the function entirely safe; `unsafe { from_utf8_unchecked }` is what `itoa` does after proving the bytes are ASCII. For signed values convert with `n.unsigned_abs()` and add the `-` last (to the left of the most significant digit)."),
           ("edge", "`i64::MIN` is why you never write `-n` on an `i64`. For grouping, count digits as you emit them from the right and insert a comma *before* the next digit whenever the count is a positive multiple of three: no comma at the front.")],
    notes=("`itoa` also processes two digits per step with a 200-byte lookup table of `\"00\"`..`\"99\"` and divides by 100 instead of 10, roughly doubling the speed, and `ryu` does the same job for floats. This is the first thing to do when profiling shows `format!` in a hot logging or serialisation path.", "O(digits)", "O(1): the caller's buffer"),
    follow_up="1) Process two digits per iteration with a lookup table and compare the speed with a benchmark. 2) Write `fmt_u128` (39 digits) and decide how big the buffer must be. 3) Add `fmt_hex(n, buf)` and `fmt_binary(n, buf)` and notice what gets simpler (shifts instead of division). 4) Write a `fmt_fixed(n_milli: i64, buf)` that prints `-12.034` from thousandths without floats. 5) Implement `std::fmt::Display` for a wrapper type using your function and check with `write!` into a `String` that it matches `to_string()`.",
    related=["S2", "F5", "F7"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. Posting list codec
PL_TYPES = """
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The ids are not in non-decreasing order.
    NotSorted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ends before the structure it describes does.
    Truncated,
    /// A block declares more than 32 bits per delta.
    BadWidth(u8),
    /// A running sum above `u32::MAX`.
    Overflow,
    /// Bytes left over after the last block.
    TrailingBytes,
}

/// Ids per block (the last block may hold fewer).
pub const BLOCK: usize = 128;
"""

PL_STARTER = PL_TYPES + """
/// Encodes a non-decreasing list of document ids.
///
/// Layout: the id count as a LEB128 varint, then one **block** per up-to-128 ids: a byte `w` (the bit width), then the
/// block's deltas packed `w` bits each, least-significant bit first, the whole block padded to a whole number of bytes.
/// A delta is `id - previous id`, where the previous id of the very first id is 0 and of a later block's first id is
/// the last id of the block before it. `w` is the number of bits of the largest delta in the block (0 if they are all 0).
pub fn encode(ids: &[u32]) -> Result<Vec<u8>, EncodeError> {
    todo!()
}

/// The inverse of `encode`.
pub fn decode(bytes: &[u8]) -> Result<Vec<u32>, DecodeError> {
    todo!()
}
"""

PL = PL_TYPES + """
fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get_varint(bytes: &[u8], at: &mut usize) -> Result<u64, DecodeError> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let b = *bytes.get(*at).ok_or(DecodeError::Truncated)?;
        *at += 1;
        v |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        if shift > 35 {
            return Err(DecodeError::Overflow);
        }
    }
}

pub fn encode(ids: &[u32]) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    put_varint(&mut out, ids.len() as u64);
    let mut prev = 0u32;
    for block in ids.chunks(BLOCK) {
        let mut deltas = Vec::with_capacity(block.len());
        for &id in block {
            if id < prev {
                return Err(EncodeError::NotSorted);
            }
            deltas.push(id - prev);
            prev = id;
        }
        let max = deltas.iter().copied().max().unwrap_or(0);
        let w = (32 - max.leading_zeros()) as usize; // bits needed for the largest delta
        out.push(w as u8);
        let mut acc: u64 = 0;
        let mut nbits = 0usize;
        for d in deltas {
            acc |= (d as u64) << nbits;
            nbits += w;
            while nbits >= 8 {
                out.push(acc as u8);
                acc >>= 8;
                nbits -= 8;
            }
        }
        if nbits > 0 {
            out.push(acc as u8); // the padded last byte
        }
    }
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<Vec<u32>, DecodeError> {
    let mut at = 0;
    let count = get_varint(bytes, &mut at)? as usize;
    let mut ids = Vec::new();
    let mut prev: u64 = 0;
    let mut remaining = count;
    while remaining > 0 {
        let n = remaining.min(BLOCK);
        let w = *bytes.get(at).ok_or(DecodeError::Truncated)?;
        at += 1;
        if w > 32 {
            return Err(DecodeError::BadWidth(w));
        }
        let w = w as usize;
        let nbytes = (n * w + 7) / 8;
        let packed = bytes.get(at..at + nbytes).ok_or(DecodeError::Truncated)?;
        at += nbytes;
        let mut acc: u64 = 0;
        let mut nbits = 0usize;
        let mut next = 0usize;
        for _ in 0..n {
            while nbits < w {
                acc |= (packed[next] as u64) << nbits;
                next += 1;
                nbits += 8;
            }
            let d = if w == 0 { 0 } else { acc & ((1u64 << w) - 1) };
            acc >>= w;
            nbits -= w;
            prev += d;
            if prev > u32::MAX as u64 {
                return Err(DecodeError::Overflow);
            }
            ids.push(prev as u32);
        }
        remaining -= n;
    }
    if at != bytes.len() {
        return Err(DecodeError::TrailingBytes);
    }
    Ok(ids)
}
"""

P.append(dict(
    slug="posting-list-codec", title="Compress a sorted id list: deltas and bit-packing", level="medium", stage="encodings",
    tags=["posting lists", "delta encoding", "bit packing", "varint", "search engines", "frame of reference", "bit streams", "tantivy"],
    teaches=["A sorted list of ids has small *gaps*: store the deltas, not the ids, and most of the 32 bits per number are wasted zeros that bit-packing removes.",
             "Bit-packing at a per-block width (frame of reference) adapts to local density: a dense stretch costs 1 bit per id, a sparse one 20, and the width byte is the whole overhead.",
             "A bit stream is an accumulator: OR the next value in at the current bit offset, flush whole bytes, keep the remainder; reading is the mirror image. Every off-by-one is in the padding."],
    statement="""
        Search engines (Lucene, tantivy, Elasticsearch) store, for every term, the sorted list of documents that contain it: a
        **posting list**. Implement the codec for one list of `u32` ids in non-decreasing order.

        Layout: the number of ids as a LEB128 varint; then one **block** for each group of up to 128 ids:
        a width byte `w`, then that block's deltas packed `w` bits each, **least-significant bit first** (delta `i` occupies bits
        `i*w .. (i+1)*w` of the block's bit stream, byte 0 holding bits 0 to 7), the block padded with zero bits to a whole number
        of bytes. A delta is `id - previous id`; the previous id of the first id is 0, and for the first id of a later block it is
        the **last id of the previous block**. `w` is the bit length of the block's largest delta (0 when all are 0, and then there
        are no payload bytes).

        `encode` fails with `NotSorted` if ids ever decrease (equal ids are allowed). `decode` fails with `Truncated` if the bytes
        run out, `BadWidth(w)` if a block declares `w > 32`, `Overflow` if the running sum passes `u32::MAX`, and `TrailingBytes` if
        bytes remain after the last block.
    """,
    starter=PL_STARTER,
    solution=PL,
    visible=[
        T("a_tiny_list", "1, 2, 3 are deltas 1, 1, 1 at one bit each", "encode(&[1, 2, 3])", "Ok(vec![3, 1, 0b111])"),
        T("a_wider_delta", "5 then a gap of 295 needs 9 bits", "encode(&[5, 300])", "Ok(vec![2, 9, 5, 0x4E, 2])"),
        T("the_empty_list", "just the count", "(encode(&[]), decode(&[0]))", "(Ok(vec![0]), Ok(vec![]))"),
        T("zero_width_blocks", "all deltas zero: no payload", "(encode(&[0, 0, 0]), decode(&[3, 0]))", "(Ok(vec![3, 0]), Ok(vec![0, 0, 0]))"),
        T("round_trip", "a few hundred ids", "{ let ids: Vec<u32> = (0..300u32).map(|i| i * i / 7 + i).collect(); decode(&encode(&ids).unwrap()) == Ok(ids) }", "true"),
        T("unsorted_input", "an error", "encode(&[3, 2])", "Err(EncodeError::NotSorted)"),
    ],
    hidden=[
        T("block_boundaries_carry_the_previous_id", "130 consecutive ids: two blocks, one bit each", "{ let ids: Vec<u32> = (1..=130).collect(); let b = encode(&ids).unwrap(); (b[..2].to_vec(), b[2], b[3..19].to_vec(), b[19], b[20], b.len()) }", "(vec![0x82, 0x01], 1, vec![0xFF; 16], 1, 3, 21)"),
        T("duplicates_are_allowed", "delta 0 inside a list", "{ let ids = vec![4u32, 4, 4, 9, 9]; decode(&encode(&ids).unwrap()) == Ok(ids) }", "true"),
        T("extremes_use_all_32_bits", "0 then u32::MAX", "{ let ids = vec![0u32, u32::MAX]; let b = encode(&ids).unwrap(); (b[1], decode(&b) == Ok(ids)) }", "(32, true)"),
        T("exact_block_sizes", "127, 128, 129 ids", "[127usize, 128, 129, 256, 257].iter().all(|&n| { let ids: Vec<u32> = (0..n as u32).map(|i| i * 3).collect(); decode(&encode(&ids).unwrap()) == Ok(ids) })", "true"),
        T("dense_lists_compress_well", "1000 consecutive ids", "{ let ids: Vec<u32> = (0..1000).collect(); let n = encode(&ids).unwrap().len(); n < 160 }", "true"),
        T("each_block_picks_its_own_width", "a sparse block then a dense one", "{ let mut ids: Vec<u32> = (0..128u32).map(|i| i * 1000).collect(); let last = *ids.last().unwrap(); ids.extend((1..=128u32).map(|i| last + i)); let b = encode(&ids).unwrap(); let w1 = b[2]; let w2 = b[2 + 1 + 128 * w1 as usize / 8 + usize::from(128 * w1 as usize % 8 != 0)]; (w1, w2) }", "(10, 1)"),
        T("truncated_inputs", "every prefix of a valid encoding fails", "{ let ids: Vec<u32> = (0..200u32).map(|i| i * 17).collect(); let b = encode(&ids).unwrap(); (0..b.len()).all(|k| decode(&b[..k]).is_err()) }", "true"),
        T("trailing_bytes", "one extra byte", "{ let mut b = encode(&[1, 2, 3]).unwrap(); b.push(0); decode(&b) }", "Err(DecodeError::TrailingBytes)"),
        T("bad_width", "w = 33", "decode(&[1, 33, 0, 0, 0, 0, 0])", "Err(DecodeError::BadWidth(33))"),
        T("overflowing_sum", "deltas add up past u32::MAX", "decode(&[2, 32, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])", "Err(DecodeError::Overflow)"),
        """
        #[test]
        fn random_sorted_lists_round_trip() {
            let mut rng = anneal_prelude::Rng::new(1234);
            for round in 0..300 {
                let n = rng.below(700);
                let mut cur = 0u64;
                let mut ids = Vec::with_capacity(n);
                let spread = 1u64 << rng.below(20);
                for _ in 0..n {
                    cur += rng.below(spread as usize + 1) as u64;
                    if cur > u32::MAX as u64 {
                        break;
                    }
                    ids.push(cur as u32);
                }
                let bytes = encode(&ids).unwrap();
                let count = ids.len();
                check!(format!("round {round}: n = {count}"), decode(&bytes), Ok(ids));
            }
        }
        """,
    ],
    wrong=dict(
        width_from_the_first_delta=sub(PL, "        let max = deltas.iter().copied().max().unwrap_or(0);", "        let max = deltas.first().copied().unwrap_or(0);"),
        most_significant_bit_first=sub(PL, "            acc |= (d as u64) << nbits;\n            nbits += w;\n            while nbits >= 8 {\n                out.push(acc as u8);\n                acc >>= 8;\n                nbits -= 8;\n            }", "            let rev = d.reverse_bits() >> (32 - w.max(1)) as u32;\n            acc |= (if w == 0 { 0 } else { rev as u64 }) << nbits;\n            nbits += w;\n            while nbits >= 8 {\n                out.push(acc as u8);\n                acc >>= 8;\n                nbits -= 8;\n            }"),
        each_block_restarts_at_zero=sub(PL, "    let mut prev = 0u32;\n    for block in ids.chunks(BLOCK) {\n        let mut deltas", "    let mut prev = 0u32;\n    for block in ids.chunks(BLOCK) {\n        prev = 0;\n        let mut deltas"),
        padding_byte_is_dropped=sub(PL, "        if nbits > 0 {\n            out.push(acc as u8); // the padded last byte\n        }\n", ""),
        accepts_unsorted=sub(PL, "            if id < prev {\n                return Err(EncodeError::NotSorted);\n            }\n            deltas.push(id - prev);", "            deltas.push(id.wrapping_sub(prev));"),
        trailing_bytes_ignored=sub(PL, "    if at != bytes.len() {\n        return Err(DecodeError::TrailingBytes);\n    }\n", ""),
        width_off_by_one=sub(PL, "let w = (32 - max.leading_zeros()) as usize; // bits needed for the largest delta", "let w = (32 - max.leading_zeros()) as usize + usize::from(max > 0 && max.is_power_of_two());"),
        width_is_never_zero=sub(PL, "let w = (32 - max.leading_zeros()) as usize; // bits needed for the largest delta", "let w = ((32 - max.leading_zeros()) as usize).max(1);"),
        overflow_is_wrapped=sub(PL, "            if prev > u32::MAX as u64 {\n                return Err(DecodeError::Overflow);\n            }\n", "            prev &= u32::MAX as u64;\n"),
    ),
    hints=[("approach", "Split into blocks of 128 with `ids.chunks(128)`. For each block compute the deltas (carrying `prev` across blocks), find the width from the largest delta (`32 - max.leading_zeros()`), then pack: keep a `u64` accumulator and a bit count; OR each delta in at the current bit offset, and whenever 8 or more bits are pending, emit a byte and shift them out. After the block, flush a final partial byte."),
           ("rust", "Reading is the mirror: `while nbits < w { acc |= (next_byte as u64) << nbits; nbits += 8 }`, then `delta = acc & ((1 << w) - 1)`, `acc >>= w`, `nbits -= w`. Special-case `w == 0` (no bytes, delta 0). The payload length is `(n * w + 7) / 8` bytes for a block of `n` ids: check it exists with `bytes.get(at..at + len)` before reading."),
           ("edge", "The first delta of the whole list is relative to 0, so ids may start anywhere. The last block has `count % 128` ids (or 128 if it divides evenly): derive it from the count, do not store it. A truncated input must be an `Err` at every prefix, never an index panic.")],
    notes=("This is the **frame-of-reference** coding used for posting lists in Lucene (`PForDelta`/`FOR`, 128-int blocks), tantivy's bitpacker and Elasticsearch: SIMD versions unpack a 128-id block in tens of nanoseconds. Real formats also store each block's maximum id so a skip list can jump over blocks without decoding them, and use `PFor` exceptions so one huge delta doesn't widen the whole block.", "O(n)", "O(n)"),
    follow_up="1) Add a skip table: for each block store its last id and byte offset, and write `advance(target)` that decodes only the block that can contain it. 2) Implement `PFor` patched coding: pick the width that fits 90% of the deltas and store the exceptions separately; measure the size win on a Zipf-like list. 3) Intersect two compressed lists without decoding fully (galloping over the skip tables). 4) Replace the byte-at-a-time loop with reading `u64`s and compare speed on a million ids. 5) Compare against plain varint deltas: when does each win in size and in speed?",
    related=["F5", "F7", "F4"],
))

STAGES = [("output", "Output", "easy"), ("encodings", "Encodings", "medium")]

if __name__ == "__main__":
    n = write_track("f7-io-serialization", "F7", "I/O & serialization", "F", "sde3", 7,
                    "Moving numbers in and out fast: allocation-free integer formatting and a delta + bit-packed posting-list codec.",
                    STAGES, P)
    print("F7", n)
