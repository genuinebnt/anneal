use std::ops::{BitAnd, BitOr, BitOrAssign, Not, Sub};

/// The six classic TCP control bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct TcpFlags {
    fin: bool,
    syn: bool,
    rst: bool,
    psh: bool,
    ack: bool,
    urg: bool,
}

const NONE: TcpFlags = TcpFlags {
    fin: false,
    syn: false,
    rst: false,
    psh: false,
    ack: false,
    urg: false,
};

impl TcpFlags {
    pub const FIN: TcpFlags = TcpFlags { fin: true, ..NONE };
    pub const SYN: TcpFlags = TcpFlags { syn: true, ..NONE };
    pub const RST: TcpFlags = TcpFlags { rst: true, ..NONE };
    pub const PSH: TcpFlags = TcpFlags { psh: true, ..NONE };
    pub const ACK: TcpFlags = TcpFlags { ack: true, ..NONE };
    pub const URG: TcpFlags = TcpFlags { urg: true, ..NONE };

    pub fn empty() -> TcpFlags {
        NONE
    }

    pub fn all() -> TcpFlags {
        TcpFlags {
            fin: true,
            syn: true,
            rst: true,
            psh: true,
            ack: true,
            urg: true,
        }
    }

    /// The wire byte: FIN = 0x01, SYN = 0x02, RST = 0x04, PSH = 0x08, ACK = 0x10, URG = 0x20.
    pub fn bits(self) -> u8 {
        [self.fin, self.syn, self.rst, self.psh, self.ack, self.urg]
            .iter()
            .enumerate()
            .map(|(i, &on)| (on as u8) << i)
            .sum()
    }

    /// `None` if any bit outside the six is set.
    pub fn from_bits(bits: u8) -> Option<TcpFlags> {
        if bits >= 0x40 {
            return None;
        }
        Some(TcpFlags::from_bits_truncate(bits))
    }

    /// Drops unknown bits.
    pub fn from_bits_truncate(bits: u8) -> TcpFlags {
        let on = |i: u8| bits & (1 << i) != 0;
        TcpFlags {
            fin: on(0),
            syn: on(1),
            rst: on(2),
            psh: on(3),
            ack: on(4),
            urg: on(5),
        }
    }

    pub fn is_empty(self) -> bool {
        self == NONE
    }

    /// Every flag in `other` is set in `self`.
    pub fn contains(self, other: TcpFlags) -> bool {
        self.intersection(other) == other
    }

    pub fn intersects(self, other: TcpFlags) -> bool {
        !self.intersection(other).is_empty()
    }

    pub fn union(self, other: TcpFlags) -> TcpFlags {
        TcpFlags::from_bits_truncate(self.bits() | other.bits())
    }

    pub fn intersection(self, other: TcpFlags) -> TcpFlags {
        TcpFlags::from_bits_truncate(self.bits() & other.bits())
    }

    pub fn difference(self, other: TcpFlags) -> TcpFlags {
        TcpFlags::from_bits_truncate(self.bits() & !other.bits())
    }

    /// The flags not in `self`.
    pub fn complement(self) -> TcpFlags {
        TcpFlags::from_bits_truncate(!self.bits())
    }

    /// Whether conntrack accepts this combination (PSH is ignored): SYN, SYN|URG, SYN|ACK, RST, RST|ACK,
    /// FIN|ACK, FIN|ACK|URG, ACK or ACK|URG.
    pub fn is_valid(self) -> bool {
        matches!(
            self.bits() & !0x08,
            0x02 | 0x22 | 0x12 | 0x04 | 0x14 | 0x11 | 0x31 | 0x10 | 0x30
        )
    }
}

impl BitOr for TcpFlags {
    type Output = TcpFlags;
    fn bitor(self, rhs: TcpFlags) -> TcpFlags {
        self.union(rhs)
    }
}

impl BitOrAssign for TcpFlags {
    fn bitor_assign(&mut self, rhs: TcpFlags) {
        *self = self.union(rhs);
    }
}

impl BitAnd for TcpFlags {
    type Output = TcpFlags;
    fn bitand(self, rhs: TcpFlags) -> TcpFlags {
        self.intersection(rhs)
    }
}

impl Sub for TcpFlags {
    type Output = TcpFlags;
    fn sub(self, rhs: TcpFlags) -> TcpFlags {
        self.difference(rhs)
    }
}

impl Not for TcpFlags {
    type Output = TcpFlags;
    fn not(self) -> TcpFlags {
        self.complement()
    }
}

/// The header fields a TCP stack keeps per queued segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentMeta {
    pub seq: u32,
    pub ack: u32,
    pub window: u16,
    pub flags: TcpFlags,
}

// TODO: `pub const VALID: [bool; 64]`, indexed by the flag byte, built at compile time.
