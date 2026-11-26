//! RECIPE 1108 (crates): bytes: Bytes, BytesMut, Buf, BufMut, zero-copy slicing and an incremental length-delimited decoder
//! LEVEL: intermediate
//! Use in: network protocols, proxies, storage formats; anywhere one buffer is parsed into many owned pieces.   Run: cargo run --release -p cb-crate-tour --example bytes_buf_buf_mut_framing
//! COMMON USES: the buffer type of tokio, hyper, axum, tonic and prost; TCP framing; fan-out of one message to many subscribers.
//! METHODS: Bytes::from_static, Bytes::copy_from_slice, Bytes::from, Bytes::slice, Bytes::slice_ref, Bytes::split_to, Bytes::split_off, Bytes::truncate, Bytes::try_into_mut, BytesMut::with_capacity, BytesMut::extend_from_slice, BytesMut::reserve, BytesMut::split, BytesMut::split_to, BytesMut::split_off, BytesMut::freeze, BytesMut::unsplit, BytesMut::resize, Buf::remaining, Buf::chunk, Buf::advance, Buf::has_remaining, Buf::get_u8, Buf::get_u16, Buf::get_u32, Buf::get_u32_le, Buf::try_get_u32, Buf::copy_to_slice, Buf::copy_to_bytes, Buf::chain, Buf::take, Buf::reader, BufMut::put_u8, BufMut::put_u16, BufMut::put_u32, BufMut::put_u32_le, BufMut::put_slice, BufMut::limit, BufMut::writer, BufMut::remaining_mut, Bytes::{from_owner, is_unique}, BytesMut::{zeroed, try_reclaim, clone}, Buf::{get_uint, get_int, get_u64, chunks_vectored}, BufMut::{put_uint, put_bytes, put}, impl Buf for your own type, std::io::Write::write_vectored
//! DOCS: https://docs.rs/bytes/1/bytes/
//! CRATES: bytes = "1"
//!
//! MENTAL MODEL
//!   Bytes    = pointer + length + shared ownership of the allocation. clone() and slice() copy no data.
//!   BytesMut = a uniquely owned WINDOW of an allocation. split_to() hands out a disjoint window; freeze() makes it a Bytes.
//!   Buf      = a read cursor (get_* consume). BufMut = a write cursor (put_* append).
//!
//! CHEAT SHEET
//!   share a message with N tasks      -> Bytes::clone
//!   keep a field of a parsed message  -> slice()/split_to (zero copy) ; copy_from_slice to avoid pinning a huge buffer
//!   read loop                         -> BytesMut, append, decode until "need more", repeat
//!   maybe-incomplete input            -> check remaining() BEFORE get_* (they PANIC when short) or use try_get_*
//!   header + body without copying     -> a.chain(b)
//!   3, 5, 6, 7 byte integers          -> get_uint(n) / put_uint(v, n)
//!   several buffers, one syscall      -> chunks_vectored + write_vectored, then advance(n) (writes can be partial)
//!   mmap / pooled memory as Bytes     -> Bytes::from_owner
//!   segmented input of your own       -> impl Buf (remaining, chunk, advance) and get every get_* for free
//!
//! PITFALLS: a 10-byte slice keeps its whole backing allocation alive; get_u32 is big-endian (use get_u32_le for LE);
//! never trust a length prefix (cap it BEFORE reserving memory).
//!
//! Read: https://docs.rs/bytes/1/bytes/struct.Bytes.html ; https://tokio.rs/tokio/tutorial/framing
//!
//! TRY NEXT:
//!   - [S] Remove the MAX_FRAME check and feed the decoder the header FF FF FF FF. What would reserve do?
//!   - [M] Add a CRC32 trailer to each frame and a test that flips one random bit.
//!   - [A] PROJECT: wrap `Decoder` around a tokio TcpStream (see `tokio_runtime_tasks_channels_timers`) with read_buf.
#![allow(dead_code)]
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::{Read, Write};

fn inside(outer: &[u8], inner: &[u8]) -> bool {
    let o = outer.as_ptr_range();
    let i = inner.as_ptr_range();
    o.start <= i.start && i.end <= o.end
}

fn bytes_is_cheap_to_clone_and_slice() {
    let data = Bytes::from((0..=255u8).collect::<Vec<_>>()); // takes the Vec's buffer, no copy
    let copy = data.clone();
    assert_eq!(copy.as_ptr(), data.as_ptr(), "clone shares the same memory");
    let mid = data.slice(10..20);
    assert_eq!(mid.as_ptr(), data[10..].as_ptr());
    assert!(inside(&data, &mid));
    // slice_ref: turn a &[u8] obtained from ordinary slice code back into an owned Bytes of the same memory.
    let text = Bytes::from_static(b"key=value");
    let value: &[u8] = text.split(|&b| b == b'=').nth(1).unwrap();
    let owned = text.slice_ref(value);
    assert_eq!(
        (owned.as_ptr(), &owned[..]),
        (value.as_ptr(), &b"value"[..])
    );
    // copy_from_slice makes an independent allocation: use it to release a large parent.
    let detached = Bytes::copy_from_slice(&mid);
    assert!(!inside(&data, &detached) && detached == mid);
    // split_to returns the FRONT and keeps the rest; split_off returns the BACK and keeps the front.
    let mut rest = data.clone();
    let front = rest.split_to(4);
    assert_eq!(
        (&front[..], rest[0], rest.len()),
        (&[0, 1, 2, 3][..], 4, 252)
    );
    let back = rest.split_off(250);
    assert_eq!((rest.len(), &back[..]), (250, &[254, 255][..]));
    rest.truncate(1);
    assert_eq!(&rest[..], [4]);
    // The slice outlives the original handle.
    drop((data, copy, front, back));
    assert_eq!(mid[0], 10);
    // Unique again => the buffer can be reclaimed for mutation without copying.
    let unique = Bytes::from(vec![1, 2, 3]);
    let ptr = unique.as_ptr();
    let mut m = unique.try_into_mut().expect("only handle");
    m[0] = 9;
    assert_eq!((m.as_ptr(), &m[..]), (ptr, &[9, 2, 3][..]));
    let shared = Bytes::from(vec![1]);
    let _other = shared.clone();
    assert!(shared.try_into_mut().is_err());
}

fn bytes_mut_windows_and_growth() {
    let mut buf = BytesMut::with_capacity(64);
    buf.extend_from_slice(b"hello world");
    let base = buf.as_ptr();
    let hello = buf.split_to(5);
    assert_eq!(
        (hello.as_ptr(), &hello[..], &buf[..]),
        (base, &b"hello"[..], &b" world"[..]),
        "two disjoint windows, no copy"
    );
    // freeze: BytesMut -> Bytes without copying.
    let frozen = hello.freeze();
    assert_eq!(frozen.as_ptr(), base);
    // unsplit rejoins ADJACENT windows without copying.
    let mut a = BytesMut::from(&b"abcdef"[..]);
    let tail = a.split_off(3);
    let ptr = a.as_ptr();
    a.unsplit(tail);
    assert_eq!((a.as_ptr(), &a[..]), (ptr, &b"abcdef"[..]));
    // reserve() can REUSE the allocation once every split-off piece has been dropped.
    let mut ring = BytesMut::with_capacity(1024);
    let start = ring.as_ptr();
    let mut allocations_moved = 0;
    for i in 0..10_000u32 {
        ring.reserve(8);
        ring.put_u64(i as u64);
        let msg = ring.split().freeze(); // everything written so far
        assert_eq!(msg.len(), 8);
        drop(msg); // consumer is done
        allocations_moved += (ring.capacity() > 1024) as u32;
    }
    ring.reserve(1024);
    assert_eq!(
        (ring.as_ptr(), allocations_moved),
        (start, 0),
        "80_000 bytes went through one 1 KiB buffer"
    );
    // But a retained piece pins the allocation, so reserve must get new memory.
    let mut pinned = BytesMut::with_capacity(16);
    pinned.put_slice(&[1; 16]);
    let keep = pinned.split().freeze();
    pinned.reserve(16);
    pinned.put_slice(&[2; 16]);
    assert!(keep.iter().all(|&b| b == 1) && !inside(&keep, &pinned));
    let mut z = BytesMut::new();
    z.resize(4, 0xAA);
    assert_eq!(&z[..], [0xAA; 4]);
}

fn buf_and_bufmut_cursors() {
    let mut out = BytesMut::new();
    out.put_u8(1);
    out.put_u16(0x0203);
    out.put_u32(0x0405_0607);
    out.put_u32_le(0x0405_0607);
    out.put_slice(b"ok");
    assert_eq!(
        &out[..],
        [1, 2, 3, 4, 5, 6, 7, 7, 6, 5, 4, b'o', b'k'],
        "big-endian unless _le"
    );
    let mut r: &[u8] = &out; // &[u8] is itself a Buf: reading advances the slice, the BytesMut is untouched
    assert_eq!(
        (r.get_u8(), r.get_u16(), r.get_u32(), r.get_u32_le()),
        (1, 0x0203, 0x0405_0607, 0x0405_0607)
    );
    let mut two = [0u8; 2];
    r.copy_to_slice(&mut two);
    assert_eq!((&two, r.has_remaining(), out.len()), (b"ok", false, 13));
    let mut short: &[u8] = &[1, 2];
    assert!(
        short.try_get_u32().is_err(),
        "try_get_* reports instead of panicking"
    );
    assert_eq!(short.remaining(), 2, "and consumes nothing");

    // chain: header and body as one logical buffer, no concatenation.
    let mut c = Bytes::from_static(b"\x00\x00").chain(Bytes::from_static(b"\x00\x05xy"));
    assert_eq!(c.remaining(), 6);
    assert_eq!(
        c.chunk(),
        b"\x00\x00",
        "chunk() is only the FIRST contiguous part: loop over it, never assume it is everything"
    );
    assert_eq!(c.get_u32(), 5, "get_u32 reads across the seam");
    assert_eq!(&c.copy_to_bytes(2)[..], b"xy");
    // take: a sub-parser that cannot read past its frame.
    let mut whole: &[u8] = b"abcdef";
    // With std::io::Read also imported, `.take()` is ambiguous (both traits provide it): name the trait.
    let mut body = Buf::take(&mut whole, 4);
    assert_eq!(&body.copy_to_bytes(body.remaining())[..], b"abcd");
    assert_eq!(whole, b"ef");
    // limit: a bounded writer.
    let mut lim = Vec::new().limit(3);
    assert_eq!(lim.remaining_mut(), 3);
    lim.put_slice(b"abc");
    assert_eq!(
        (lim.remaining_mut(), lim.into_inner()),
        (0, b"abc".to_vec())
    );
    // Adapters to std::io.
    let mut text = String::new();
    Bytes::from_static(b"via Read")
        .reader()
        .read_to_string(&mut text)
        .unwrap();
    let mut w = BytesMut::new().writer();
    write!(w, "{text}!").unwrap();
    assert_eq!(&w.into_inner()[..], b"via Read!");
}

// ---------------- a length-delimited codec: [u32 length][u8 kind][payload] ----------------
const MAX_FRAME: usize = 64 * 1024;
const HEADER: usize = 5;
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    kind: u8,
    payload: Bytes,
}
#[derive(Debug, PartialEq)]
enum DecodeError {
    TooLarge(usize),
}
fn encode(f: &Frame, out: &mut BytesMut) {
    assert!(f.payload.len() <= MAX_FRAME);
    out.reserve(HEADER + f.payload.len());
    out.put_u32(f.payload.len() as u32);
    out.put_u8(f.kind);
    out.put_slice(&f.payload);
}
/// Ok(None) = incomplete: NOTHING is consumed, call again after appending more input.
fn decode(buf: &mut BytesMut) -> Result<Option<Frame>, DecodeError> {
    let Some(mut head) = buf.get(..HEADER) else {
        return Ok(None);
    }; // peek; `head` is a &[u8] cursor
    let len = head.get_u32() as usize;
    let kind = head.get_u8();
    if len > MAX_FRAME {
        return Err(DecodeError::TooLarge(len)); // reject BEFORE reserving: the peer controls this number
    }
    if buf.len() < HEADER + len {
        buf.reserve(HEADER + len - buf.len());
        return Ok(None);
    }
    buf.advance(HEADER);
    Ok(Some(Frame {
        kind,
        payload: buf.split_to(len).freeze(),
    }))
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}
fn codec_under_arbitrary_fragmentation() {
    let mut r = Rng(0x1108);
    for _ in 0..500 {
        let frames: Vec<Frame> = (0..r.below(8))
            .map(|_| {
                let len = if r.below(10) == 0 {
                    MAX_FRAME - r.below(2) as usize
                } else {
                    r.below(40) as usize
                };
                Frame {
                    kind: r.below(256) as u8,
                    payload: (0..len).map(|i| (i * 7) as u8).collect::<Vec<_>>().into(),
                }
            })
            .collect();
        let mut wire = BytesMut::new();
        frames.iter().for_each(|f| encode(f, &mut wire));
        // Deliver in random fragments, like TCP does.
        let (mut input, mut got) = (BytesMut::new(), Vec::new());
        let mut remaining: &[u8] = &wire;
        while !remaining.is_empty() {
            let max = if r.below(2) == 0 { 7 } else { 20_000 };
            let n = (1 + r.below(max) as usize).min(remaining.len());
            input.extend_from_slice(&remaining[..n]);
            remaining = &remaining[n..];
            while let Some(f) = decode(&mut input).unwrap() {
                got.push(f);
            }
        }
        assert_eq!(got, frames);
        assert!(input.is_empty());
    }
    // Zero copy: the payload points into the buffer the bytes were received in.
    let mut input = BytesMut::new();
    encode(
        &Frame {
            kind: 1,
            payload: Bytes::from_static(b"payload"),
        },
        &mut input,
    );
    let at = input[HEADER..].as_ptr();
    assert_eq!(decode(&mut input).unwrap().unwrap().payload.as_ptr(), at);
    // Hostile length prefix.
    let mut evil = BytesMut::from(&[0xFF, 0xFF, 0xFF, 0xFF, 0][..]);
    let cap = evil.capacity();
    assert_eq!(
        decode(&mut evil),
        Err(DecodeError::TooLarge(u32::MAX as usize))
    );
    assert_eq!(evil.capacity(), cap, "no memory was reserved");
    // Truncated stream: every strict prefix is "need more", never a panic or a bogus frame.
    let mut full = BytesMut::new();
    encode(
        &Frame {
            kind: 2,
            payload: Bytes::from_static(b"abc"),
        },
        &mut full,
    );
    for cut in 0..full.len() {
        let mut part = BytesMut::from(&full[..cut]);
        assert_eq!(decode(&mut part), Ok(None), "prefix {cut}");
        assert_eq!(part.len(), cut, "nothing consumed");
    }
    println!("codec: 500 random frame sequences survive random fragmentation");
}

/// Scenario: the 9-byte HTTP/2 frame header (RFC 9113, section 4.1): length:24, type:8, flags:8, reserved:1, stream:31.
fn odd_width_integers() {
    let mut out = BytesMut::new();
    out.put_uint(0x01_02_03, 3); // the low 3 bytes, big-endian
    out.put_u8(0x1);
    out.put_u8(0x5);
    out.put_u32(0x8000_0007); // top bit is reserved and must be ignored by the reader
    assert_eq!(&out[..], [1, 2, 3, 1, 5, 0x80, 0, 0, 7]);
    let mut r = out.freeze();
    let (len, kind, flags, stream) = (
        r.get_uint(3),
        r.get_u8(),
        r.get_u8(),
        r.get_u32() & 0x7FFF_FFFF,
    );
    assert_eq!((len, kind, flags, stream), (0x010203, 1, 5, 7));
    // get_int sign-extends from the given width; get_uint does not.
    let raw = [0xFF, 0xFF, 0xFE];
    assert_eq!((&raw[..]).get_int(3), -2);
    assert_eq!((&raw[..]).get_uint(3), 0xFF_FF_FE);
    // PITFALL: put_uint truncates silently to the width you ask for. Range-check lengths yourself.
    let mut t = BytesMut::new();
    t.put_uint(0x0100_0005, 3);
    assert_eq!((&t[..]).get_uint(3), 5);
}

/// A Buf over several non-contiguous segments. Only three methods are required; get_u32, copy_to_slice, take, chain and
/// the rest then work across segment boundaries.
struct Segments {
    parts: std::collections::VecDeque<Bytes>,
}
impl Buf for Segments {
    fn remaining(&self) -> usize {
        self.parts.iter().map(Bytes::len).sum() // O(segments): cache it in real code
    }
    /// CONTRACT: empty only when remaining() == 0. So empty segments must never sit at the front.
    fn chunk(&self) -> &[u8] {
        self.parts.front().map_or(&[], |b| b)
    }
    fn advance(&mut self, mut n: usize) {
        assert!(n <= self.remaining(), "advance past the end");
        while n > 0 {
            let front = &mut self.parts[0];
            let step = n.min(front.len());
            front.advance(step);
            n -= step;
            if front.is_empty() {
                self.parts.pop_front();
            }
        }
    }
}
impl Segments {
    fn new(parts: impl IntoIterator<Item = Bytes>) -> Self {
        Segments {
            parts: parts.into_iter().filter(|p| !p.is_empty()).collect(),
        }
    }
}

fn own_buf_implementation() {
    let mut r = Rng(0x5E6);
    for _ in 0..1000 {
        let numbers: Vec<u64> = (0..r.below(6)).map(|_| r.below(u64::MAX)).collect();
        let mut flat = BytesMut::new();
        numbers.iter().for_each(|&n| flat.put_u64(n));
        // Cut the same bytes at random places, including empty pieces.
        let mut rest = flat.clone().freeze();
        let mut parts = vec![];
        while !rest.is_empty() {
            let n = (r.below(11) as usize).min(rest.len());
            parts.push(rest.split_to(n));
        }
        let mut seg = Segments::new(parts);
        assert_eq!(seg.remaining(), flat.len());
        let got: Vec<u64> = (0..numbers.len()).map(|_| seg.get_u64()).collect();
        assert_eq!(got, numbers);
        assert!(!seg.has_remaining() && seg.chunk().is_empty());
    }
    // BufMut::put drains a whole Buf, whatever its shape.
    let mut out = BytesMut::new();
    out.put(Segments::new([
        Bytes::from_static(b"ab"),
        Bytes::from_static(b"cd"),
    ]));
    out.put_bytes(b'-', 3);
    assert_eq!(&out[..], b"abcd---");
}

/// Accepts at most `max` bytes per call, as a socket with a nearly full send buffer does.
struct ShortWriter {
    data: Vec<u8>,
    max: usize,
    calls: usize,
}
impl Write for ShortWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.write_vectored(&[std::io::IoSlice::new(b)])
    }
    fn write_vectored(&mut self, slices: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.calls += 1;
        let mut left = self.max;
        for s in slices {
            let n = left.min(s.len());
            self.data.extend_from_slice(&s[..n]);
            left -= n;
        }
        Ok(self.max - left)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn write_all_vectored(w: &mut impl Write, buf: &mut impl Buf) -> std::io::Result<()> {
    while buf.has_remaining() {
        let mut slices = [std::io::IoSlice::new(&[]); 8];
        let filled = buf.chunks_vectored(&mut slices);
        let n = w.write_vectored(&slices[..filled])?;
        if n == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        buf.advance(n); // a short write is normal: the Buf remembers where we are, even in the middle of a segment
    }
    Ok(())
}
fn vectored_writes() {
    let header = Bytes::from_static(b"HDR:");
    let body = Bytes::from(vec![b'x'; 100]);
    let frame = || header.clone().chain(body.clone()); // two reference-count bumps, no byte copies
    let first = frame();
    let mut slices = [std::io::IoSlice::new(&[]); 4];
    assert_eq!(first.chunks_vectored(&mut slices), 2);
    assert_eq!(
        (slices[0].as_ptr(), slices[1].as_ptr()),
        (header.as_ptr(), body.as_ptr())
    );
    for max in [1, 3, 4, 5, 64, 1000] {
        let mut w = ShortWriter {
            data: vec![],
            max,
            calls: 0,
        };
        write_all_vectored(&mut w, &mut frame()).unwrap();
        assert_eq!(w.data, [&header[..], &body[..]].concat(), "max {max}");
        assert_eq!(w.calls, 104usize.div_ceil(max));
    }
    // PITFALL: a Buf that does not override chunks_vectored reports only chunk(), one segment per syscall. `Segments`
    // above is such a type.
    let seg = Segments::new([header.clone(), body.clone()]);
    assert_eq!(seg.chunks_vectored(&mut slices), 1);
}

fn ownership_and_writer_limits() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    // from_owner: wrap memory that something else manages (an mmap, a pool slot). Drop runs when the LAST view goes.
    struct Pooled {
        data: Vec<u8>,
        returned: Arc<AtomicBool>,
    }
    impl AsRef<[u8]> for Pooled {
        fn as_ref(&self) -> &[u8] {
            &self.data
        }
    }
    impl Drop for Pooled {
        fn drop(&mut self) {
            self.returned.store(true, SeqCst);
        }
    }
    let returned = Arc::new(AtomicBool::new(false));
    let data = vec![1, 2, 3, 4];
    let ptr = data.as_ptr();
    let whole = Bytes::from_owner(Pooled {
        data,
        returned: returned.clone(),
    });
    assert_eq!(whole.as_ptr(), ptr, "no copy");
    let slice = whole.slice(1..3);
    assert!(
        !whole.is_unique(),
        "owner-backed Bytes never reports unique"
    );
    drop(whole);
    assert!(!returned.load(SeqCst), "a 2-byte slice still pins the slot");
    drop(slice);
    assert!(returned.load(SeqCst));

    let a = Bytes::from(vec![1u8; 8]);
    assert!(a.is_unique());
    let b = a.clone();
    assert!(!a.is_unique());
    drop(b);
    assert!(a.is_unique());
    // PITFALL: unlike Bytes::clone, BytesMut::clone is a DEEP copy (two writers may not share bytes).
    let m = BytesMut::zeroed(16);
    let copy = m.clone();
    assert!(m.as_ptr() != copy.as_ptr() && copy.iter().all(|&x| x == 0));

    // try_reclaim: reuse the allocation if that is possible without allocating, and say whether it was.
    let mut buf = BytesMut::with_capacity(64);
    buf.put_slice(&[0; 64]);
    let held = buf.split().freeze();
    assert!(!buf.try_reclaim(64), "a live view blocks reuse");
    drop(held);
    assert!(buf.try_reclaim(64));

    // Bytes works as a map key and can be looked up with a plain byte slice, without allocating a key.
    let map = std::collections::HashMap::from([(Bytes::from_static(b"content-type"), 1)]);
    assert_eq!(map.get(&b"content-type"[..]), Some(&1));

    // PITFALL: BufMut is implemented for BytesMut and Vec<u8> (they grow) and for &mut [u8] (it cannot).
    let mut fixed = [0u8; 4];
    let mut cursor = &mut fixed[..];
    cursor.put_u16(0xAABB);
    assert_eq!(cursor.remaining_mut(), 2);
    assert!(
        cursor.remaining_mut() < 4,
        "check before put_u32, which would panic"
    );
    assert_eq!(fixed, [0xAA, 0xBB, 0, 0]);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let overflow = std::panic::catch_unwind(|| (&mut [0u8; 2][..]).put_u32(1));
    let short_read = std::panic::catch_unwind(|| (&[1u8, 2][..]).get_u32());
    std::panic::set_hook(prev);
    assert!(overflow.is_err() && short_read.is_err());
}

pub fn main() {
    bytes_is_cheap_to_clone_and_slice();
    bytes_mut_windows_and_growth();
    buf_and_bufmut_cursors();
    codec_under_arbitrary_fragmentation();
    odd_width_integers();
    own_buf_implementation();
    vectored_writes();
    ownership_and_writer_limits();
    println!("bytes: all sections passed");
}
