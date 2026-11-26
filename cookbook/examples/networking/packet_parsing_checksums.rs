//! RECIPE 432 (network): parsing and building packets from raw bytes: Ethernet/VLAN, ARP, IPv4/IPv6, TCP, UDP, ICMP, Internet checksum, fragmentation, pcap, flow table
//! LEVEL: advanced
//! Use in: packet capture tools, firewalls, load balancers, IDS, userspace network stacks (smoltcp-style), protocol test harnesses.   Run: cargo run --release --example packet_parsing_checksums
#![allow(dead_code)]
//!
//! WHAT: a bounds-checked, zero-copy dissector. Each header type is a thin VIEW over `&[u8]`: `parse` validates
//! lengths ONCE (header present, IHL/data-offset sane, total length within the buffer) and the accessors then
//! read fixed offsets without copying. Anything inconsistent returns `ParseError`; nothing panics, which the
//! 30,000-input fuzz at the end checks. Stack handled: Ethernet II with 802.1Q/QinQ tags, ARP, IPv4 (IHL,
//! options, flags, fragment offset), IPv6 (extension-header chain), TCP (flags, MSS/WS/SACK/timestamp options),
//! UDP, ICMP and ICMPv6 echo.
//!
//! THE INTERNET CHECKSUM (RFC 1071): ones' complement of the ones' complement sum of 16-bit big-endian words
//! (odd trailing byte padded with 0). Properties used here: the sum is order-independent and can be folded
//! lazily (carries added back at the end); verifying = summing everything INCLUDING the checksum field and
//! getting 0xFFFF; RFC 1624 incremental update HC' = ~(~HC + ~m + m') patches a header when one word changes (TTL
//! decrement in a router) without re-summing. TCP/UDP/ICMPv6 checksums also cover a PSEUDO-HEADER (addresses,
//! protocol, length) so a segment delivered to the wrong host or protocol fails.
//!
//! FRAGMENTATION: IPv4 splits a datagram into fragments whose payload offsets are multiples of 8, all sharing
//! (src, dst, id, protocol); the last has MF=0. REASSEMBLY is attack surface: out-of-order and duplicate
//! fragments must work, but OVERLAPPING fragments with different bytes (teardrop, and the ambiguity IDS evasion
//! exploits) are rejected here by dropping the whole datagram; sizes past 65535 (ping of death), too many
//! fragments, and unbounded tables are capped. PCAP: the classic libpcap file (24-byte global header with a
//! magic that tells byte order and microsecond vs nanosecond timestamps, then 16-byte record headers).
//! FLOW TABLE: packets grouped by canonical 5-tuple with a simplified TCP state machine
//! (SYN -> SYN/ACK -> ACK -> FINs -> closed, RST), ignoring sequence validation and timers.
//!
//! COMMON USES: tcpdump/Wireshark dissectors, Suricata/Zeek-style monitors, Linux netfilter conntrack,
//! NAT devices (they patch addresses and ports with incremental checksum updates), software routers, smoltcp.
//! Read: RFC 1071 https://www.rfc-editor.org/rfc/rfc1071 ; RFC 1624 https://www.rfc-editor.org/rfc/rfc1624 ;
//! RFC 791 (IPv4) https://www.rfc-editor.org/rfc/rfc791 ; RFC 8200 (IPv6) https://www.rfc-editor.org/rfc/rfc8200 ;
//! RFC 793 (TCP) https://www.rfc-editor.org/rfc/rfc793 ; RFC 768 (UDP) https://www.rfc-editor.org/rfc/rfc768 ;
//! RFC 826 (ARP) https://www.rfc-editor.org/rfc/rfc826 ; RFC 792 (ICMP) https://www.rfc-editor.org/rfc/rfc792 ;
//! libpcap file format https://wiki.wireshark.org/Development/LibpcapFileFormat
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// ------------------------------------------------------------------------------------------------ errors
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseError {
    Truncated(&'static str),
    Malformed(&'static str),
}
type R<T> = Result<T, ParseError>;

fn need(b: &[u8], n: usize, what: &'static str) -> R<()> {
    if b.len() < n {
        Err(ParseError::Truncated(what))
    } else {
        Ok(())
    }
}
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([b[i], b[i + 1]])
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

// -------------------------------------------------------------------------------------------- checksums
/// Add 16-bit big-endian words into a wide accumulator (no folding yet, so no carry is ever lost).
fn sum16(data: &[u8]) -> u64 {
    let mut s = 0u64;
    let mut it = data.chunks_exact(2);
    for w in &mut it {
        s += u16::from_be_bytes([w[0], w[1]]) as u64;
    }
    if let [last] = it.remainder() {
        s += (*last as u64) << 8;
    }
    s
}
/// Fold carries back in: the ones' complement SUM.
fn fold(mut s: u64) -> u16 {
    while s >> 16 != 0 {
        s = (s & 0xffff) + (s >> 16);
    }
    s as u16
}
/// The Internet checksum of `parts` (only the LAST part may have odd length).
fn checksum(parts: &[&[u8]]) -> u16 {
    !fold(parts.iter().map(|p| sum16(p)).sum())
}
/// RFC 1624 eqn 3: HC' = ~(~HC + ~m + m') for one 16-bit word changing from m to m'.
fn csum_update(hc: u16, m: u16, m_new: u16) -> u16 {
    !fold((!hc) as u64 + (!m) as u64 + m_new as u64)
}
fn ip_octets(ip: &IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(a) => a.octets().to_vec(),
        IpAddr::V6(a) => a.octets().to_vec(),
    }
}
/// TCP/UDP/ICMPv6 pseudo-header (RFC 793 / RFC 8200 section 8.1).
fn pseudo_header(src: &IpAddr, dst: &IpAddr, proto: u8, len: usize) -> Vec<u8> {
    let mut p = ip_octets(src);
    p.extend(ip_octets(dst));
    if src.is_ipv4() {
        p.extend([0, proto]);
        p.extend((len as u16).to_be_bytes());
    } else {
        p.extend((len as u32).to_be_bytes());
        p.extend([0, 0, 0, proto]);
    }
    p
}
/// Checksum to PUT in a segment whose checksum field is currently zero.
fn l4_checksum(src: &IpAddr, dst: &IpAddr, proto: u8, seg: &[u8]) -> u16 {
    checksum(&[&pseudo_header(src, dst, proto, seg.len()), seg])
}
/// Verify a received segment (checksum field included): the sum must be 0xFFFF.
fn l4_verify(src: &IpAddr, dst: &IpAddr, proto: u8, seg: &[u8]) -> bool {
    fold(sum16(&pseudo_header(src, dst, proto, seg.len())) + sum16(seg)) == 0xffff
}

// ----------------------------------------------------------------------------------------------- views
const ETH_IPV4: u16 = 0x0800;
const ETH_ARP: u16 = 0x0806;
const ETH_VLAN: u16 = 0x8100;
const ETH_QINQ: u16 = 0x88a8;
const ETH_IPV6: u16 = 0x86dd;

struct Eth<'a> {
    dst: [u8; 6],
    src: [u8; 6],
    vlans: Vec<u16>, // VLAN ids (12 bits), outermost first
    ethertype: u16,
    payload: &'a [u8],
}
impl<'a> Eth<'a> {
    fn parse(b: &'a [u8]) -> R<Eth<'a>> {
        need(b, 14, "ethernet header")?;
        let (dst, src) = (b[0..6].try_into().unwrap(), b[6..12].try_into().unwrap());
        let (mut et, mut pos, mut vlans) = (u16_at(b, 12), 14, Vec::new());
        while matches!(et, ETH_VLAN | ETH_QINQ | 0x9100) {
            need(&b[pos..], 4, "802.1Q tag")?;
            if vlans.len() == 2 {
                return Err(ParseError::Malformed("more than 2 vlan tags"));
            }
            vlans.push(u16_at(b, pos) & 0x0fff);
            et = u16_at(b, pos + 2);
            pos += 4;
        }
        Ok(Eth {
            dst,
            src,
            vlans,
            ethertype: et,
            payload: &b[pos..],
        })
    }
}

struct Arp {
    op: u16,
    sha: [u8; 6],
    spa: Ipv4Addr,
    tha: [u8; 6],
    tpa: Ipv4Addr,
}
impl Arp {
    fn parse(b: &[u8]) -> R<Arp> {
        need(b, 28, "arp")?;
        if u16_at(b, 0) != 1 || u16_at(b, 2) != ETH_IPV4 || b[4] != 6 || b[5] != 4 {
            return Err(ParseError::Malformed("arp: only Ethernet/IPv4 supported"));
        }
        Ok(Arp {
            op: u16_at(b, 6),
            sha: b[8..14].try_into().unwrap(),
            spa: Ipv4Addr::new(b[14], b[15], b[16], b[17]),
            tha: b[18..24].try_into().unwrap(),
            tpa: Ipv4Addr::new(b[24], b[25], b[26], b[27]),
        })
    }
}

struct Ipv4<'a> {
    hdr: &'a [u8],
    payload: &'a [u8],
}
impl<'a> Ipv4<'a> {
    fn parse(b: &'a [u8]) -> R<Ipv4<'a>> {
        need(b, 20, "ipv4 header")?;
        if b[0] >> 4 != 4 {
            return Err(ParseError::Malformed("not ipv4"));
        }
        let ihl = (b[0] & 15) as usize * 4;
        if ihl < 20 {
            return Err(ParseError::Malformed("ihl < 5"));
        }
        need(b, ihl, "ipv4 options")?;
        let total = u16_at(b, 2) as usize;
        if total < ihl {
            return Err(ParseError::Malformed("total length < header length"));
        }
        need(b, total, "ipv4 total length")?; // bytes beyond `total` are Ethernet padding: ignored
        Ok(Ipv4 {
            hdr: &b[..ihl],
            payload: &b[ihl..total],
        })
    }
    fn src(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.hdr[12], self.hdr[13], self.hdr[14], self.hdr[15])
    }
    fn dst(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.hdr[16], self.hdr[17], self.hdr[18], self.hdr[19])
    }
    fn proto(&self) -> u8 {
        self.hdr[9]
    }
    fn ttl(&self) -> u8 {
        self.hdr[8]
    }
    fn id(&self) -> u16 {
        u16_at(self.hdr, 4)
    }
    fn df(&self) -> bool {
        self.hdr[6] & 0x40 != 0
    }
    fn mf(&self) -> bool {
        self.hdr[6] & 0x20 != 0
    }
    fn frag_offset_bytes(&self) -> usize {
        (u16_at(self.hdr, 6) & 0x1fff) as usize * 8
    }
    fn is_fragment(&self) -> bool {
        self.mf() || self.frag_offset_bytes() != 0
    }
    fn options(&self) -> &[u8] {
        &self.hdr[20..]
    }
    fn checksum_ok(&self) -> bool {
        fold(sum16(self.hdr)) == 0xffff
    }
}

/// What the IPv6 extension-header walk found.
struct Ipv6Walk<'a> {
    upper_proto: u8,
    upper: &'a [u8],
    fragment: Option<(usize, bool, u32)>, // (offset bytes, more fragments, id)
    ext_headers: usize,
}
struct Ipv6<'a> {
    hdr: &'a [u8],
    payload: &'a [u8],
}
impl<'a> Ipv6<'a> {
    fn parse(b: &'a [u8]) -> R<Ipv6<'a>> {
        need(b, 40, "ipv6 header")?;
        if b[0] >> 4 != 6 {
            return Err(ParseError::Malformed("not ipv6"));
        }
        let plen = u16_at(b, 4) as usize;
        need(b, 40 + plen, "ipv6 payload length")?;
        Ok(Ipv6 {
            hdr: &b[..40],
            payload: &b[40..40 + plen],
        })
    }
    fn src(&self) -> Ipv6Addr {
        Ipv6Addr::from(<[u8; 16]>::try_from(&self.hdr[8..24]).unwrap())
    }
    fn dst(&self) -> Ipv6Addr {
        Ipv6Addr::from(<[u8; 16]>::try_from(&self.hdr[24..40]).unwrap())
    }
    /// Walk hop-by-hop (0), routing (43), fragment (44), AH (51) and destination options (60) headers.
    fn walk(&self) -> R<Ipv6Walk<'a>> {
        let (mut nh, mut p, mut fragment, mut n) = (self.hdr[6], self.payload, None, 0);
        loop {
            match nh {
                0 | 43 | 60 | 51 => {
                    need(p, 8, "ipv6 extension header")?;
                    let len = if nh == 51 {
                        (p[1] as usize + 2) * 4
                    } else {
                        (p[1] as usize + 1) * 8
                    };
                    need(p, len, "ipv6 extension header body")?;
                    nh = p[0];
                    p = &p[len..];
                }
                44 => {
                    need(p, 8, "ipv6 fragment header")?;
                    fragment = Some((
                        (u16_at(p, 2) >> 3) as usize * 8,
                        p[3] & 1 != 0,
                        u32_at(p, 4),
                    ));
                    nh = p[0];
                    p = &p[8..];
                }
                _ => break,
            }
            n += 1;
            if n > 8 {
                return Err(ParseError::Malformed("too many ipv6 extension headers"));
            }
        }
        Ok(Ipv6Walk {
            upper_proto: nh,
            upper: p,
            fragment,
            ext_headers: n,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TcpOpt {
    Eol,
    Nop,
    Mss(u16),
    WindowScale(u8),
    SackPermitted,
    Sack(Vec<(u32, u32)>),
    Timestamps(u32, u32),
    Unknown(u8),
}
const FIN: u8 = 1;
const SYN: u8 = 2;
const RST: u8 = 4;
const PSH: u8 = 8;
const ACK: u8 = 0x10;

struct Tcp<'a> {
    b: &'a [u8],
    off: usize,
}
impl<'a> Tcp<'a> {
    fn parse(b: &'a [u8]) -> R<Tcp<'a>> {
        need(b, 20, "tcp header")?;
        let off = (b[12] >> 4) as usize * 4;
        if off < 20 {
            return Err(ParseError::Malformed("tcp data offset < 5"));
        }
        need(b, off, "tcp options")?;
        Ok(Tcp { b, off })
    }
    fn sport(&self) -> u16 {
        u16_at(self.b, 0)
    }
    fn dport(&self) -> u16 {
        u16_at(self.b, 2)
    }
    fn seq(&self) -> u32 {
        u32_at(self.b, 4)
    }
    fn ack(&self) -> u32 {
        u32_at(self.b, 8)
    }
    fn flags(&self) -> u8 {
        self.b[13]
    }
    fn window(&self) -> u16 {
        u16_at(self.b, 14)
    }
    fn payload(&self) -> &'a [u8] {
        &self.b[self.off..]
    }
    fn options(&self) -> R<Vec<TcpOpt>> {
        let (mut o, mut out) = (&self.b[20..self.off], Vec::new());
        while let Some(&kind) = o.first() {
            match kind {
                0 => {
                    out.push(TcpOpt::Eol);
                    break;
                }
                1 => {
                    out.push(TcpOpt::Nop);
                    o = &o[1..];
                }
                _ => {
                    need(o, 2, "tcp option length")?;
                    let len = o[1] as usize;
                    if len < 2 || len > o.len() {
                        return Err(ParseError::Malformed("bad tcp option length"));
                    }
                    let body = &o[2..len];
                    out.push(match (kind, len) {
                        (2, 4) => TcpOpt::Mss(u16_at(body, 0)),
                        (3, 3) => TcpOpt::WindowScale(body[0]),
                        (4, 2) => TcpOpt::SackPermitted,
                        (5, l) if (l - 2) % 8 == 0 => TcpOpt::Sack(
                            body.chunks_exact(8)
                                .map(|c| (u32_at(c, 0), u32_at(c, 4)))
                                .collect(),
                        ),
                        (8, 10) => TcpOpt::Timestamps(u32_at(body, 0), u32_at(body, 4)),
                        _ => TcpOpt::Unknown(kind),
                    });
                    o = &o[len..];
                }
            }
        }
        Ok(out)
    }
}

struct Udp<'a> {
    b: &'a [u8],
}
impl<'a> Udp<'a> {
    fn parse(b: &'a [u8]) -> R<Udp<'a>> {
        need(b, 8, "udp header")?;
        let len = u16_at(b, 4) as usize;
        if len < 8 {
            return Err(ParseError::Malformed("udp length < 8"));
        }
        need(b, len, "udp length")?;
        Ok(Udp { b: &b[..len] })
    }
    fn sport(&self) -> u16 {
        u16_at(self.b, 0)
    }
    fn dport(&self) -> u16 {
        u16_at(self.b, 2)
    }
    fn payload(&self) -> &'a [u8] {
        &self.b[8..]
    }
    fn checksum_field(&self) -> u16 {
        u16_at(self.b, 6)
    }
}

// ----------------------------------------------------------------------------------------------- builders
fn eth_frame(
    dst: [u8; 6],
    src: [u8; 6],
    vlan: Option<u16>,
    ethertype: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut f = [dst, src].concat();
    if let Some(v) = vlan {
        f.extend(ETH_VLAN.to_be_bytes());
        f.extend(v.to_be_bytes());
    }
    f.extend(ethertype.to_be_bytes());
    f.extend(payload);
    f
}

fn ipv4_packet(
    proto: u8,
    src: Ipv4Addr,
    dst: Ipv4Addr,
    id: u16,
    flags_frag: u16,
    ttl: u8,
    options: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    assert!(options.len() % 4 == 0 && options.len() <= 40);
    let ihl = 20 + options.len();
    let mut p = vec![0x40 | (ihl / 4) as u8, 0];
    p.extend(((ihl + payload.len()) as u16).to_be_bytes());
    p.extend(id.to_be_bytes());
    p.extend(flags_frag.to_be_bytes());
    p.extend([ttl, proto, 0, 0]);
    p.extend(src.octets());
    p.extend(dst.octets());
    p.extend(options);
    let c = checksum(&[&p]);
    p[10..12].copy_from_slice(&c.to_be_bytes());
    p.extend(payload);
    p
}

fn ipv6_packet(next: u8, src: Ipv6Addr, dst: Ipv6Addr, hop: u8, payload: &[u8]) -> Vec<u8> {
    let mut p = vec![0x60, 0, 0, 0];
    p.extend((payload.len() as u16).to_be_bytes());
    p.extend([next, hop]);
    p.extend(src.octets());
    p.extend(dst.octets());
    p.extend(payload);
    p
}

fn tcp_segment(
    src: &IpAddr,
    dst: &IpAddr,
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    win: u16,
    opts: &[TcpOpt],
    payload: &[u8],
) -> Vec<u8> {
    let mut o = Vec::new();
    for opt in opts {
        match opt {
            TcpOpt::Mss(m) => o.extend([2, 4, (m >> 8) as u8, *m as u8]),
            TcpOpt::WindowScale(s) => o.extend([3, 3, *s]),
            TcpOpt::SackPermitted => o.extend([4, 2]),
            TcpOpt::Timestamps(a, b) => {
                o.extend([8, 10]);
                o.extend(a.to_be_bytes());
                o.extend(b.to_be_bytes());
            }
            TcpOpt::Sack(blocks) => {
                o.extend([5, (2 + 8 * blocks.len()) as u8]);
                for (l, r) in blocks {
                    o.extend(l.to_be_bytes());
                    o.extend(r.to_be_bytes());
                }
            }
            TcpOpt::Nop => o.push(1),
            TcpOpt::Eol => o.push(0),
            TcpOpt::Unknown(_) => {}
        }
    }
    while o.len() % 4 != 0 {
        o.push(1);
    }
    let mut s = Vec::new();
    s.extend(sport.to_be_bytes());
    s.extend(dport.to_be_bytes());
    s.extend(seq.to_be_bytes());
    s.extend(ack.to_be_bytes());
    s.extend([((20 + o.len()) / 4 << 4) as u8, flags]);
    s.extend(win.to_be_bytes());
    s.extend([0, 0, 0, 0]);
    s.extend(&o);
    s.extend(payload);
    let c = l4_checksum(src, dst, 6, &s);
    s[16..18].copy_from_slice(&c.to_be_bytes());
    s
}

fn udp_datagram(src: &IpAddr, dst: &IpAddr, sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let mut s = [
        sport.to_be_bytes(),
        dport.to_be_bytes(),
        ((8 + payload.len()) as u16).to_be_bytes(),
        [0, 0],
    ]
    .concat();
    s.extend(payload);
    let c = match l4_checksum(src, dst, 17, &s) {
        0 => 0xffff, // 0 means "no checksum" in UDP, so a computed 0 is sent as all ones
        c => c,
    };
    s[6..8].copy_from_slice(&c.to_be_bytes());
    s
}

/// ICMPv4 (proto 1, no pseudo-header) or ICMPv6 (proto 58, with pseudo-header) echo request/reply.
fn icmp_echo(
    src: &IpAddr,
    dst: &IpAddr,
    reply: bool,
    id: u16,
    seq: u16,
    payload: &[u8],
) -> Vec<u8> {
    let ty = match (src.is_ipv4(), reply) {
        (true, false) => 8,
        (true, true) => 0,
        (false, false) => 128,
        (false, true) => 129,
    };
    let mut m = vec![ty, 0, 0, 0];
    m.extend(id.to_be_bytes());
    m.extend(seq.to_be_bytes());
    m.extend(payload);
    let c = if src.is_ipv4() {
        checksum(&[&m])
    } else {
        l4_checksum(src, dst, 58, &m)
    };
    m[2..4].copy_from_slice(&c.to_be_bytes());
    m
}

fn arp_packet(op: u16, sha: [u8; 6], spa: Ipv4Addr, tha: [u8; 6], tpa: Ipv4Addr) -> Vec<u8> {
    let mut p = vec![0, 1, 8, 0, 6, 4];
    p.extend(op.to_be_bytes());
    p.extend(sha);
    p.extend(spa.octets());
    p.extend(tha);
    p.extend(tpa.octets());
    p
}

// ------------------------------------------------------------------------------------------- dissector
#[derive(Debug, Clone, PartialEq)]
enum L4 {
    Tcp {
        sport: u16,
        dport: u16,
        seq: u32,
        ack: u32,
        flags: u8,
        win: u16,
        opts: Vec<TcpOpt>,
        payload_len: usize,
        csum_ok: bool,
    },
    Udp {
        sport: u16,
        dport: u16,
        payload_len: usize,
        csum_ok: bool,
    },
    Echo {
        reply: bool,
        id: u16,
        seq: u16,
        csum_ok: bool,
    },
    Icmp {
        ty: u8,
        code: u8,
    },
    Fragment,
    Other,
}
#[derive(Debug, Clone, PartialEq)]
struct IpInfo {
    src: IpAddr,
    dst: IpAddr,
    proto: u8,
    ttl: u8,
    id: u32,
    frag: Option<(usize, bool)>, // (offset bytes, more fragments)
    hdr_csum_ok: Option<bool>,   // IPv4 only
    l4: L4,
}
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Arp {
        op: u16,
        spa: Ipv4Addr,
        tpa: Ipv4Addr,
    },
    Ip(IpInfo),
    Other(u16),
}
#[derive(Debug, Clone, PartialEq)]
struct Pkt {
    vlans: Vec<u16>,
    kind: Kind,
}

fn dissect_l4(src: IpAddr, dst: IpAddr, proto: u8, b: &[u8]) -> R<L4> {
    Ok(match proto {
        6 => {
            let t = Tcp::parse(b)?;
            L4::Tcp {
                sport: t.sport(),
                dport: t.dport(),
                seq: t.seq(),
                ack: t.ack(),
                flags: t.flags(),
                win: t.window(),
                opts: t.options()?,
                payload_len: t.payload().len(),
                csum_ok: l4_verify(&src, &dst, 6, b),
            }
        }
        17 => {
            let u = Udp::parse(b)?;
            let ok = (src.is_ipv4() && u.checksum_field() == 0) || l4_verify(&src, &dst, 17, u.b);
            L4::Udp {
                sport: u.sport(),
                dport: u.dport(),
                payload_len: u.payload().len(),
                csum_ok: ok,
            }
        }
        1 | 58 => {
            need(b, 8, "icmp header")?;
            let (ty, code) = (b[0], b[1]);
            let ok = if proto == 1 {
                fold(sum16(b)) == 0xffff
            } else {
                l4_verify(&src, &dst, 58, b)
            };
            match (proto, ty) {
                (1, 0 | 8) | (58, 128 | 129) => L4::Echo {
                    reply: matches!(ty, 0 | 129),
                    id: u16_at(b, 4),
                    seq: u16_at(b, 6),
                    csum_ok: ok,
                },
                _ => L4::Icmp { ty, code },
            }
        }
        _ => L4::Other,
    })
}

fn dissect(frame: &[u8]) -> R<Pkt> {
    let eth = Eth::parse(frame)?;
    let kind = match eth.ethertype {
        ETH_ARP => {
            let a = Arp::parse(eth.payload)?;
            Kind::Arp {
                op: a.op,
                spa: a.spa,
                tpa: a.tpa,
            }
        }
        ETH_IPV4 => {
            let ip = Ipv4::parse(eth.payload)?;
            let (src, dst) = (IpAddr::V4(ip.src()), IpAddr::V4(ip.dst()));
            let l4 = if ip.is_fragment() {
                L4::Fragment
            } else {
                dissect_l4(src, dst, ip.proto(), ip.payload)?
            };
            Kind::Ip(IpInfo {
                src,
                dst,
                proto: ip.proto(),
                ttl: ip.ttl(),
                id: ip.id() as u32,
                frag: ip.is_fragment().then(|| (ip.frag_offset_bytes(), ip.mf())),
                hdr_csum_ok: Some(ip.checksum_ok()),
                l4,
            })
        }
        ETH_IPV6 => {
            let ip = Ipv6::parse(eth.payload)?;
            let w = ip.walk()?;
            let (src, dst) = (IpAddr::V6(ip.src()), IpAddr::V6(ip.dst()));
            let frag = w.fragment.filter(|f| f.0 != 0 || f.1);
            let l4 = if frag.is_some() {
                L4::Fragment
            } else {
                dissect_l4(src, dst, w.upper_proto, w.upper)?
            };
            Kind::Ip(IpInfo {
                src,
                dst,
                proto: w.upper_proto,
                ttl: ip.hdr[7],
                id: w.fragment.map_or(0, |f| f.2),
                frag: frag.map(|f| (f.0, f.1)),
                hdr_csum_ok: None,
                l4,
            })
        }
        other => Kind::Other(other),
    };
    Ok(Pkt {
        vlans: eth.vlans,
        kind,
    })
}

fn flags_str(f: u8) -> String {
    let mut s = String::new();
    for (bit, c) in [(SYN, 'S'), (FIN, 'F'), (RST, 'R'), (PSH, 'P')] {
        if f & bit != 0 {
            s.push(c);
        }
    }
    if f & ACK != 0 {
        s.push('.');
    }
    if s.is_empty() { "none".into() } else { s }
}

/// A tcpdump-like one-line description.
fn describe(p: &Pkt) -> String {
    let vlan = p
        .vlans
        .first()
        .map_or(String::new(), |v| format!("vlan {v}, "));
    match &p.kind {
        Kind::Arp { op: 1, spa, tpa } => format!("{vlan}ARP, Request who-has {tpa} tell {spa}"),
        Kind::Arp { op, spa, tpa } => format!("{vlan}ARP, op {op} {spa} -> {tpa}"),
        Kind::Other(t) => format!("{vlan}ethertype 0x{t:04x}"),
        Kind::Ip(i) => {
            let v = if i.src.is_ipv4() { "IP" } else { "IP6" };
            let (sp, dp) = match &i.l4 {
                L4::Tcp { sport, dport, .. } | L4::Udp { sport, dport, .. } => {
                    (format!(".{sport}"), format!(".{dport}"))
                }
                _ => (String::new(), String::new()),
            };
            let head = format!("{vlan}{v} {}{sp} > {}{dp}:", i.src, i.dst);
            match &i.l4 {
                L4::Tcp {
                    flags,
                    seq,
                    ack,
                    win,
                    opts,
                    payload_len,
                    csum_ok,
                    ..
                } => {
                    let o: Vec<String> = opts
                        .iter()
                        .filter(|o| **o != TcpOpt::Nop)
                        .map(|o| match o {
                            TcpOpt::Mss(m) => format!("mss {m}"),
                            TcpOpt::WindowScale(s) => format!("wscale {s}"),
                            TcpOpt::SackPermitted => "sackOK".into(),
                            TcpOpt::Timestamps(a, b) => format!("TS val {a} ecr {b}"),
                            TcpOpt::Sack(b) => format!(
                                "sack {}",
                                b.iter()
                                    .map(|(l, r)| format!("{l}:{r}"))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            ),
                            other => format!("{other:?}"),
                        })
                        .collect();
                    let o = if o.is_empty() {
                        String::new()
                    } else {
                        format!(", options [{}]", o.join(","))
                    };
                    format!(
                        "{head} Flags [{}], seq {seq}, ack {ack}, win {win}{o}, length {payload_len}{}",
                        flags_str(*flags),
                        if *csum_ok { "" } else { " (bad cksum!)" }
                    )
                }
                L4::Udp {
                    payload_len,
                    csum_ok,
                    ..
                } => format!(
                    "{head} UDP, length {payload_len}{}",
                    if *csum_ok { "" } else { " (bad cksum!)" }
                ),
                L4::Echo {
                    reply,
                    id,
                    seq,
                    csum_ok,
                } => format!(
                    "{head} ICMP echo {}, id {id}, seq {seq}{}",
                    if *reply { "reply" } else { "request" },
                    if *csum_ok { "" } else { " (bad cksum!)" }
                ),
                L4::Icmp { ty, code } => format!("{head} ICMP type {ty} code {code}"),
                L4::Fragment => {
                    let (off, mf) = i.frag.unwrap();
                    format!(
                        "{head} frag (id {}, off {off}{})",
                        i.id,
                        if mf { ", MF" } else { "" }
                    )
                }
                L4::Other => format!("{head} proto {}", i.proto),
            }
        }
    }
}

// ----------------------------------------------------------------------------------- fragment/reassembly
fn fragment_v4(packet: &[u8], mtu: usize) -> Vec<Vec<u8>> {
    let ip = Ipv4::parse(packet).unwrap();
    assert!(
        ip.hdr.len() == 20 && mtu >= 28,
        "this demo fragments option-less packets"
    );
    let per = (mtu - 20) / 8 * 8;
    let mut out = Vec::new();
    for (i, chunk) in ip.payload.chunks(per).enumerate() {
        let last = (i + 1) * per >= ip.payload.len();
        let ff = ((i * per / 8) as u16) | if last { 0 } else { 0x2000 };
        out.push(ipv4_packet(
            ip.proto(),
            ip.src(),
            ip.dst(),
            ip.id(),
            ff,
            ip.ttl(),
            &[],
            chunk,
        ));
    }
    out
}

#[derive(Debug, PartialEq)]
enum Reasm {
    Pending,
    Complete(Vec<u8>), // a full IPv4 packet, fragment fields cleared, checksum recomputed
    Rejected(&'static str),
}

#[derive(Default)]
struct Datagram {
    frags: BTreeMap<usize, Vec<u8>>,
    total: Option<usize>,
    first_hdr: Option<Vec<u8>>,
}

struct Reassembler {
    table: HashMap<(Ipv4Addr, Ipv4Addr, u16, u8), Datagram>,
    order: VecDeque<(Ipv4Addr, Ipv4Addr, u16, u8)>,
    max_datagrams: usize,
    max_frags: usize,
    evicted: usize,
}

impl Reassembler {
    fn new(max_datagrams: usize, max_frags: usize) -> Self {
        Reassembler {
            table: HashMap::new(),
            order: VecDeque::new(),
            max_datagrams,
            max_frags,
            evicted: 0,
        }
    }
    fn drop_key(&mut self, k: &(Ipv4Addr, Ipv4Addr, u16, u8)) {
        self.table.remove(k);
        self.order.retain(|x| x != k);
    }
    /// Feed one IPv4 packet that is a fragment. Policy: exact duplicates are ignored; ANY other overlap, an
    /// inconsistent end, or a size over 65535 drops the whole datagram (so the attacker's overlap cannot pick
    /// which bytes win, and nothing is ever corrupted).
    fn add(&mut self, packet: &[u8]) -> Reasm {
        let ip = match Ipv4::parse(packet) {
            Ok(ip) => ip,
            Err(_) => return Reasm::Rejected("unparseable"),
        };
        let (off, data) = (ip.frag_offset_bytes(), ip.payload);
        let end = off + data.len();
        if data.is_empty() {
            return Reasm::Rejected("empty fragment");
        }
        if end > 65535 - 20 {
            return Reasm::Rejected("reassembled size over 65535 (ping of death)");
        }
        if ip.mf() && data.len() % 8 != 0 {
            return Reasm::Rejected("non-final fragment not a multiple of 8");
        }
        let key = (ip.src(), ip.dst(), ip.id(), ip.proto());
        if !self.table.contains_key(&key) {
            if self.table.len() >= self.max_datagrams {
                if let Some(old) = self.order.pop_front() {
                    self.table.remove(&old);
                    self.evicted += 1;
                }
            }
            self.order.push_back(key);
            self.table.insert(key, Datagram::default());
        }
        let d = self.table.get_mut(&key).unwrap();
        for (&o, existing) in d.frags.range(..end).rev() {
            let o_end = o + existing.len();
            if o_end <= off {
                break; // stored fragments never overlap each other, so nothing further left can overlap
            }
            if off >= o && end <= o_end && existing[off - o..end - o] == *data {
                return Reasm::Pending; // exact duplicate (or sub-range of one): ignore
            }
            self.drop_key(&key);
            return Reasm::Rejected("overlapping fragments (teardrop-style): datagram dropped");
        }
        if !ip.mf() {
            if d.total.is_some_and(|t| t != end)
                || d.frags
                    .keys()
                    .next_back()
                    .is_some_and(|&o| o + d.frags[&o].len() > end)
            {
                self.drop_key(&key);
                return Reasm::Rejected("conflicting end of datagram");
            }
            d.total = Some(end);
        } else if d.total.is_some_and(|t| end > t) {
            self.drop_key(&key);
            return Reasm::Rejected("fragment beyond declared end");
        }
        if d.frags.len() >= self.max_frags {
            self.drop_key(&key);
            return Reasm::Rejected("too many fragments");
        }
        if off == 0 {
            d.first_hdr = Some(ip.hdr.to_vec());
        }
        d.frags.insert(off, data.to_vec());
        let Some(total) = d.total else {
            return Reasm::Pending;
        };
        let mut next = 0;
        for (&o, f) in &d.frags {
            if o != next {
                return Reasm::Pending;
            }
            next = o + f.len();
        }
        if next != total || d.first_hdr.is_none() {
            return Reasm::Pending;
        }
        let payload: Vec<u8> = d.frags.values().flatten().copied().collect();
        let h = d.first_hdr.clone().unwrap();
        let out = ipv4_packet(
            h[9],
            Ipv4Addr::new(h[12], h[13], h[14], h[15]),
            Ipv4Addr::new(h[16], h[17], h[18], h[19]),
            u16_at(&h, 4),
            0,
            h[8],
            &h[20..],
            &payload,
        );
        self.drop_key(&key);
        Reasm::Complete(out)
    }
}

// -------------------------------------------------------------------------------------------------- pcap
struct PcapWriter {
    buf: Vec<u8>,
    be: bool,
    nanos: bool,
}
impl PcapWriter {
    fn new(be: bool, nanos: bool, snaplen: u32, linktype: u32) -> Self {
        let mut w = PcapWriter {
            buf: Vec::new(),
            be,
            nanos,
        };
        let magic = if nanos { 0xa1b2_3c4d } else { 0xa1b2_c3d4 };
        w.u32(magic);
        w.u16(2);
        w.u16(4);
        w.u32(0); // thiszone
        w.u32(0); // sigfigs
        w.u32(snaplen);
        w.u32(linktype);
        w
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend(if self.be {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        });
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend(if self.be {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        });
    }
    /// `frac` is microseconds, or nanoseconds for a nanosecond file.
    fn packet(&mut self, sec: u32, frac: u32, data: &[u8], orig_len: u32) {
        self.u32(sec);
        self.u32(frac);
        self.u32(data.len() as u32);
        self.u32(orig_len);
        self.buf.extend(data);
    }
}

struct PcapRecord<'a> {
    sec: u32,
    nsec: u32, // normalised to nanoseconds whatever the file used
    orig_len: u32,
    data: &'a [u8],
}
struct PcapReader<'a> {
    d: &'a [u8],
    pos: usize,
    be: bool,
    snaplen: u32,
    linktype: u32,
    nanos: bool,
    failed: bool,
}
impl<'a> PcapReader<'a> {
    fn open(d: &'a [u8]) -> R<PcapReader<'a>> {
        need(d, 24, "pcap global header")?;
        let (be, nanos) = match u32::from_le_bytes(d[0..4].try_into().unwrap()) {
            0xa1b2_c3d4 => (false, false),
            0xa1b2_3c4d => (false, true),
            0xd4c3_b2a1 => (true, false),
            0x4d3c_b2a1 => (true, true),
            _ => return Err(ParseError::Malformed("not a pcap file (bad magic)")),
        };
        let rd = |i: usize| {
            if be {
                u32::from_be_bytes(d[i..i + 4].try_into().unwrap())
            } else {
                u32::from_le_bytes(d[i..i + 4].try_into().unwrap())
            }
        };
        Ok(PcapReader {
            d,
            pos: 24,
            be,
            snaplen: rd(16),
            linktype: rd(20),
            nanos,
            failed: false,
        })
    }
}
impl<'a> Iterator for PcapReader<'a> {
    type Item = R<PcapRecord<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.pos == self.d.len() {
            return None;
        }
        let rd = |s: &[u8], i: usize| {
            if self.be {
                u32::from_be_bytes(s[i..i + 4].try_into().unwrap())
            } else {
                u32::from_le_bytes(s[i..i + 4].try_into().unwrap())
            }
        };
        let rest = &self.d[self.pos..];
        let res = (|| {
            need(rest, 16, "pcap record header")?;
            let (sec, frac, incl, orig) =
                (rd(rest, 0), rd(rest, 4), rd(rest, 8) as usize, rd(rest, 12));
            if incl > 262_144 || incl > orig as usize {
                return Err(ParseError::Malformed("implausible captured length"));
            }
            need(&rest[16..], incl, "pcap record body")?;
            let nsec = if self.nanos {
                frac
            } else {
                frac.checked_mul(1000)
                    .ok_or(ParseError::Malformed("bad usec"))?
            };
            if nsec >= 1_000_000_000 {
                return Err(ParseError::Malformed("fractional seconds out of range"));
            }
            Ok((
                PcapRecord {
                    sec,
                    nsec,
                    orig_len: orig,
                    data: &rest[16..16 + incl],
                },
                16 + incl,
            ))
        })();
        match res {
            Ok((r, used)) => {
                self.pos += used;
                Some(Ok(r))
            }
            Err(e) => {
                self.failed = true;
                Some(Err(e))
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------- flow table
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TcpState {
    SynSent,
    SynRecv,
    Established,
    Closing,
    Closed,
    Reset,
}
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
struct FlowKey {
    a: (IpAddr, u16),
    b: (IpAddr, u16),
    proto: u8,
}
struct Flow {
    initiator: (IpAddr, u16),
    state: Option<TcpState>, // None for UDP
    packets: [u64; 2],       // [initiator -> responder, responder -> initiator]
    bytes: [u64; 2],
    fins: [bool; 2],
}
struct FlowTable {
    flows: HashMap<FlowKey, Flow>,
    max: usize,
    refused: usize,
}
impl FlowTable {
    fn new(max: usize) -> Self {
        FlowTable {
            flows: HashMap::new(),
            max,
            refused: 0,
        }
    }
    fn update(&mut self, i: &IpInfo) {
        let (sp, dp, flags, plen) = match &i.l4 {
            L4::Tcp {
                sport,
                dport,
                flags,
                payload_len,
                ..
            } => (*sport, *dport, Some(*flags), *payload_len),
            L4::Udp {
                sport,
                dport,
                payload_len,
                ..
            } => (*sport, *dport, None, *payload_len),
            _ => return,
        };
        let (s, d) = ((i.src, sp), (i.dst, dp));
        let key = if s <= d {
            FlowKey {
                a: s,
                b: d,
                proto: i.proto,
            }
        } else {
            FlowKey {
                a: d,
                b: s,
                proto: i.proto,
            }
        };
        if !self.flows.contains_key(&key) && self.flows.len() >= self.max {
            self.refused += 1;
            return;
        }
        let f = self.flows.entry(key).or_insert_with(|| Flow {
            initiator: s,
            state: flags.map(|_| TcpState::SynSent),
            packets: [0; 2],
            bytes: [0; 2],
            fins: [false; 2],
        });
        let dir = if s == f.initiator { 0 } else { 1 };
        f.packets[dir] += 1;
        f.bytes[dir] += plen as u64;
        let Some(fl) = flags else { return };
        let st = f.state.unwrap();
        let (syn, ack, fin, rst) = (fl & SYN != 0, fl & ACK != 0, fl & FIN != 0, fl & RST != 0);
        if fin {
            f.fins[dir] = true;
        }
        f.state = Some(match st {
            _ if rst => TcpState::Reset,
            TcpState::SynSent if syn && ack && dir == 1 => TcpState::SynRecv,
            TcpState::SynSent if !syn && ack && f.packets[0] == 1 && f.packets[1] == 0 => {
                TcpState::Established
            } // picked up midstream
            TcpState::SynRecv if ack && !syn && dir == 0 => TcpState::Established,
            TcpState::Established if fin => TcpState::Closing,
            TcpState::Closing if f.fins == [true, true] && ack && !fin => TcpState::Closed,
            other => other,
        });
    }
}

// ----------------------------------------------------------------------------------------------- scenario
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const MAC_A: [u8; 6] = [2, 0, 0, 0, 0, 1];
const MAC_B: [u8; 6] = [2, 0, 0, 0, 0, 2];
const A4: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
const B4: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 2);

fn tcp4(
    sport: u16,
    dport: u16,
    from_a: bool,
    seq: u32,
    ack: u32,
    flags: u8,
    opts: &[TcpOpt],
    payload: &[u8],
    id: u16,
) -> Vec<u8> {
    let (s, d, ms, md) = if from_a {
        (A4, B4, MAC_A, MAC_B)
    } else {
        (B4, A4, MAC_B, MAC_A)
    };
    let (sp, dp) = if from_a {
        (sport, dport)
    } else {
        (dport, sport)
    };
    let seg = tcp_segment(
        &IpAddr::V4(s),
        &IpAddr::V4(d),
        sp,
        dp,
        seq,
        ack,
        flags,
        64240,
        opts,
        payload,
    );
    eth_frame(
        md,
        ms,
        Some(100),
        ETH_IPV4,
        &ipv4_packet(6, s, d, id, 0x4000, 64, &[], &seg),
    )
}

fn scenario() -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut f: Vec<Vec<u8>> = Vec::new();
    let (a, b) = (IpAddr::V4(A4), IpAddr::V4(B4));
    // ARP
    f.push(eth_frame(
        [0xff; 6],
        MAC_A,
        None,
        ETH_ARP,
        &arp_packet(1, MAC_A, A4, [0; 6], B4),
    ));
    f.push(eth_frame(
        MAC_A,
        MAC_B,
        None,
        ETH_ARP,
        &arp_packet(2, MAC_B, B4, MAC_A, A4),
    ));
    // TCP in VLAN 100: handshake, request, response, close
    let syn_opts = [
        TcpOpt::Mss(1460),
        TcpOpt::SackPermitted,
        TcpOpt::Timestamps(1000, 0),
        TcpOpt::Nop,
        TcpOpt::WindowScale(7),
    ];
    let req = b"GET / HTTP/1.0\r\n\r\n";
    let resp = vec![b'x'; 100];
    f.push(tcp4(50000, 80, true, 1000, 0, SYN, &syn_opts, &[], 1));
    f.push(tcp4(
        50000,
        80,
        false,
        7000,
        1001,
        SYN | ACK,
        &[
            TcpOpt::Mss(1460),
            TcpOpt::SackPermitted,
            TcpOpt::Timestamps(2000, 1000),
            TcpOpt::Nop,
            TcpOpt::WindowScale(8),
        ],
        &[],
        1,
    ));
    f.push(tcp4(50000, 80, true, 1001, 7001, ACK, &[], &[], 2));
    f.push(tcp4(50000, 80, true, 1001, 7001, PSH | ACK, &[], req, 3));
    f.push(tcp4(
        50000,
        80,
        false,
        7001,
        1001 + req.len() as u32,
        PSH | ACK,
        &[TcpOpt::Sack(vec![(5, 9)])],
        &resp,
        2,
    ));
    f.push(tcp4(
        50000,
        80,
        true,
        1001 + req.len() as u32,
        7101,
        FIN | ACK,
        &[],
        &[],
        4,
    ));
    f.push(tcp4(
        50000,
        80,
        false,
        7101,
        1002 + req.len() as u32,
        FIN | ACK,
        &[],
        &[],
        3,
    ));
    f.push(tcp4(
        50000,
        80,
        true,
        1002 + req.len() as u32,
        7102,
        ACK,
        &[],
        &[],
        5,
    ));
    // ICMP echo
    for (reply, seq) in [(false, 1), (true, 1)] {
        let (s, d, ms, md) = if reply {
            (B4, A4, MAC_B, MAC_A)
        } else {
            (A4, B4, MAC_A, MAC_B)
        };
        let m = icmp_echo(
            &IpAddr::V4(s),
            &IpAddr::V4(d),
            reply,
            0x1234,
            seq,
            b"abcdefgh",
        );
        f.push(eth_frame(
            md,
            ms,
            None,
            ETH_IPV4,
            &ipv4_packet(1, s, d, 100 + seq, 0, 64, &[], &m),
        ));
    }
    // UDP query and response (with an IPv4 option: 4 NOPs-equivalent padding, to exercise IHL > 5)
    let q = udp_datagram(&a, &b, 40000, 53, b"\x12\x34\x01\x00\x00\x01example");
    f.push(eth_frame(
        MAC_B,
        MAC_A,
        None,
        ETH_IPV4,
        &ipv4_packet(17, A4, B4, 200, 0, 64, &[1, 1, 1, 0], &q),
    ));
    let r = udp_datagram(&b, &a, 53, 40000, b"\x12\x34\x81\x80reply-bytes");
    f.push(eth_frame(
        MAC_A,
        MAC_B,
        None,
        ETH_IPV4,
        &ipv4_packet(17, B4, A4, 201, 0, 64, &[], &r),
    ));
    // IPv6: TCP SYN behind a destination-options header, then ICMPv6 echo
    let (a6, b6) = (
        Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1),
        Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2),
    );
    let syn6 = tcp_segment(
        &IpAddr::V6(a6),
        &IpAddr::V6(b6),
        51000,
        443,
        42,
        0,
        SYN,
        65535,
        &[TcpOpt::Mss(1440), TcpOpt::SackPermitted],
        &[],
    );
    let dstopts = [&[6u8, 0, 1, 4, 0, 0, 0, 0][..], &syn6].concat(); // next=TCP, hdr ext len 0, PadN option
    f.push(eth_frame(
        MAC_B,
        MAC_A,
        None,
        ETH_IPV6,
        &ipv6_packet(60, a6, b6, 64, &dstopts),
    ));
    let e6 = icmp_echo(&IpAddr::V6(a6), &IpAddr::V6(b6), false, 7, 1, b"ping6");
    f.push(eth_frame(
        MAC_B,
        MAC_A,
        None,
        ETH_IPV6,
        &ipv6_packet(58, a6, b6, 64, &e6),
    ));
    // A 3000-byte UDP datagram fragmented at MTU 1500, delivered out of order with a duplicate
    let big: Vec<u8> = (0..3000u32).map(|i| (i * 7 + 3) as u8).collect();
    let udp = udp_datagram(&a, &b, 6000, 7000, &big);
    let whole = ipv4_packet(17, A4, B4, 777, 0, 64, &[], &udp);
    let fr = fragment_v4(&whole, 1500);
    assert_eq!(fr.len(), 3);
    for i in [2usize, 0, 1, 0] {
        f.push(eth_frame(MAC_B, MAC_A, None, ETH_IPV4, &fr[i]));
    }
    (f, whole)
}

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut v = base.to_vec();
    match rng.below(5) {
        0 => {
            for _ in 0..1 + rng.below(4) {
                if !v.is_empty() {
                    let i = rng.below(v.len());
                    v[i] ^= 1 << rng.below(8);
                }
            }
        }
        1 => v.truncate(rng.below(v.len() + 1)),
        2 => {
            for _ in 0..rng.below(20) {
                v.push(rng.next() as u8);
            }
        }
        3 => {
            if !v.is_empty() {
                let i = rng.below(v.len());
                v[i] = rng.next() as u8;
                let j = rng.below(v.len());
                v[j] = [0, 0xff, 0x80][rng.below(3)];
            }
        }
        _ => {
            let i = rng.below(v.len() + 1);
            let n = rng.below(8);
            v.splice(i..i, (0..n).map(|_| rng.next() as u8));
        }
    }
    v
}

fn main() {
    // ---- 1. checksum vectors -------------------------------------------------------------------------
    let rfc1071 = [0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
    assert_eq!(
        fold(sum16(&rfc1071)),
        0xddf2,
        "RFC 1071 section 3 example: sum is ddf2"
    );
    assert_eq!(checksum(&[&rfc1071]), 0x220d);
    // A real, widely published IPv4 header (4500 0073 0000 4000 4011 [b861] c0a8 0001 c0a8 00c7)
    let hdr = [
        0x45, 0, 0, 0x73, 0, 0, 0x40, 0, 0x40, 0x11, 0, 0, 0xc0, 0xa8, 0, 1, 0xc0, 0xa8, 0, 0xc7,
    ];
    assert_eq!(checksum(&[&hdr]), 0xb861);
    let rebuilt = ipv4_packet(
        17,
        Ipv4Addr::new(192, 168, 0, 1),
        Ipv4Addr::new(192, 168, 0, 199),
        0,
        0x4000,
        64,
        &[],
        &[0; 0x73 - 20],
    );
    assert_eq!(
        &rebuilt[10..12],
        &[0xb8, 0x61],
        "builder reproduces the published header checksum"
    );
    assert!(Ipv4::parse(&rebuilt).unwrap().checksum_ok());
    // odd-length data and chunked == whole
    assert_eq!(checksum(&[&[1, 2, 3]]), !0x0402u16 & 0xffff);
    println!(
        "-- checksums: RFC 1071 example sum 0xddf2 -> checksum 0x220d; published IPv4 header -> 0xb861"
    );

    // RFC 1624 incremental update == full recompute (router decrements TTL)
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..2000 {
        let mut h: Vec<u8> = (0..20).map(|_| rng.next() as u8).collect();
        h[0] = 0x45;
        h[10] = 0;
        h[11] = 0;
        let c = checksum(&[&h]);
        let old = u16_at(&h, 8); // TTL|protocol word
        h[8] = h[8].wrapping_sub(1);
        let new = u16_at(&h, 8);
        let inc = csum_update(c, old, new);
        let full = checksum(&[&h]);
        assert!(
            inc == full || (inc == 0 && full == 0xffff) || (inc == 0xffff && full == 0),
            "incremental {inc:04x} vs full {full:04x}"
        );
    }
    println!(
        "-- RFC 1624 incremental update matched a full recompute on 2000 random headers (TTL decrement)"
    );

    // ---- 2. build + dissect ---------------------------------------------------------------------------
    let (frames, whole_frag_pkt) = scenario();
    let pkts: Vec<Pkt> = frames
        .iter()
        .map(|f| dissect(f).expect("scenario frames are valid"))
        .collect();
    for p in &pkts {
        if let Kind::Ip(i) = &p.kind {
            assert_ne!(i.hdr_csum_ok, Some(false), "IPv4 header checksum");
            match &i.l4 {
                L4::Tcp { csum_ok, .. } | L4::Udp { csum_ok, .. } | L4::Echo { csum_ok, .. } => {
                    assert!(
                        csum_ok,
                        "L4 checksum incl. pseudo-header must verify: {}",
                        describe(p)
                    )
                }
                _ => {}
            }
        }
    }
    // spot checks of parsed fields
    match &pkts[2].kind {
        Kind::Ip(IpInfo {
            l4: L4::Tcp {
                flags, opts, seq, ..
            },
            ttl: 64,
            ..
        }) => {
            assert_eq!(*flags, SYN);
            assert_eq!(*seq, 1000);
            assert!(
                opts.contains(&TcpOpt::Mss(1460))
                    && opts.contains(&TcpOpt::WindowScale(7))
                    && opts.contains(&TcpOpt::Timestamps(1000, 0))
                    && opts.contains(&TcpOpt::SackPermitted)
            );
        }
        k => panic!("unexpected {k:?}"),
    }
    assert_eq!(pkts[2].vlans, vec![100]);
    let udp_with_options = Eth::parse(&frames[12]).unwrap();
    let ip = Ipv4::parse(udp_with_options.payload).unwrap();
    assert_eq!(
        (ip.hdr.len(), ip.options().len()),
        (24, 4),
        "IHL 6: 4 option bytes"
    );
    let v6 = Eth::parse(&frames[14]).unwrap();
    let w = Ipv6::parse(v6.payload).unwrap().walk().unwrap();
    assert_eq!(
        (w.upper_proto, w.ext_headers),
        (6, 1),
        "TCP found behind one destination-options header"
    );
    // a flipped payload bit and a corrupted checksum are detected
    let mut bad = frames[2].clone();
    let last = bad.len() - 1;
    bad[last - 1] ^= 0x01;
    let mut bad_tcp = tcp4(1, 2, true, 1, 1, ACK, &[], b"payload!", 9);
    let n = bad_tcp.len();
    bad_tcp[n - 1] ^= 0x40;
    match dissect(&bad_tcp).unwrap().kind {
        Kind::Ip(IpInfo {
            l4: L4::Tcp { csum_ok, .. },
            ..
        }) => assert!(!csum_ok, "payload corruption must break the TCP checksum"),
        _ => unreachable!(),
    }
    // pseudo-header: the same segment verified against the WRONG destination fails
    let seg = tcp_segment(
        &IpAddr::V4(A4),
        &IpAddr::V4(B4),
        1,
        2,
        3,
        4,
        ACK,
        5,
        &[],
        b"hi",
    );
    assert!(l4_verify(&IpAddr::V4(A4), &IpAddr::V4(B4), 6, &seg));
    assert!(
        !l4_verify(
            &IpAddr::V4(A4),
            &IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)),
            6,
            &seg
        ),
        "mis-delivered segment fails"
    );
    println!(
        "-- dissected {} scenario frames: all checksums (IPv4 header, TCP/UDP pseudo-header, ICMP, ICMPv6) verify; corruption and mis-delivery are caught",
        frames.len()
    );

    // ---- 3. fragmentation / reassembly ---------------------------------------------------------------
    let mut re = Reassembler::new(64, 128);
    let fr = fragment_v4(&whole_frag_pkt, 1500);
    let mut results = Vec::new();
    for i in [2usize, 0, 1, 0] {
        results.push(re.add(&fr[i]));
    }
    assert_eq!(results[0], Reasm::Pending);
    assert_eq!(
        results[2],
        Reasm::Complete(whole_frag_pkt.clone()),
        "out-of-order fragments reassemble to the original packet"
    );
    println!(
        "-- reassembly: fragments arrived as 3,1,2,1(dup): original 3028-byte packet rebuilt byte-for-byte"
    );
    // property test: random sizes / MTUs / orders / duplicates
    for case in 0..400 {
        let n = 1 + rng.below(6000);
        let payload: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        let orig = ipv4_packet(17, A4, B4, case as u16, 0, 64, &[], &payload);
        let mut frs = fragment_v4(&orig, 28 + rng.below(1500));
        for _ in 0..if frs.len() > 1 { rng.below(3) } else { 0 } {
            let i = rng.below(frs.len());
            frs.push(frs[i].clone());
        }
        for i in (1..frs.len()).rev() {
            frs.swap(i, rng.below(i + 1));
        }
        let mut done = None;
        let mut re = Reassembler::new(8, 4096);
        for f in &frs {
            match re.add(f) {
                Reasm::Complete(p) => {
                    // duplicates arriving after completion can start (and finish) a second copy: it must be identical
                    assert!(done.as_ref().map_or(true, |d| *d == p));
                    done = Some(p);
                }
                Reasm::Rejected(why) => panic!("valid fragments rejected: {why}"),
                Reasm::Pending => {}
            }
        }
        assert_eq!(done.expect("must complete"), orig);
    }
    println!(
        "-- reassembly property test: 400 random datagrams x random MTU x shuffle x duplicates -> exact"
    );
    // teardrop / overlap / ping-of-death attacks
    let payload: Vec<u8> = (0..200u8).collect();
    let mk = |off: usize, mf: bool, data: &[u8]| {
        ipv4_packet(
            17,
            A4,
            B4,
            4242,
            ((off / 8) as u16) | if mf { 0x2000 } else { 0 },
            64,
            &[],
            data,
        )
    };
    let mut re = Reassembler::new(64, 128);
    assert_eq!(re.add(&mk(0, true, &payload[..96])), Reasm::Pending);
    let evil = re.add(&mk(40, true, &[0xEE; 96])); // overlaps bytes 40..96 with DIFFERENT data
    assert!(
        matches!(evil, Reasm::Rejected(m) if m.contains("overlap")),
        "{evil:?}"
    );
    assert_eq!(
        re.table.len(),
        0,
        "poisoned datagram dropped, nothing retained"
    );
    // identical overlap (a retransmitted fragment) is tolerated
    assert_eq!(re.add(&mk(0, true, &payload[..96])), Reasm::Pending);
    assert_eq!(re.add(&mk(0, true, &payload[..96])), Reasm::Pending);
    assert!(
        matches!(re.add(&mk(65520, false, &[0; 100])), Reasm::Rejected(m) if m.contains("65535"))
    );
    assert!(
        matches!(re.add(&mk(8, true, &[0; 13])), Reasm::Rejected(_)),
        "MF fragment must be a multiple of 8"
    );
    // fragment flood: table bounded, per-datagram fragments bounded
    let mut re = Reassembler::new(16, 8);
    for id in 0..1000u16 {
        let p = ipv4_packet(17, A4, B4, id, 0x2000 | 1, 64, &[], &[0u8; 8]);
        re.add(&p);
    }
    assert!(re.table.len() <= 16 && re.evicted >= 984);
    println!(
        "-- attacks: teardrop overlap rejected and dropped, ping-of-death size rejected, flood of 1000 datagrams held to a 16-entry table ({} evicted)",
        re.evicted
    );

    // ---- 4. pcap -------------------------------------------------------------------------------------
    for (be, nanos) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut w = PcapWriter::new(be, nanos, 65535, 1);
        for (i, f) in frames.iter().enumerate() {
            w.packet(
                1_700_000_000 + i as u32,
                if nanos { 123_456_789 } else { 123_456 },
                f,
                f.len() as u32,
            );
        }
        let r = PcapReader::open(&w.buf).unwrap();
        assert_eq!(
            (r.be, r.nanos, r.linktype, r.snaplen),
            (be, nanos, 1, 65535)
        );
        let recs: Vec<_> = r.collect::<R<Vec<_>>>().unwrap();
        assert_eq!(recs.len(), frames.len());
        for (i, (rec, f)) in recs.iter().zip(&frames).enumerate() {
            assert_eq!(rec.data, &f[..]);
            assert_eq!(
                (rec.sec, rec.nsec),
                (
                    1_700_000_000 + i as u32,
                    if nanos { 123_456_789 } else { 123_456_000 }
                )
            );
        }
    }
    println!(
        "-- pcap round trip OK for all four variants (little/big endian x microsecond/nanosecond magic)"
    );
    assert!(PcapReader::open(b"not a pcap file at all, just text....").is_err());
    let mut w = PcapWriter::new(false, false, 65535, 1);
    w.packet(1, 2, &[1, 2, 3, 4], 4);
    let cut = &w.buf[..w.buf.len() - 2];
    assert_eq!(
        PcapReader::open(cut).unwrap().next().unwrap().err(),
        Some(ParseError::Truncated("pcap record body"))
    );

    // synthetic capture on disk -> read back -> tcpdump-like summary + flow table
    let mut w = PcapWriter::new(false, false, 65535, 1);
    for (i, f) in frames.iter().enumerate() {
        w.packet(
            1_700_000_000 + (i as u32 / 4),
            (i as u32 % 4) * 250_000 + 123,
            f,
            f.len() as u32,
        );
    }
    let path = std::env::temp_dir().join(format!("cookbook_packets_{}.pcap", std::process::id()));
    std::fs::write(&path, &w.buf).unwrap();
    let data = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(data, w.buf);
    println!(
        "-- tcpdump-style summary of the synthetic capture ({} bytes, {} packets):",
        data.len(),
        frames.len()
    );
    let mut table = FlowTable::new(1000);
    let mut re = Reassembler::new(64, 128);
    for rec in PcapReader::open(&data).unwrap() {
        let rec = rec.unwrap();
        let p = dissect(rec.data).unwrap();
        let t = rec.sec % 86400;
        println!(
            "   {:02}:{:02}:{:02}.{:06} {}",
            t / 3600,
            t / 60 % 60,
            t % 60,
            rec.nsec / 1000,
            describe(&p)
        );
        if let Kind::Ip(i) = &p.kind {
            table.update(i);
            if i.frag.is_some() && i.src.is_ipv4() {
                let eth = Eth::parse(rec.data).unwrap();
                if let Reasm::Complete(full) = re.add(eth.payload) {
                    let ip = Ipv4::parse(&full).unwrap();
                    let u = Udp::parse(ip.payload).unwrap();
                    assert!(l4_verify(
                        &IpAddr::V4(ip.src()),
                        &IpAddr::V4(ip.dst()),
                        17,
                        u.b
                    ));
                    println!(
                        "   ... reassembled datagram id {}: UDP {} > {}, {} payload bytes, checksum OK",
                        i.id,
                        u.sport(),
                        u.dport(),
                        u.payload().len()
                    );
                    assert_eq!(u.payload().len(), 3000);
                }
            }
        }
    }
    println!("-- flow table ({} flows):", table.flows.len());
    let mut keys: Vec<_> = table.flows.keys().cloned().collect();
    keys.sort();
    for k in &keys {
        let f = &table.flows[k];
        println!(
            "   proto {:>2} {}:{} <-> {}:{}  pkts {:?} bytes {:?} state {:?}",
            k.proto, k.a.0, k.a.1, k.b.0, k.b.1, f.packets, f.bytes, f.state
        );
    }
    let web = &table.flows[&FlowKey {
        a: (IpAddr::V4(A4), 50000),
        b: (IpAddr::V4(B4), 80),
        proto: 6,
    }];
    assert_eq!(web.state, Some(TcpState::Closed));
    assert_eq!(
        (web.packets, web.bytes),
        ([5, 3], [18, 100]),
        "both directions of the TCP connection share one flow"
    );
    assert_eq!(
        table.flows.len(),
        3,
        "tcp/v4, udp dns, tcp/v6 (ICMP and fragments carry no ports)"
    );

    // ---- 5. fuzz ------------------------------------------------------------------------------------
    let (mut ok, mut err, mut reasm_ok) = (0u32, 0u32, 0u32);
    let mut table = FlowTable::new(5000);
    let mut re = Reassembler::new(32, 64);
    let mut feed = |buf: &[u8], table: &mut FlowTable, re: &mut Reassembler| match dissect(buf) {
        Ok(p) => {
            ok += 1;
            if let Kind::Ip(i) = &p.kind {
                table.update(i);
                if i.frag.is_some() && i.src.is_ipv4() {
                    if let Ok(eth) = Eth::parse(buf) {
                        if matches!(re.add(eth.payload), Reasm::Complete(_)) {
                            reasm_ok += 1;
                        }
                    }
                }
            }
            let _ = describe(&p);
        }
        Err(_) => err += 1,
    };
    for i in 0..30_000 {
        let input: Vec<u8> = match i % 3 {
            0 => (0..rng.below(120)).map(|_| rng.next() as u8).collect(),
            1 => {
                // random bytes behind a valid-looking Ethernet header so the deeper parsers get exercised
                let et = [ETH_IPV4, ETH_IPV6, ETH_ARP, ETH_VLAN][rng.below(4)];
                let mut v = eth_frame(MAC_A, MAC_B, None, et, &[]);
                v.extend((0..rng.below(100)).map(|_| rng.next() as u8));
                if v.len() > 14 && rng.below(2) == 0 {
                    v[14] = if et == ETH_IPV6 {
                        0x60
                    } else {
                        0x40 | (rng.below(16) as u8)
                    };
                }
                v
            }
            _ => {
                let k = rng.below(frames.len());
                mutate(&mut rng, &frames[k])
            }
        };
        feed(&input, &mut table, &mut re);
    }
    let mut pcap_ok = 0;
    for _ in 0..3000 {
        let mut d = mutate(&mut rng, &w.buf[..w.buf.len().min(400)]);
        if rng.below(2) == 0 && d.len() > 4 {
            d[..4].copy_from_slice(&0xa1b2_c3d4u32.to_le_bytes());
        }
        if let Ok(r) = PcapReader::open(&d) {
            pcap_ok += r.filter(|x| x.is_ok()).count();
        }
    }
    println!(
        "-- fuzz: 30000 packets (random, shaped, mutated valid) -> {ok} dissected, {err} rejected with an error, 0 panics; {reasm_ok} random fragment sets completed; 3000 mutated pcap files -> {pcap_ok} records read, 0 panics"
    );
    assert_eq!(ok + err, 30_000);
    assert!(
        ok > 1000 && err > 1000,
        "the fuzz inputs must exercise both outcomes"
    );
    println!("packet_parsing_checksums: all checks passed");
}
