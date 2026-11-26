//! RECIPE 189 (protocols): DNS wire format (RFC 1035): header, questions, A/AAAA/CNAME/NS/MX/TXT records, name compression.
//! LEVEL: advanced
//! Use in: DNS clients/servers/stub resolvers, mDNS/DoH tooling, packet inspectors, any format with back-references.   Run: cargo run --release --example dns_message_codec
#![allow(dead_code)]
//!
//! WHAT: a DNS message is a 12-byte header (id, flags, four big-endian counts) followed by question, answer, authority and
//! additional sections. Each resource record is `NAME TYPE(2) CLASS(2) TTL(4) RDLENGTH(2) RDATA`. All integers are big-endian
//! (network order). A NAME is a sequence of length-prefixed labels ending in a zero byte (`3www7example3com0`).
//! COMPRESSION: because the same suffix repeats constantly, a label position may instead be a 2-byte POINTER: top two bits
//! `11` plus a 14-bit offset from the start of the message, meaning "the rest of the name is over there". The encoder keeps a
//! map suffix -> first offset where it was written and emits a pointer on every repeat (only offsets below 0x4000 are
//! addressable). A pointer ends the name, so a name's consumed length is its bytes up to and including the first pointer.
//! WHY POINTERS ARE A CLASSIC VULNERABILITY SOURCE: the decoder follows attacker-controlled offsets inside the same buffer.
//! Without guards a pointer to itself (or A -> B -> A) loops forever (CPU DoS); a pointer past the end reads out of
//! bounds; and expansion can make a tiny packet decode to a huge name (labels reused through pointer chains) overflowing a
//! fixed 255-byte buffer in C. Our decoder (1) requires every pointer to point STRICTLY BACKWARD, so pointer-only chains
//! must terminate, (2) caps total jumps, (3) caps the expanded name at 255 bytes (labels at 63, which the 6-bit length gives
//! for free), (4) rejects the reserved label types `01` and `10`, and (5) bounds-checks every read. Record counts are also
//! untrusted: we never allocate from them, and reject counts that cannot fit in the bytes left.
//! COMMON USES: stub resolvers and recursive resolvers, authoritative servers, DNS-over-HTTPS/TLS proxies, mDNS/Bonjour
//! service discovery, DNS-based service discovery (SRV/TXT), packet capture tools, and fuzz targets (DNS parsers have a long
//! CVE history, exactly because of compression).
//! Read: https://datatracker.ietf.org/doc/html/rfc1035
use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------- model

type Name = Vec<Vec<u8>>; // labels, root = []

fn name(s: &str) -> Name {
    s.split('.')
        .filter(|l| !l.is_empty())
        .map(|l| l.as_bytes().to_vec())
        .collect()
}

fn show(n: &Name) -> String {
    if n.is_empty() {
        return ".".into();
    }
    n.iter()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect::<Vec<_>>()
        .join(".")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DnsError {
    Truncated,
    BadLabelType,
    ForwardPointer,
    TooManyJumps,
    NameTooLong,
    LabelTooLong,
    BadRdata,
    TooManyRecords,
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq)]
enum RData {
    A([u8; 4]),
    Aaaa([u8; 16]),
    Cname(Name),
    Ns(Name),
    Mx { pref: u16, exchange: Name },
    Txt(Vec<Vec<u8>>),
    Other(u16, Vec<u8>),
}

impl RData {
    fn type_code(&self) -> u16 {
        match self {
            RData::A(_) => 1,
            RData::Ns(_) => 2,
            RData::Cname(_) => 5,
            RData::Mx { .. } => 15,
            RData::Txt(_) => 16,
            RData::Aaaa(_) => 28,
            RData::Other(t, _) => *t,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Question {
    name: Name,
    qtype: u16,
    qclass: u16,
}

#[derive(Debug, Clone, PartialEq)]
struct Record {
    name: Name,
    class: u16,
    ttl: u32,
    data: RData,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Flags {
    qr: bool,
    opcode: u8, // 4 bits
    aa: bool,
    tc: bool,
    rd: bool,
    ra: bool,
    rcode: u8, // 4 bits
}

impl Flags {
    fn to_u16(self) -> u16 {
        (self.qr as u16) << 15
            | (self.opcode as u16 & 0xf) << 11
            | (self.aa as u16) << 10
            | (self.tc as u16) << 9
            | (self.rd as u16) << 8
            | (self.ra as u16) << 7
            | (self.rcode as u16 & 0xf)
    }
    fn from_u16(v: u16) -> Flags {
        Flags {
            qr: v >> 15 & 1 == 1,
            opcode: (v >> 11 & 0xf) as u8,
            aa: v >> 10 & 1 == 1,
            tc: v >> 9 & 1 == 1,
            rd: v >> 8 & 1 == 1,
            ra: v >> 7 & 1 == 1,
            rcode: (v & 0xf) as u8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Message {
    id: u16,
    flags: Flags,
    questions: Vec<Question>,
    answers: Vec<Record>,
    authority: Vec<Record>,
    additional: Vec<Record>,
}

// ---------------------------------------------------------------- encoder

struct Encoder {
    out: Vec<u8>,
    seen: HashMap<Name, u16>, // suffix -> offset of its first occurrence
    compress: bool,
}

impl Encoder {
    fn put_name(&mut self, n: &Name) -> Result<(), DnsError> {
        let wire: usize = n.iter().map(|l| l.len() + 1).sum::<usize>() + 1;
        if n.iter().any(|l| l.is_empty() || l.len() > 63) {
            return Err(DnsError::LabelTooLong);
        }
        if wire > 255 {
            return Err(DnsError::NameTooLong);
        }
        for i in 0..n.len() {
            let suffix = n[i..].to_vec();
            if self.compress {
                if let Some(&off) = self.seen.get(&suffix) {
                    self.out.extend_from_slice(&(0xC000 | off).to_be_bytes());
                    return Ok(());
                }
                if self.out.len() < 0x4000 {
                    self.seen.insert(suffix, self.out.len() as u16);
                }
            }
            self.out.push(n[i].len() as u8);
            self.out.extend_from_slice(&n[i]);
        }
        self.out.push(0);
        Ok(())
    }

    fn put_u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_be_bytes());
    }

    fn put_record(&mut self, r: &Record) -> Result<(), DnsError> {
        self.put_name(&r.name)?;
        self.put_u16(r.data.type_code());
        self.put_u16(r.class);
        self.out.extend_from_slice(&r.ttl.to_be_bytes());
        let len_at = self.out.len();
        self.put_u16(0); // RDLENGTH placeholder, back-patched below
        match &r.data {
            RData::A(a) => self.out.extend_from_slice(a),
            RData::Aaaa(a) => self.out.extend_from_slice(a),
            RData::Cname(n) | RData::Ns(n) => self.put_name(n)?,
            RData::Mx { pref, exchange } => {
                self.put_u16(*pref);
                self.put_name(exchange)?;
            }
            RData::Txt(parts) => {
                for p in parts {
                    if p.len() > 255 {
                        return Err(DnsError::BadRdata);
                    }
                    self.out.push(p.len() as u8);
                    self.out.extend_from_slice(p);
                }
            }
            RData::Other(_, b) => self.out.extend_from_slice(b),
        }
        let rdlen = self.out.len() - len_at - 2;
        if rdlen > 0xffff {
            return Err(DnsError::BadRdata);
        }
        self.out[len_at..len_at + 2].copy_from_slice(&(rdlen as u16).to_be_bytes());
        Ok(())
    }
}

fn encode(m: &Message, compress: bool) -> Result<Vec<u8>, DnsError> {
    let mut e = Encoder {
        out: Vec::new(),
        seen: HashMap::new(),
        compress,
    };
    e.put_u16(m.id);
    e.put_u16(m.flags.to_u16());
    for n in [
        m.questions.len(),
        m.answers.len(),
        m.authority.len(),
        m.additional.len(),
    ] {
        e.put_u16(u16::try_from(n).map_err(|_| DnsError::TooManyRecords)?);
    }
    for q in &m.questions {
        e.put_name(&q.name)?;
        e.put_u16(q.qtype);
        e.put_u16(q.qclass);
    }
    for r in m.answers.iter().chain(&m.authority).chain(&m.additional) {
        e.put_record(r)?;
    }
    Ok(e.out)
}

// ---------------------------------------------------------------- decoder

fn u16_at(b: &[u8], p: usize) -> Result<u16, DnsError> {
    Ok(u16::from_be_bytes(
        b.get(p..p + 2)
            .ok_or(DnsError::Truncated)?
            .try_into()
            .unwrap(),
    ))
}

/// Decode a possibly-compressed name at `pos`; returns (labels, position just after the name IN THE ORIGINAL STREAM).
fn decode_name(buf: &[u8], pos: usize) -> Result<(Name, usize), DnsError> {
    let (mut p, mut end, mut jumps, mut total) = (pos, None, 0, 1usize); // total includes the final root byte
    let mut labels = Vec::new();
    loop {
        let len = *buf.get(p).ok_or(DnsError::Truncated)?;
        match len & 0xC0 {
            0x00 if len == 0 => return Ok((labels, end.unwrap_or(p + 1))),
            0x00 => {
                let l = len as usize;
                let label = buf.get(p + 1..p + 1 + l).ok_or(DnsError::Truncated)?;
                total += l + 1;
                if total > 255 {
                    return Err(DnsError::NameTooLong); // also what stops "label, pointer-back-to-label" cycles
                }
                labels.push(label.to_vec());
                p += 1 + l;
            }
            0xC0 => {
                let ptr = ((len as usize & 0x3f) << 8)
                    | *buf.get(p + 1).ok_or(DnsError::Truncated)? as usize;
                end.get_or_insert(p + 2);
                if ptr >= p {
                    return Err(DnsError::ForwardPointer); // self or forward: the loop/overread guard
                }
                jumps += 1;
                if jumps > 127 {
                    return Err(DnsError::TooManyJumps);
                }
                p = ptr;
            }
            _ => return Err(DnsError::BadLabelType), // 01 and 10 are reserved (old EDNS0 extended labels)
        }
    }
}

fn decode_record(buf: &[u8], pos: usize) -> Result<(Record, usize), DnsError> {
    let (name, p) = decode_name(buf, pos)?;
    let ty = u16_at(buf, p)?;
    let class = u16_at(buf, p + 2)?;
    let ttl = u32::from_be_bytes(
        buf.get(p + 4..p + 8)
            .ok_or(DnsError::Truncated)?
            .try_into()
            .unwrap(),
    );
    let rdlen = u16_at(buf, p + 8)? as usize;
    let (start, end) = (p + 10, p + 10 + rdlen);
    let rd = buf.get(start..end).ok_or(DnsError::Truncated)?;
    let name_exact = |at: usize| -> Result<Name, DnsError> {
        let (n, e) = decode_name(&buf[..end], at)?; // names inside RDATA may not read past RDATA...
        if e != end {
            Err(DnsError::BadRdata)
        } else {
            Ok(n)
        } // ...and must consume it exactly
    };
    let data = match ty {
        1 => RData::A(rd.try_into().map_err(|_| DnsError::BadRdata)?),
        28 => RData::Aaaa(rd.try_into().map_err(|_| DnsError::BadRdata)?),
        2 => RData::Ns(name_exact(start)?),
        5 => RData::Cname(name_exact(start)?),
        15 => {
            if rdlen < 3 {
                return Err(DnsError::BadRdata);
            }
            RData::Mx {
                pref: u16_at(buf, start)?,
                exchange: name_exact(start + 2)?,
            }
        }
        16 => {
            let (mut parts, mut i) = (Vec::new(), 0);
            while i < rd.len() {
                let l = rd[i] as usize;
                parts.push(rd.get(i + 1..i + 1 + l).ok_or(DnsError::BadRdata)?.to_vec());
                i += 1 + l;
            }
            RData::Txt(parts)
        }
        t => RData::Other(t, rd.to_vec()),
    };
    Ok((
        Record {
            name,
            class,
            ttl,
            data,
        },
        end,
    ))
}

fn decode(buf: &[u8]) -> Result<Message, DnsError> {
    if buf.len() < 12 {
        return Err(DnsError::Truncated);
    }
    let id = u16_at(buf, 0)?;
    let flags = Flags::from_u16(u16_at(buf, 2)?);
    let counts: Vec<usize> = (0..4)
        .map(|i| u16_at(buf, 4 + 2 * i).unwrap() as usize)
        .collect();
    // Smallest question = 5 bytes, smallest record = 11: a count that cannot fit is a lie.
    if counts[0] * 5 + (counts[1] + counts[2] + counts[3]) * 11 > buf.len() - 12 {
        return Err(DnsError::Truncated);
    }
    let mut pos = 12;
    let mut questions = Vec::new();
    for _ in 0..counts[0] {
        let (name, p) = decode_name(buf, pos)?;
        questions.push(Question {
            name,
            qtype: u16_at(buf, p)?,
            qclass: u16_at(buf, p + 2)?,
        });
        pos = p + 4;
    }
    let mut sections: [Vec<Record>; 3] = Default::default();
    for (s, &n) in sections.iter_mut().zip(&counts[1..]) {
        for _ in 0..n {
            let (r, p) = decode_record(buf, pos)?;
            s.push(r);
            pos = p;
        }
    }
    let [answers, authority, additional] = sections;
    Ok(Message {
        id,
        flags,
        questions,
        answers,
        authority,
        additional,
    })
}

// ---------------------------------------------------------------- tests / fuzz support

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn random_name(r: &mut Rng) -> Name {
    // A small pool of base domains makes suffix sharing (and so compression) common.
    const BASES: [&str; 3] = ["example.com", "mail.example.com", "a.b.test"];
    let mut n: Name = (0..r.below(3))
        .map(|_| {
            (0..1 + r.below(6))
                .map(|_| b'a' + r.below(26) as u8)
                .collect()
        })
        .collect();
    if r.below(4) != 0 {
        n.extend(name(BASES[r.below(3)]));
    }
    n
}

fn random_record(r: &mut Rng) -> Record {
    let data = match r.below(7) {
        0 => RData::A((r.next() as u32).to_be_bytes()),
        1 => {
            let mut a = [0u8; 16];
            a.iter_mut().for_each(|b| *b = r.next() as u8);
            RData::Aaaa(a)
        }
        2 => RData::Cname(random_name(r)),
        3 => RData::Ns(random_name(r)),
        4 => RData::Mx {
            pref: r.next() as u16,
            exchange: random_name(r),
        },
        5 => RData::Txt(
            (0..r.below(4))
                .map(|_| (0..r.below(40)).map(|_| r.next() as u8).collect())
                .collect(),
        ),
        _ => RData::Other(99, (0..r.below(10)).map(|_| r.next() as u8).collect()),
    };
    Record {
        name: random_name(r),
        class: 1,
        ttl: r.next() as u32,
        data,
    }
}

fn random_message(r: &mut Rng) -> Message {
    let flags = Flags::from_u16(r.next() as u16 & 0xFF8F | (r.next() as u16 & 0x7800)); // keep Z bits zero
    let recs = |r: &mut Rng| {
        (0..r.below(5))
            .map(|_| random_record(r))
            .collect::<Vec<_>>()
    };
    Message {
        id: r.next() as u16,
        flags,
        questions: (0..r.below(3))
            .map(|_| Question {
                name: random_name(r),
                qtype: 1 + r.below(30) as u16,
                qclass: 1,
            })
            .collect(),
        answers: recs(r),
        authority: recs(r),
        additional: recs(r),
    }
}

fn main() {
    // 1. A real-looking response, assembled by hand byte for byte: www.example.com CNAME web.example.com ; web A 93.184.216.34
    let mut pkt: Vec<u8> = vec![0xab, 0xcd, 0x81, 0x80, 0, 1, 0, 2, 0, 0, 0, 0];
    pkt.extend_from_slice(b"\x03www\x07example\x03com\x00"); // question name at offset 12 ("example" label at 16)
    pkt.extend_from_slice(&[0, 1, 0, 1]); // QTYPE A, QCLASS IN
    pkt.extend_from_slice(&[0xc0, 0x0c, 0, 5, 0, 1, 0, 0, 1, 0x2c, 0, 6]); // answer 1: ptr->12, CNAME, ttl 300, rdlen 6
    pkt.extend_from_slice(b"\x03web\xc0\x10"); // "web" + pointer to "example.com" at 16 (rdata starts at offset 45)
    pkt.extend_from_slice(&[0xc0, 45, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 93, 184, 216, 34]); // answer 2: ptr->"web...", A
    let m = decode(&pkt).unwrap();
    assert_eq!(m.id, 0xabcd);
    assert!(m.flags.qr && m.flags.rd && m.flags.ra && m.flags.rcode == 0);
    assert_eq!(show(&m.questions[0].name), "www.example.com");
    assert_eq!(m.answers[0].data, RData::Cname(name("web.example.com")));
    assert_eq!(
        (show(&m.answers[1].name), m.answers[1].ttl),
        ("web.example.com".to_string(), 60)
    );
    assert_eq!(m.answers[1].data, RData::A([93, 184, 216, 34]));
    println!(
        "hand-built {}-byte response: {} -> {} -> {:?}",
        pkt.len(),
        show(&m.answers[0].name),
        show(&m.answers[1].name),
        m.answers[1].data
    );
    // Re-encoding with compression reproduces a packet that decodes to the same message.
    assert_eq!(decode(&encode(&m, true).unwrap()).unwrap(), m);

    // 2. Compression actually saves bytes, and a query round trip.
    let mut r = Rng(0xD15EA5E);
    let q = Message {
        id: 7,
        flags: Flags {
            rd: true,
            ..Default::default()
        },
        questions: vec![Question {
            name: name("example.com"),
            qtype: 15,
            qclass: 1,
        }],
        answers: (1..4)
            .map(|i| Record {
                name: name("example.com"),
                class: 1,
                ttl: 300,
                data: RData::Mx {
                    pref: i * 10,
                    exchange: name(&format!("mx{i}.example.com")),
                },
            })
            .collect(),
        authority: vec![],
        additional: vec![Record {
            name: name("mx1.example.com"),
            class: 1,
            ttl: 60,
            data: RData::Txt(vec![b"v=spf1 -all".to_vec()]),
        }],
    };
    let (plain, packed) = (encode(&q, false).unwrap(), encode(&q, true).unwrap());
    assert!(packed.len() < plain.len());
    assert_eq!(decode(&packed).unwrap(), q);
    assert_eq!(decode(&plain).unwrap(), q);
    println!(
        "compression: {} bytes -> {} bytes",
        plain.len(),
        packed.len()
    );

    // 3. Random-message round trips (both modes) and every truncation is an error.
    let mut truncs = 0;
    for _ in 0..2000 {
        let m = random_message(&mut r);
        for c in [true, false] {
            let bytes = encode(&m, c).unwrap();
            assert_eq!(decode(&bytes).unwrap(), m);
        }
        let bytes = encode(&m, true).unwrap();
        for cut in 0..bytes.len() {
            assert!(
                decode(&bytes[..cut]).is_err(),
                "truncation at {cut} of {} decoded",
                bytes.len()
            );
            truncs += 1;
        }
    }
    println!("2000 random messages round-tripped; {truncs} truncations all rejected");

    // 4. Pointer attacks and limits.
    let head = |qd: u8| vec![0, 1, 0, 0, 0, qd, 0, 0, 0, 0, 0, 0];
    let tail = [0, 1, 0, 1];
    let attack = |nm: &[u8]| {
        let mut p = head(1);
        p.extend_from_slice(nm);
        p.extend_from_slice(&tail);
        decode(&p).unwrap_err()
    };
    assert_eq!(attack(&[0xc0, 12]), DnsError::ForwardPointer); // points at itself
    assert_eq!(attack(&[0xc0, 40]), DnsError::ForwardPointer); // forward
    assert_eq!(attack(&[1, b'a', 0xc0, 12]), DnsError::NameTooLong); // label then back to start: infinite cycle, bounded by 255
    assert_eq!(attack(&[0x40, 0]), DnsError::BadLabelType);
    assert_eq!(attack(&[0x80, 0]), DnsError::BadLabelType);
    assert_eq!(attack(&[5, b'a']), DnsError::Truncated);
    let long: Name = vec![vec![b'a'; 63]; 4]; // 4*64+1 = 257 > 255
    assert_eq!(
        encode(
            &Message {
                questions: vec![Question {
                    name: long,
                    qtype: 1,
                    qclass: 1
                }],
                ..q.clone()
            },
            true
        )
        .unwrap_err(),
        DnsError::NameTooLong
    );
    assert_eq!(put_check(vec![vec![b'a'; 64]]), DnsError::LabelTooLong);
    let mut lying = head(0);
    lying[7] = 255; // claims 255 answers in a 12-byte packet
    assert_eq!(decode(&lying).unwrap_err(), DnsError::Truncated);
    println!("pointer loops, forward pointers, reserved label types, 255/63 limits: all rejected");

    // 5. Fuzz: random bytes and mutated valid packets never panic or loop; accepted input re-encodes stably.
    let (mut accepted, mut rejected) = (0, 0);
    for i in 0..30_000 {
        let input: Vec<u8> = if i % 3 == 0 {
            (0..r.below(120)).map(|_| r.next() as u8).collect()
        } else {
            let mut b = encode(&random_message(&mut r), true).unwrap();
            for _ in 0..1 + r.below(4) {
                if !b.is_empty() {
                    let at = r.below(b.len());
                    b[at] = if r.below(3) == 0 {
                        0xc0
                    } else {
                        r.next() as u8
                    };
                }
            }
            b
        };
        match decode(&input) {
            Ok(m) => {
                accepted += 1;
                let again = decode(&encode(&m, true).unwrap()).unwrap();
                assert_eq!(again, m);
            }
            Err(_) => rejected += 1,
        }
    }
    println!("fuzz: 30000 inputs ({accepted} accepted, {rejected} rejected), no panic, no loop");
}

fn put_check(n: Name) -> DnsError {
    let m = Message {
        id: 1,
        flags: Flags::default(),
        questions: vec![Question {
            name: n,
            qtype: 1,
            qclass: 1,
        }],
        answers: vec![],
        authority: vec![],
        additional: vec![],
    };
    encode(&m, true).unwrap_err()
}
