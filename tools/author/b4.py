from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
CODEC = """
/// The stream can never be decoded again: a frame announced more than the allowed size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge { len: usize, max: usize },
}

/// Appends `[len: u32 big-endian][payload]` to `out`.
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}

/// Turns arbitrary chunks of a byte stream (as `read` returns them) back into whole frames.
pub struct Decoder {
    buf: Vec<u8>,
    max: usize,
}

impl Decoder {
    pub fn new(max_frame: usize) -> Self {
        Decoder { buf: Vec::new(), max: max_frame }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// The next complete frame's payload, `Ok(None)` if more bytes are needed, or an error for an oversized length.
    /// The length is checked as soon as its four bytes are here, before any payload is buffered.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes(self.buf[..4].try_into().unwrap()) as usize;
        if len > self.max {
            return Err(FrameError::TooLarge { len, max: self.max });
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let frame = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Ok(Some(frame))
    }

    /// Bytes held that do not yet form a frame.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}
"""

CODEC_STARTER = """
/// The stream can never be decoded again: a frame announced more than the allowed size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge { len: usize, max: usize },
}

/// Appends `[len: u32 big-endian][payload]` to `out`.
pub fn encode_frame(payload: &[u8], out: &mut Vec<u8>) {
    todo!()
}

/// Turns arbitrary chunks of a byte stream (as `read` returns them) back into whole frames.
pub struct Decoder {
    // TODO: the bytes received so far, and the largest frame you accept
}

impl Decoder {
    pub fn new(max_frame: usize) -> Self {
        todo!()
    }

    pub fn feed(&mut self, data: &[u8]) {
        todo!()
    }

    /// The next complete frame's payload, `Ok(None)` if more bytes are needed, or an error for an oversized length.
    /// The length is checked as soon as its four bytes are here, before any payload is buffered.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        todo!()
    }

    /// Bytes held that do not yet form a frame.
    pub fn buffered(&self) -> usize {
        todo!()
    }
}
"""

P.append(dict(
    slug="length-prefixed-codec", title="A length-prefixed frame codec that survives partial reads", level="medium", stage="understand-it",
    tags=["framing", "TCP is a stream", "partial reads", "u32 big-endian", "DoS limits", "Decoder", "tokio_util codec"],
    teaches=["TCP delivers a byte stream, not messages: a `read` can return half a frame or three frames, so framing is your job.",
             "The decoder contract: `Ok(None)` means 'need more bytes' and must consume nothing; a frame is consumed exactly once.",
             "Validate the announced length before buffering the payload, or one 4-byte header makes the server allocate 4 GB."],
    statement="""
        Implement the framing layer used by almost every binary protocol (Redis-style, Kafka, gRPC's message prefix, `tokio_util`'s
        `LengthDelimitedCodec`): each message is `[len: u32 big-endian][payload of len bytes]`.

        `encode_frame` writes one frame. `Decoder` is fed **arbitrary chunks** with `feed` and hands out complete frames with
        `next_frame`: `Ok(Some(payload))` for the next whole frame, `Ok(None)` when it needs more bytes (and keeps what it has), and
        `Err(TooLarge)` as soon as the 4-byte header announces more than `max_frame` bytes: **before** any of that payload is
        buffered. Several frames may arrive in one chunk, and a frame may arrive one byte at a time. A zero-length frame is valid.
    """,
    starter=CODEC_STARTER,
    solution=CODEC,
    visible=[
        T("encode_layout", "3 bytes", "{ let mut out = Vec::new(); encode_frame(b\"abc\", &mut out); out }", "vec![0, 0, 0, 3, b'a', b'b', b'c']"),
        T("decode_one", "one whole frame", "{ let mut d = Decoder::new(1024); d.feed(&[0, 0, 0, 2, 9, 8]); (d.next_frame(), d.next_frame()) }", "(Ok(Some(vec![9, 8])), Ok(None))"),
        T("partial_then_rest", "header split from payload", "{ let mut d = Decoder::new(1024); d.feed(&[0, 0]); let a = d.next_frame(); d.feed(&[0, 3, 1]); let b = d.next_frame(); d.feed(&[2, 3]); (a, b, d.next_frame()) }", "(Ok(None), Ok(None), Ok(Some(vec![1, 2, 3])))"),
        T("two_frames_in_one_chunk", "pipelined", "{ let mut d = Decoder::new(1024); d.feed(&[0, 0, 0, 1, 7, 0, 0, 0, 1, 8]); (d.next_frame(), d.next_frame(), d.next_frame()) }", "(Ok(Some(vec![7])), Ok(Some(vec![8])), Ok(None))"),
        T("oversized_is_rejected_early", "header only", "{ let mut d = Decoder::new(16); d.feed(&[0, 0, 1, 0]); d.next_frame() }", "Err(FrameError::TooLarge { len: 256, max: 16 })"),
    ],
    hidden=[
        T("empty_frame", "len 0", "{ let mut d = Decoder::new(8); d.feed(&[0, 0, 0, 0, 0, 0, 0, 1, 5]); (d.next_frame(), d.next_frame(), d.next_frame()) }", "(Ok(Some(vec![])), Ok(Some(vec![5])), Ok(None))"),
        T("exactly_the_limit_is_fine", "max 4, len 4", "{ let mut d = Decoder::new(4); d.feed(&[0, 0, 0, 4, 1, 2, 3, 4]); d.next_frame() }", "Ok(Some(vec![1, 2, 3, 4]))"),
        T("one_over_the_limit", "max 4, len 5", "{ let mut d = Decoder::new(4); d.feed(&[0, 0, 0, 5]); d.next_frame() }", "Err(FrameError::TooLarge { len: 5, max: 4 })"),
        T("four_gigabyte_header_costs_nothing", "len = u32::MAX", "{ let mut d = Decoder::new(1 << 20); d.feed(&[0xFF, 0xFF, 0xFF, 0xFF]); (d.next_frame(), d.buffered()) }", "(Err(FrameError::TooLarge { len: u32::MAX as usize, max: 1 << 20 }), 4)"),
        T("error_is_sticky", "the stream cannot recover", "{ let mut d = Decoder::new(2); d.feed(&[0, 0, 0, 9, 1, 2, 3]); let a = d.next_frame(); let b = d.next_frame(); (a == b, a.is_err()) }", "(true, true)"),
        T("buffered_counts_the_leftover", "partial frame", "{ let mut d = Decoder::new(64); d.feed(&[0, 0, 0, 5, 1, 2]); let a = d.buffered(); d.next_frame().unwrap(); (a, d.buffered()) }", "(6, 6)"),
        T("buffered_is_zero_after_whole_frames", "everything consumed", "{ let mut d = Decoder::new(64); d.feed(&[0, 0, 0, 1, 1, 0, 0, 0, 2, 2, 2]); d.next_frame().unwrap(); d.next_frame().unwrap(); d.buffered() }", "0"),
        T("big_endian_on_the_wire", "len 258 = 0x0102", "{ let mut out = Vec::new(); encode_frame(&vec![0u8; 258], &mut out); out[..4].to_vec() }", "vec![0, 0, 1, 2]"),
        """
        #[test]
        fn one_byte_at_a_time() {
            let mut stream = Vec::new();
            let payloads: Vec<Vec<u8>> = vec![b"hello".to_vec(), vec![], vec![7; 300], b"x".to_vec()];
            for p in &payloads {
                encode_frame(p, &mut stream);
            }
            let mut d = Decoder::new(1000);
            let mut got = Vec::new();
            for b in stream {
                d.feed(&[b]);
                while let Some(f) = d.next_frame().unwrap() {
                    got.push(f);
                }
            }
            check!("every frame came out, in order, exactly once", got, payloads);
            check!("nothing left over", d.buffered(), 0);
        }

        #[test]
        fn random_chunking_round_trip() {
            let mut rng = anneal_prelude::Rng::new(6801);
            for _ in 0..200 {
                let payloads: Vec<Vec<u8>> = (0..rng.below(20)).map(|_| (0..rng.below(50)).map(|_| rng.below(256) as u8).collect()).collect();
                let mut stream = Vec::new();
                for p in &payloads {
                    encode_frame(p, &mut stream);
                }
                let mut d = Decoder::new(64);
                let mut got = Vec::new();
                let mut i = 0;
                while i < stream.len() {
                    let n = 1 + rng.below(17);
                    let end = (i + n).min(stream.len());
                    d.feed(&stream[i..end]);
                    i = end;
                    while let Some(f) = d.next_frame().unwrap() {
                        got.push(f);
                    }
                }
                check!("round trip through random read sizes", got, payloads);
            }
        }
        """,
    ],
    wrong=dict(
        little_endian=sub(sub(CODEC, "out.extend_from_slice(&(payload.len() as u32).to_be_bytes());", "out.extend_from_slice(&(payload.len() as u32).to_le_bytes());"), "u32::from_be_bytes(self.buf[..4].try_into().unwrap())", "u32::from_le_bytes(self.buf[..4].try_into().unwrap())"),
        no_size_limit=sub(CODEC, "        if len > self.max {\n            return Err(FrameError::TooLarge { len, max: self.max });\n        }\n", ""),
        limit_is_exclusive=sub(CODEC, "if len > self.max {", "if len >= self.max {"),
        discards_the_rest_of_the_chunk=sub(CODEC, "self.buf.drain(..4 + len);", "self.buf.clear();"),
        zero_length_means_wait=sub(CODEC, "        if self.buf.len() < 4 + len {\n            return Ok(None);\n        }", "        if len == 0 || self.buf.len() < 4 + len {\n            return Ok(None);\n        }"),
    ),
    hints=[("approach", "Keep a `Vec<u8>` of everything received but not yet consumed. `next_frame`: need 4 bytes for the header, else `Ok(None)`; read the length; reject it if above the limit; need `4 + len` bytes in total, else `Ok(None)`; copy the payload out and remove the whole frame from the front."),
           ("rust", "`u32::from_be_bytes(buf[..4].try_into().unwrap())` reads the header; `u32::to_be_bytes` writes it. `Vec::drain(..n)` removes the consumed prefix (a `BytesMut`/`VecDeque` would avoid the memmove: a follow-up)."),
           ("edge", "Check the limit right after the header is readable, **before** waiting for the payload: otherwise a peer that sends `FF FF FF FF` makes you wait for (and buffer) gigabytes. Return `Ok(None)` without touching the buffer when more bytes are needed.")],
    notes=("`tokio_util::codec::LengthDelimitedCodec` is this plus configurable field width, endianness and offset; Kafka, Postgres (`[tag][len]`), Redis RESP3 bulk strings and gRPC (`[flag][len]`) all frame this way. The two bugs people ship are treating one `read` as one message, and trusting the length field: both are CVE classes.", "O(frame) per frame", "O(max_frame)"),
    follow_up="1) Replace `drain(..n)` with a read cursor and compact lazily: what is the cost model? 2) Add a `u16` or varint length option (reuse the F5 varint). 3) Make the decoder generic over `Read` and write a blocking `read_frame(&mut impl Read)` using `read_exact`; why is `read_exact` wrong for a nonblocking socket? 4) What should the server do after `TooLarge`: why close the connection rather than skip the bytes? 5) Add a checksum (CRC32) trailer and decide what a corrupt frame does to the stream.",
    related=["S9", "F7", "F5"],
))

# ---------------------------------------------------------------------------------------------------------------
REASM = """
use std::collections::BTreeMap;

/// Rebuilds an in-order byte stream from TCP-like segments that may arrive late, twice, overlapping, or not at all.
/// Stream offset 0 has sequence number `isn`; sequence numbers are 32-bit and wrap around.
pub struct Reassembler {
    isn: u32,
    window: usize,
    next: u64,                       // stream offset of the next byte to hand out
    pending: BTreeMap<u64, Vec<u8>>, // disjoint chunks at offsets >= next, waiting for a gap to be filled
    ready: Vec<u8>,                  // contiguous bytes not yet read
    fin_at: Option<u64>,
}

impl Reassembler {
    pub fn new(isn: u32, window: usize) -> Self {
        Reassembler { isn, window, next: 0, pending: BTreeMap::new(), ready: Vec::new(), fin_at: None }
    }

    /// The 64-bit stream offset whose low 32 bits match `seq` and which is closest to `next`.
    fn unwrap(&self, seq: u32) -> u64 {
        let diff = seq.wrapping_sub(self.isn) as u64;
        let base = self.next & !0xFFFF_FFFF;
        let cand = base + diff;
        if cand >= (1 << 32) && cand >= self.next + (1 << 31) {
            cand - (1 << 32)
        } else if cand + (1 << 31) < self.next {
            cand + (1 << 32)
        } else {
            cand
        }
    }

    /// A segment starting at sequence number `seq`. `fin` marks the end of the stream (just after this segment's data).
    /// Bytes outside `[next, next + window)` are dropped (the sender retransmits); where segments overlap, the bytes
    /// that arrived first win.
    pub fn segment(&mut self, seq: u32, data: &[u8], fin: bool) {
        let offset = self.unwrap(seq);
        let limit = self.next + self.window as u64;
        let end = offset + data.len() as u64;
        if fin && end <= limit {
            self.fin_at = Some(end);
        }
        let start = offset.max(self.next);
        let end = end.min(limit);
        if start < end {
            self.store(start, &data[(start - offset) as usize..(end - offset) as usize]);
        }
        // Hand over everything that is now contiguous.
        while let Some(entry) = self.pending.first_entry() {
            if *entry.key() != self.next {
                break;
            }
            let chunk = entry.remove();
            self.next += chunk.len() as u64;
            self.ready.extend_from_slice(&chunk);
        }
    }

    /// Stores `[start, start + data.len())`, keeping only the parts no earlier chunk covers.
    fn store(&mut self, start: u64, data: &[u8]) {
        let end = start + data.len() as u64;
        let mut cursor = start;
        let mut new_chunks: Vec<(u64, Vec<u8>)> = Vec::new();
        for (&s, chunk) in self.pending.range(..end) {
            let e = s + chunk.len() as u64;
            if e <= cursor {
                continue;
            }
            if s > cursor {
                new_chunks.push((cursor, data[(cursor - start) as usize..(s - start) as usize].to_vec()));
            }
            cursor = cursor.max(e);
        }
        if cursor < end {
            new_chunks.push((cursor, data[(cursor - start) as usize..].to_vec()));
        }
        for (s, c) in new_chunks {
            self.pending.insert(s, c);
        }
    }

    /// Takes every byte that is in order and not yet read.
    pub fn read(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.ready)
    }

    /// True once the FIN has been seen and every byte before it has been handed out (by `segment`, to be `read`).
    pub fn is_finished(&self) -> bool {
        self.fin_at == Some(self.next)
    }

    /// Bytes held out of order, waiting for an earlier gap to fill.
    pub fn buffered(&self) -> usize {
        self.pending.values().map(|c| c.len()).sum()
    }
}
"""

REASM_STARTER = """
/// Rebuilds an in-order byte stream from TCP-like segments that may arrive late, twice, overlapping, or not at all.
/// Stream offset 0 has sequence number `isn`; sequence numbers are 32-bit and wrap around.
pub struct Reassembler {
    // TODO: next offset to deliver, out-of-order chunks keyed by offset, bytes ready to read, the FIN position
}

impl Reassembler {
    /// `window`: how many bytes ahead of the next expected one you are willing to buffer.
    pub fn new(isn: u32, window: usize) -> Self {
        todo!()
    }

    /// A segment starting at sequence number `seq`. `fin` marks the end of the stream (just after this segment's data).
    /// Bytes outside `[next, next + window)` are dropped (the sender retransmits); where segments overlap, the bytes
    /// that arrived first win.
    pub fn segment(&mut self, seq: u32, data: &[u8], fin: bool) {
        todo!()
    }

    /// Takes every byte that is in order and not yet read.
    pub fn read(&mut self) -> Vec<u8> {
        todo!()
    }

    /// True once the FIN has been seen and every byte before it has been handed out.
    pub fn is_finished(&self) -> bool {
        todo!()
    }

    /// Bytes held out of order, waiting for an earlier gap to fill.
    pub fn buffered(&self) -> usize {
        todo!()
    }
}
"""

P.append(dict(
    slug="tcp-reassembler", title="Reassemble a TCP byte stream from out-of-order segments", level="hard", stage="build-it",
    tags=["TCP", "sequence numbers", "wraparound", "BTreeMap", "reassembly", "flow-control window", "CS144"],
    teaches=["Why a transport protocol needs sequence numbers: segments are reordered, duplicated and overlap, and the receiver must still emit each byte once, in order.",
             "Unwrapping a 32-bit wrapping counter into a 64-bit stream offset relative to a checkpoint (the trick used for TCP seq numbers and QUIC packet numbers).",
             "A receive window as a memory bound: bytes beyond it are dropped, so a malicious sender cannot make you buffer without limit."],
    statement="""
        Implement the receive side of a TCP-like stream. `Reassembler::new(isn, window)`: the stream's first byte has sequence number
        `isn` (offset 0); sequence numbers are `u32` and **wrap around**, so a stream crosses `u32::MAX` back to 0.

        `segment(seq, data, fin)` may be called with segments in any order, with duplicates, and with overlaps. Rules:
        * Only bytes at stream offsets `[next, next + window)` are kept, where `next` is the first byte not yet handed out. Earlier bytes
          are duplicates; later ones are dropped (a real sender would retransmit them). A `fin` whose end lies beyond the window is ignored.
        * Where segments overlap, the **first-arrived** bytes win.
        * `read()` returns the newly contiguous bytes, each exactly once. `is_finished()` is true once a `fin` was accepted and everything before it
          was handed out. `buffered()` counts out-of-order bytes held.
    """,
    starter=REASM_STARTER,
    solution=REASM,
    visible=[
        T("in_order", "two segments", "{ let mut r = Reassembler::new(1000, 100); r.segment(1000, b\"hel\", false); let a = r.read(); r.segment(1003, b\"lo\", false); (a, r.read()) }", "(b\"hel\".to_vec(), b\"lo\".to_vec())"),
        T("out_of_order_waits", "the gap is filled later", "{ let mut r = Reassembler::new(0, 100); r.segment(3, b\"def\", false); let a = r.read(); let held = r.buffered(); r.segment(0, b\"abc\", false); (a, held, r.read(), r.buffered()) }", "(vec![], 3, b\"abcdef\".to_vec(), 0)"),
        T("duplicate_is_ignored", "same segment twice", "{ let mut r = Reassembler::new(0, 100); r.segment(0, b\"abc\", false); r.segment(0, b\"abc\", false); (r.read(), r.buffered()) }", "(b\"abc\".to_vec(), 0)"),
        T("overlap_extends", "a retransmit that carries new bytes", "{ let mut r = Reassembler::new(0, 100); r.segment(0, b\"abcd\", false); r.segment(2, b\"cdef\", false); r.read() }", "b\"abcdef\".to_vec()"),
        T("fin_ends_the_stream", "finished only when all bytes arrived", "{ let mut r = Reassembler::new(0, 100); r.segment(3, b\"def\", true); let early = r.is_finished(); r.segment(0, b\"abc\", false); (early, r.is_finished(), r.read()) }", "(false, true, b\"abcdef\".to_vec())"),
    ],
    hidden=[
        T("sequence_numbers_wrap", "isn near u32::MAX", "{ let isn = u32::MAX - 2; let mut r = Reassembler::new(isn, 100); r.segment(isn, b\"abc\", false); r.segment(isn.wrapping_add(3), b\"def\", false); r.segment(isn.wrapping_add(6), b\"g\", true); (r.read(), r.is_finished()) }", "(b\"abcdefg\".to_vec(), true)"),
        T("wrapped_segment_arrives_before_the_gap_fills", "later segment past the wrap, earlier one late", "{ let isn = u32::MAX - 1; let mut r = Reassembler::new(isn, 100); r.segment(isn.wrapping_add(4), b\"ef\", false); r.segment(isn, b\"abcd\", false); r.read() }", "b\"abcdef\".to_vec()"),
        T("first_arrival_wins_on_conflict", "overlapping segments disagree", "{ let mut r = Reassembler::new(0, 100); r.segment(2, b\"XXXX\", false); r.segment(0, b\"abcdef\", false); r.read() }", "b\"abXXXX\".to_vec()"),
        T("old_data_is_ignored", "a stale retransmit after delivery", "{ let mut r = Reassembler::new(0, 100); r.segment(0, b\"abcdef\", false); r.read(); r.segment(1, b\"bcd\", false); (r.read(), r.buffered()) }", "(vec![], 0)"),
        T("bytes_beyond_the_window_are_dropped", "window 4", "{ let mut r = Reassembler::new(0, 4); r.segment(2, b\"cdefgh\", false); let held = r.buffered(); r.segment(0, b\"ab\", false); (held, r.read(), r.buffered()) }", "(2, b\"abcd\".to_vec(), 0)"),
        T("window_slides_as_bytes_are_delivered", "window 4 again", "{ let mut r = Reassembler::new(0, 4); r.segment(0, b\"abcd\", false); r.segment(4, b\"efgh\", false); (r.read(), r.buffered()) }", "(b\"abcdefgh\".to_vec(), 0)"),
        T("fin_beyond_the_window_is_ignored", "the sender will retransmit it", "{ let mut r = Reassembler::new(0, 3); r.segment(0, b\"abcdef\", true); let a = (r.read(), r.is_finished()); r.segment(3, b\"def\", true); (a, r.read(), r.is_finished()) }", "((b\"abc\".to_vec(), false), b\"def\".to_vec(), true)"),
        T("fin_with_no_data", "an empty final segment", "{ let mut r = Reassembler::new(5, 10); r.segment(5, b\"ab\", false); r.segment(7, b\"\", true); (r.read(), r.is_finished()) }", "(b\"ab\".to_vec(), true)"),
        T("empty_stream", "fin at offset 0", "{ let mut r = Reassembler::new(9, 10); r.segment(9, b\"\", true); (r.read(), r.is_finished()) }", "(vec![], true)"),
        """
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
        """,
    ],
    wrong=dict(
        forgets_the_sequence_space_wraps=sub(REASM, "        let diff = seq.wrapping_sub(self.isn) as u64;\n        let base = self.next & !0xFFFF_FFFF;\n        let cand = base + diff;\n        if cand >= (1 << 32) && cand >= self.next + (1 << 31) {\n            cand - (1 << 32)\n        } else if cand + (1 << 31) < self.next {\n            cand + (1 << 32)\n        } else {\n            cand\n        }", "        (seq as u64).saturating_sub(self.isn as u64)"),
        ignores_the_window=sub(REASM, "        let end = end.min(limit);\n", ""),
        newest_overlap_wins=sub(REASM, "            if e <= cursor {\n                continue;\n            }\n            if s > cursor {\n                new_chunks.push((cursor, data[(cursor - start) as usize..(s - start) as usize].to_vec()));\n            }\n            cursor = cursor.max(e);\n        }\n        if cursor < end {\n            new_chunks.push((cursor, data[(cursor - start) as usize..].to_vec()));\n        }", "            let _ = (s, e, cursor);\n        }\n        new_chunks.push((start, data.to_vec()));"),
        finished_on_fin_alone=sub(REASM, "self.fin_at == Some(self.next)", "self.fin_at.is_some()"),
        keeps_already_delivered_bytes=sub(REASM, "let start = offset.max(self.next);", "let start = offset;"),
    ),
    hints=[("approach", "Keep `next` (the stream offset of the first undelivered byte) and a `BTreeMap<offset, Vec<u8>>` of disjoint out-of-order chunks. On each segment: unwrap `seq` to an offset, clip to `[next, next + window)`, insert only the parts no stored chunk already covers, then pop chunks while the first one starts exactly at `next`."),
           ("rust", "Unwrap: `diff = seq.wrapping_sub(isn) as u64`; the answer is the value `base + diff` (with `base` = `next` rounded down to a multiple of 2^32), moved by \u00b12^32 if that lands more than 2^31 from `next`. `BTreeMap::range(..end)` walks chunks that start before the new one ends; `first_entry()` / `remove()` pops the front."),
           ("edge", "Compute the clipped range in 64-bit offsets before slicing `data` (a segment can start before `next`). Only accept a FIN whose end offset is inside the window. `is_finished` must compare `next` with the FIN offset: a FIN that arrives first (out of order) says nothing until the gap fills.")],
    notes=("This is the CS144 'TCP receiver' lab and the core of any transport: TCP, QUIC streams (offset-based `CRYPTO`/`STREAM` frames) and SCTP all reassemble this way, and the first-writer-wins overlap policy is why different OSes' stacks can be fingerprinted and why IDS evasions through overlapping segments exist. The window is the receiver's memory bound and the basis of flow control.", "O(log n) per segment", "O(window)"),
    follow_up="1) Merge adjacent chunks in `pending` so the map stays small under heavy reordering, and measure the effect. 2) Add `ack()` returning the 32-bit cumulative acknowledgment number (`isn + next`) and a `window_size()` for the sender: what does a zero window do to the sender? 3) Replace first-wins with last-wins and show an inconsistent-retransmit attack on an intrusion detector that picks the other policy. 4) Add selective acknowledgment blocks (SACK) from `pending`. 5) How do QUIC's per-stream offsets (62-bit, no wraparound) simplify this compared with TCP's 32-bit sequence space?",
    related=["B5", "C2", "F4"],
))

# ---------------------------------------------------------------------------------------------------------------
HTTP = """
/// A parsed HTTP/1.x request. `minor` is 0 or 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub minor: u8,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first header with this name, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_head: usize,    // request line + headers + the blank line
    pub max_headers: usize, // number of header lines
    pub max_body: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BadRequestLine,
    BadVersion,
    BadHeader,
    HeadTooLarge,
    TooManyHeaders,
    BodyTooLarge,
    BadContentLength,
    BadChunk,
    ConflictingLength,
}

/// Incremental HTTP/1.1 request parser: feed bytes as they arrive, take requests as they complete.
pub struct Parser {
    buf: Vec<u8>,
    limits: Limits,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

impl Parser {
    pub fn new(limits: Limits) -> Self {
        Parser { buf: Vec::new(), limits }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Bytes held that do not yet form a complete request.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// The next complete request, `Ok(None)` if more bytes are needed (nothing is consumed), or the reason the stream is bad.
    pub fn next_request(&mut self) -> Result<Option<Request>, ParseError> {
        let Some(head_end) = find(&self.buf, b"\\r\\n\\r\\n") else {
            if self.buf.len() > self.limits.max_head {
                return Err(ParseError::HeadTooLarge); // never wait for a terminator that is not coming
            }
            return Ok(None);
        };
        let head_len = head_end + 4;
        if head_len > self.limits.max_head {
            return Err(ParseError::HeadTooLarge);
        }
        let head = std::str::from_utf8(&self.buf[..head_end]).map_err(|_| ParseError::BadHeader)?;
        let mut lines = head.split("\\r\\n");

        // request line: METHOD SP target SP HTTP/1.x
        let line = lines.next().unwrap_or("");
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() != 3 || parts[0].is_empty() || !parts[0].bytes().all(|b| b.is_ascii_uppercase()) || parts[1].is_empty() || parts[1].bytes().any(|b| b.is_ascii_control()) {
            return Err(ParseError::BadRequestLine);
        }
        let minor = match parts[2] {
            "HTTP/1.1" => 1,
            "HTTP/1.0" => 0,
            v if v.starts_with("HTTP/") => return Err(ParseError::BadVersion),
            _ => return Err(ParseError::BadRequestLine),
        };

        // headers
        let mut headers = Vec::new();
        for l in lines {
            let (name, value) = l.split_once(':').ok_or(ParseError::BadHeader)?;
            if name.is_empty() || !name.bytes().all(is_token_char) {
                return Err(ParseError::BadHeader); // includes a space before the colon and obsolete line folding
            }
            headers.push((name.to_string(), value.trim_matches(|c| c == ' ' || c == '\\t').to_string()));
            if headers.len() > self.limits.max_headers {
                return Err(ParseError::TooManyHeaders);
            }
        }

        // body framing: Content-Length xor chunked
        let mut content_length: Option<usize> = None;
        let mut chunked = false;
        for (n, v) in &headers {
            if n.eq_ignore_ascii_case("content-length") {
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ParseError::BadContentLength);
                }
                let len: usize = v.parse().map_err(|_| ParseError::BadContentLength)?;
                match content_length {
                    Some(prev) if prev != len => return Err(ParseError::ConflictingLength),
                    _ => content_length = Some(len),
                }
            } else if n.eq_ignore_ascii_case("transfer-encoding") {
                if !v.eq_ignore_ascii_case("chunked") {
                    return Err(ParseError::BadHeader);
                }
                chunked = true;
            }
        }
        if chunked && content_length.is_some() {
            return Err(ParseError::ConflictingLength); // the request-smuggling ambiguity: refuse it
        }
        let (method, target) = (parts[0].to_string(), parts[1].to_string());
        let (body, consumed) = if chunked {
            match parse_chunked(&self.buf[head_len..], self.limits.max_body)? {
                Some((body, used)) => (body, head_len + used),
                None => return Ok(None),
            }
        } else if let Some(len) = content_length {
            if len > self.limits.max_body {
                return Err(ParseError::BodyTooLarge); // decided from the header, before the body arrives
            }
            if self.buf.len() < head_len + len {
                return Ok(None);
            }
            (self.buf[head_len..head_len + len].to_vec(), head_len + len)
        } else {
            (Vec::new(), head_len)
        };
        self.buf.drain(..consumed);
        Ok(Some(Request { method, target, minor, headers, body }))
    }
}

/// Decodes a chunked body from the start of `buf`: returns the body and how many bytes it used, or `None` if incomplete.
fn parse_chunked(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ParseError> {
    let mut body = Vec::new();
    let mut pos = 0;
    loop {
        let Some(eol) = find(&buf[pos..], b"\\r\\n") else {
            return if buf.len() - pos > 64 { Err(ParseError::BadChunk) } else { Ok(None) };
        };
        let line = std::str::from_utf8(&buf[pos..pos + eol]).map_err(|_| ParseError::BadChunk)?;
        let size_str = line.split(';').next().unwrap_or(""); // drop chunk extensions
        if size_str.is_empty() || !size_str.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseError::BadChunk);
        }
        let size = usize::from_str_radix(size_str, 16).map_err(|_| ParseError::BadChunk)?;
        pos += eol + 2;
        if size == 0 {
            // trailer lines until an empty line
            loop {
                let Some(eol) = find(&buf[pos..], b"\\r\\n") else { return Ok(None) };
                let empty = eol == 0;
                pos += eol + 2;
                if empty {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if body.len().checked_add(size).map_or(true, |t| t > max_body) {
            return Err(ParseError::BodyTooLarge); // from the announced size, before the data arrives
        }
        if buf.len() < pos + size + 2 {
            return Ok(None);
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        if &buf[pos + size..pos + size + 2] != b"\\r\\n" {
            return Err(ParseError::BadChunk);
        }
        pos += size + 2;
    }
}
"""

HTTP_STARTER = """
/// A parsed HTTP/1.x request. `minor` is 0 or 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub minor: u8,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first header with this name, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_head: usize,    // request line + headers + the blank line
    pub max_headers: usize, // number of header lines
    pub max_body: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BadRequestLine,
    BadVersion,
    BadHeader,
    HeadTooLarge,
    TooManyHeaders,
    BodyTooLarge,
    BadContentLength,
    BadChunk,
    ConflictingLength,
}

/// Incremental HTTP/1.1 request parser: feed bytes as they arrive, take requests as they complete.
pub struct Parser {
    // TODO: the bytes received so far and the limits
}

impl Parser {
    pub fn new(limits: Limits) -> Self {
        todo!()
    }

    pub fn feed(&mut self, data: &[u8]) {
        todo!()
    }

    /// Bytes held that do not yet form a complete request.
    pub fn buffered(&self) -> usize {
        todo!()
    }

    /// The next complete request, `Ok(None)` if more bytes are needed (nothing is consumed), or the reason the stream is bad.
    pub fn next_request(&mut self) -> Result<Option<Request>, ParseError> {
        todo!()
    }
}
"""

HTTP_HELPERS = """
fn lim() -> Limits {
    Limits { max_head: 8192, max_headers: 32, max_body: 1 << 20 }
}

fn one(bytes: &[u8]) -> Result<Option<Request>, ParseError> {
    let mut p = Parser::new(lim());
    p.feed(bytes);
    p.next_request()
}

fn all(chunks: &[&[u8]], limits: Limits) -> Result<Vec<Request>, ParseError> {
    let mut p = Parser::new(limits);
    let mut out = Vec::new();
    for c in chunks {
        p.feed(c);
        while let Some(r) = p.next_request()? {
            out.push(r);
        }
    }
    Ok(out)
}
"""

P.append(dict(
    slug="http1-request-parser", title="A strict, incremental HTTP/1.1 request parser", level="hard", stage="build-it",
    tags=["HTTP/1.1", "parser", "Content-Length", "chunked", "request smuggling", "limits", "incremental", "pipelining"],
    teaches=["How a request is framed on the wire: request line, headers, blank line, then a body delimited by `Content-Length` or `chunked`.",
             "Why strictness is a security feature: a space before the colon, two `Content-Length`s, or both framing headers are how request smuggling works.",
             "Limits that fire early (head size, header count, body size from the announced length), so a slow or hostile peer cannot make the server buffer without bound."],
    statement="""
        Implement the request-parsing half of an HTTP/1.1 server, as an incremental parser: `feed` bytes as they arrive from the socket,
        and `next_request` returns `Ok(Some(request))` for each complete request (several may be pipelined in the bytes), `Ok(None)` when
        it needs more bytes (consuming nothing), or `Err` when the stream is invalid.

        Parse `METHOD SP target SP HTTP/1.0|1.1` (method uppercase letters, exactly one space between parts), then header lines
        `name: value` (name is a token with **no whitespace before the colon**; value is trimmed of spaces and tabs; no obsolete line folding), then
        `CRLF`. Body framing: `Content-Length: <digits>` (digits only, no sign), or `Transfer-Encoding: chunked` (hex sizes, optional `;extension`,
        trailers skipped, final `0` chunk). **Reject** a request that has both framing headers, or two `Content-Length`s with different values.

        Enforce `Limits` early: the head (request line plus headers plus blank line) may not exceed `max_head`, even while still incomplete; at most
        `max_headers` header lines; a body larger than `max_body` is `BodyTooLarge` as soon as its size is announced, not after it arrives.
    """,
    starter=HTTP_STARTER,
    solution=HTTP,
    visible=[
        HTTP_HELPERS + """
        #[test]
        fn a_simple_get() {
            let r = one(b"GET /index.html?x=1 HTTP/1.1\\r\\nHost: example.com\\r\\nAccept: */*\\r\\n\\r\\n").unwrap().unwrap();
            check!("method, target, version", (r.method.as_str(), r.target.as_str(), r.minor), ("GET", "/index.html?x=1", 1));
            check!("headers in order", r.headers, vec![("Host".to_string(), "example.com".to_string()), ("Accept".to_string(), "*/*".to_string())]);
            check!("no body", r.body.len(), 0);
        }

        #[test]
        fn a_post_with_content_length() {
            let r = one(b"POST /submit HTTP/1.1\\r\\nContent-Length: 5\\r\\n\\r\\nhello").unwrap().unwrap();
            check!("the body", r.body, b"hello".to_vec());
        }

        #[test]
        fn header_lookup_ignores_case_and_trims_values() {
            let r = one(b"GET / HTTP/1.0\\r\\nhOsT:   spaced.example \\t\\r\\n\\r\\n").unwrap().unwrap();
            check!("case-insensitive, trimmed", (r.header("HOST"), r.header("host"), r.header("nope"), r.minor), (Some("spaced.example"), Some("spaced.example"), None, 0));
        }

        #[test]
        fn a_chunked_body() {
            let r = one(b"POST /up HTTP/1.1\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\n5\\r\\nhello\\r\\n6;ext=1\\r\\n world\\r\\n0\\r\\nTrailer: x\\r\\n\\r\\n").unwrap().unwrap();
            check!("chunks joined", r.body, b"hello world".to_vec());
        }

        #[test]
        fn incomplete_input_waits_and_pipelined_requests_come_out_in_order() {
            let mut p = Parser::new(lim());
            p.feed(b"GET /a HTTP/1.1\\r\\nHost: x\\r\\n");
            check!("no blank line yet", p.next_request(), Ok(None));
            p.feed(b"\\r\\nGET /b HTTP/1.1\\r\\n\\r\\nGET /c HT");
            let a = p.next_request().unwrap().unwrap();
            let b = p.next_request().unwrap().unwrap();
            check!("two complete, the third partial", (a.target.as_str(), b.target.as_str(), p.next_request()), ("/a", "/b", Ok(None)));
        }
        """,
    ],
    hidden=[
        HTTP_HELPERS + """
        #[test]
        fn bad_request_lines() {
            for bad in [&b"GET  / HTTP/1.1\\r\\n\\r\\n"[..], b"GET /\\r\\n\\r\\n", b"get / HTTP/1.1\\r\\n\\r\\n", b"GET / HTTP/1.1 extra\\r\\n\\r\\n", b"GET / FTP/1.1\\r\\n\\r\\n", b" / HTTP/1.1\\r\\n\\r\\n"] {
                check!("rejected request line", one(bad), Err(ParseError::BadRequestLine));
            }
        }

        #[test]
        fn bad_versions() {
            check!("HTTP/2.0 is not HTTP/1.x", one(b"GET / HTTP/2.0\\r\\n\\r\\n"), Err(ParseError::BadVersion));
            check!("HTTP/1.2 neither", one(b"GET / HTTP/1.2\\r\\n\\r\\n"), Err(ParseError::BadVersion));
        }

        #[test]
        fn header_syntax_is_strict() {
            check!("space before the colon (a smuggling vector)", one(b"GET / HTTP/1.1\\r\\nContent-Length : 5\\r\\n\\r\\n"), Err(ParseError::BadHeader));
            check!("obsolete line folding", one(b"GET / HTTP/1.1\\r\\nA: b\\r\\n c\\r\\n\\r\\n"), Err(ParseError::BadHeader));
            check!("no colon", one(b"GET / HTTP/1.1\\r\\nNoColonHere\\r\\n\\r\\n"), Err(ParseError::BadHeader));
            check!("empty name", one(b"GET / HTTP/1.1\\r\\n: v\\r\\n\\r\\n"), Err(ParseError::BadHeader));
        }

        #[test]
        fn both_framing_headers_are_refused() {
            check!("Content-Length and chunked together", one(b"POST / HTTP/1.1\\r\\nContent-Length: 4\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\n0\\r\\n\\r\\n"), Err(ParseError::ConflictingLength));
        }

        #[test]
        fn duplicate_content_length() {
            check!("different values", one(b"POST / HTTP/1.1\\r\\nContent-Length: 4\\r\\nContent-Length: 5\\r\\n\\r\\nhello"), Err(ParseError::ConflictingLength));
            let r = one(b"POST / HTTP/1.1\\r\\nContent-Length: 2\\r\\nContent-Length: 2\\r\\n\\r\\nok").unwrap().unwrap();
            check!("identical values are fine", r.body, b"ok".to_vec());
        }

        #[test]
        fn content_length_is_digits_only() {
            for bad in ["+5", "-1", "0x10", "5 5", "", "99999999999999999999999"] {
                let req = format!("POST / HTTP/1.1\\r\\nContent-Length: {bad}\\r\\n\\r\\nhello");
                check!("rejected length", one(req.as_bytes()), Err(ParseError::BadContentLength));
            }
        }

        #[test]
        fn head_limit_applies_before_the_blank_line_arrives() {
            let mut p = Parser::new(Limits { max_head: 64, max_headers: 100, max_body: 100 });
            p.feed(b"GET / HTTP/1.1\\r\\n");
            check!("under the limit: wait", p.next_request(), Ok(None));
            p.feed(&[b'a'; 100]); // a header line that never ends
            check!("over the limit with no terminator", p.next_request(), Err(ParseError::HeadTooLarge));
        }

        #[test]
        fn head_limit_counts_a_complete_head_too() {
            let mut req = b"GET / HTTP/1.1\\r\\nX: ".to_vec();
            req.extend(vec![b'v'; 200]);
            req.extend(b"\\r\\n\\r\\n");
            let mut p = Parser::new(Limits { max_head: 100, max_headers: 100, max_body: 100 });
            p.feed(&req);
            check!("a complete but oversized head", p.next_request(), Err(ParseError::HeadTooLarge));
        }

        #[test]
        fn too_many_headers() {
            let mut req = b"GET / HTTP/1.1\\r\\n".to_vec();
            for i in 0..5 {
                req.extend(format!("H{i}: v\\r\\n").as_bytes());
            }
            req.extend(b"\\r\\n");
            let mut p = Parser::new(Limits { max_head: 8192, max_headers: 4, max_body: 100 });
            p.feed(&req);
            check!("five headers with a limit of four", p.next_request(), Err(ParseError::TooManyHeaders));
        }

        #[test]
        fn body_limit_fires_from_the_announced_length() {
            let mut p = Parser::new(Limits { max_head: 8192, max_headers: 8, max_body: 10 });
            p.feed(b"POST / HTTP/1.1\\r\\nContent-Length: 1000000000\\r\\n\\r\\n");
            check!("no body bytes have arrived yet", p.next_request(), Err(ParseError::BodyTooLarge));
            let mut q = Parser::new(Limits { max_head: 8192, max_headers: 8, max_body: 10 });
            q.feed(b"POST / HTTP/1.1\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\nFFFFFFF\\r\\n");
            check!("a chunk announcing too much", q.next_request(), Err(ParseError::BodyTooLarge));
        }

        #[test]
        fn bad_chunks() {
            let head = b"POST / HTTP/1.1\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\n";
            for tail in [&b"zz\\r\\nab\\r\\n0\\r\\n\\r\\n"[..], b"2\\r\\nabc\\r\\n0\\r\\n\\r\\n", b"-1\\r\\n", b"\\r\\n", b"2;ok\\r\\nabXX0\\r\\n\\r\\n"] {
                let mut req = head.to_vec();
                req.extend(tail);
                check!("rejected chunk framing", one(&req), Err(ParseError::BadChunk));
            }
        }

        #[test]
        fn an_incomplete_body_consumes_nothing() {
            let mut p = Parser::new(lim());
            p.feed(b"POST / HTTP/1.1\\r\\nContent-Length: 10\\r\\n\\r\\nabc");
            let n = p.buffered();
            check!("wait for the rest", p.next_request(), Ok(None));
            check!("nothing consumed", p.buffered(), n);
            p.feed(b"defghij");
            let r = p.next_request().unwrap().unwrap();
            check!("whole body, buffer empty", (r.body, p.buffered()), (b"abcdefghij".to_vec(), 0));
        }

        #[test]
        fn splitting_the_bytes_anywhere_gives_the_same_requests() {
            let wire: Vec<u8> = [
                &b"GET /a?q=1 HTTP/1.1\\r\\nHost: h\\r\\nX-A: 1\\r\\n\\r\\n"[..],
                b"POST /p HTTP/1.1\\r\\nContent-Length: 11\\r\\n\\r\\nhello world",
                b"POST /c HTTP/1.1\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\n3\\r\\nabc\\r\\n4;x=y\\r\\ndefg\\r\\n0\\r\\n\\r\\n",
                b"GET /z HTTP/1.0\\r\\n\\r\\n",
            ]
            .concat();
            let whole = all(&[&wire], lim()).unwrap();
            check!("four requests", whole.len(), 4);
            let mut rng = anneal_prelude::Rng::new(6804);
            for _ in 0..300 {
                let mut cuts: Vec<usize> = (0..rng.below(12)).map(|_| rng.below(wire.len())).collect();
                cuts.sort();
                let mut pieces: Vec<&[u8]> = Vec::new();
                let mut last = 0;
                for c in cuts {
                    pieces.push(&wire[last..c]);
                    last = c;
                }
                pieces.push(&wire[last..]);
                check!("same requests however the bytes are split", all(&pieces, lim()).unwrap(), whole.clone());
            }
            let bytewise: Vec<&[u8]> = wire.chunks(1).collect();
            check!("one byte at a time", all(&bytewise, lim()).unwrap(), whole);
        }
        """,
    ],
    wrong=dict(
        allows_space_before_colon=sub(HTTP, "let (name, value) = l.split_once(':').ok_or(ParseError::BadHeader)?;", "let (name, value) = l.split_once(':').ok_or(ParseError::BadHeader)?;\n            let name = name.trim_end();"),
        accepts_both_framing_headers=sub(HTTP, "        if chunked && content_length.is_some() {\n            return Err(ParseError::ConflictingLength); // the request-smuggling ambiguity: refuse it\n        }\n", "        if chunked && content_length.is_some() {\n            chunked = false;\n        }\n"),
        first_content_length_wins=sub(HTTP, "Some(prev) if prev != len => return Err(ParseError::ConflictingLength),\n                    _ => content_length = Some(len),", "Some(_) => {}\n                    None => content_length = Some(len),"),
        no_early_head_limit=sub(HTTP, "            if self.buf.len() > self.limits.max_head {\n                return Err(ParseError::HeadTooLarge); // never wait for a terminator that is not coming\n            }\n", ""),
        plus_sign_content_length=sub(HTTP, "if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {", "if v.is_empty() {"),
        body_limit_after_the_body_arrives=sub(HTTP, "            if len > self.limits.max_body {\n                return Err(ParseError::BodyTooLarge); // decided from the header, before the body arrives\n            }\n            if self.buf.len() < head_len + len {\n                return Ok(None);\n            }", "            if self.buf.len() < head_len + len {\n                return Ok(None);\n            }\n            if len > self.limits.max_body {\n                return Err(ParseError::BodyTooLarge);\n            }"),
    ),
    hints=[("approach", "Find `\\r\\n\\r\\n`; no terminator means incomplete (but already an error if the buffer exceeds `max_head`). Split the head into lines: request line, then headers. Decide the body framing from the headers (reject both / conflicting), then see whether the whole body is in the buffer; only when it is, drain exactly the request's bytes."),
           ("rust", "`str::split(\"\\r\\n\")` for lines, `split_once(':')` for a header, `bytes().all(is_token_char)` to validate names (a space or tab before the colon fails it, which is exactly the point). For lengths use `all(|b| b.is_ascii_digit())` *and then* `parse::<usize>()`: `parse` alone accepts a leading `+`. A helper `parse_chunked(buf) -> Result<Option<(body, used)>>` keeps the state machine out of the main function."),
           ("edge", "Return `Ok(None)` without consuming anything while a request is incomplete, so the next `feed` re-parses from the start (O(n\u00b2) worst case: a follow-up). Apply `max_body` from the announced `Content-Length` or chunk size, before the data is here. A request with no `Content-Length` and no chunking has an empty body.")],
    notes=("This is what `httparse` + `hyper` do (hyper is pickier still, and rejects every case here). Request smuggling is the attack where a front proxy and a back end disagree about where a request ends; it needs exactly one of: whitespace-before-colon, duplicate `Content-Length`, or `Content-Length` plus `Transfer-Encoding`. Parsers that are strict, bounded and incremental are the single biggest defence for an internet-facing Rust service.", "O(n) per call (re-parse), O(n) total with a resumable state machine", "O(max_head + max_body)"),
    follow_up="1) Make the parser resumable (a state enum: `RequestLine`, `Headers`, `BodyFixed(n)`, `BodyChunked(...)`) so each byte is examined once. 2) Add `Expect: 100-continue`, absolute-form targets and `Host` validation for HTTP/1.1 (a missing `Host` is an error). 3) Write the response side: status line, `Content-Length`, `Connection: close` / keep-alive rules for 1.0 vs 1.1. 4) Fuzz it with `cargo fuzz` or a property test comparing against `httparse` on random bytes: what do you find? 5) Plug it into the length-prefixed decoder pattern: write a `tokio_util::codec::Decoder` impl and a toy server using the C5 executor.",
    related=["B5", "S9", "F7"],
))

STAGES = [("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("b4-networking-protocols", "B4", "Networking & protocols", "B", "core", 4,
                    "Wire protocols without a socket: framing, TCP stream reassembly and a strict HTTP/1.1 parser, all as pure state machines you can test.",
                    STAGES, P)
    print("B4", n)
