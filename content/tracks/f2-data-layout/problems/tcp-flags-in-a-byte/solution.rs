use std::fmt;
use std::ops::{BitAnd, BitOr, BitOrAssign, Not, Sub};

/// The six classic TCP control bits in one byte, exactly as they sit in the header.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct TcpFlags(u8);

impl TcpFlags {
    pub const FIN: TcpFlags = TcpFlags(0x01);
    pub const SYN: TcpFlags = TcpFlags(0x02);
    pub const RST: TcpFlags = TcpFlags(0x04);
    pub const PSH: TcpFlags = TcpFlags(0x08);
    pub const ACK: TcpFlags = TcpFlags(0x10);
    pub const URG: TcpFlags = TcpFlags(0x20);
    const ALL: u8 = 0x3F;
    const NAMES: [(TcpFlags, &'static str); 6] = [
        (TcpFlags::FIN, "FIN"),
        (TcpFlags::SYN, "SYN"),
        (TcpFlags::RST, "RST"),
        (TcpFlags::PSH, "PSH"),
        (TcpFlags::ACK, "ACK"),
        (TcpFlags::URG, "URG"),
    ];

    pub const fn empty() -> TcpFlags {
        TcpFlags(0)
    }

    pub const fn all() -> TcpFlags {
        TcpFlags(TcpFlags::ALL)
    }

    /// The wire byte: FIN = 0x01, SYN = 0x02, RST = 0x04, PSH = 0x08, ACK = 0x10, URG = 0x20.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// `None` if any bit outside the six is set.
    pub const fn from_bits(bits: u8) -> Option<TcpFlags> {
        if bits & !TcpFlags::ALL == 0 {
            Some(TcpFlags(bits))
        } else {
            None
        }
    }

    /// Drops unknown bits.
    pub const fn from_bits_truncate(bits: u8) -> TcpFlags {
        TcpFlags(bits & TcpFlags::ALL)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Every flag in `other` is set in `self`.
    pub const fn contains(self, other: TcpFlags) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: TcpFlags) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: TcpFlags) -> TcpFlags {
        TcpFlags(self.0 | other.0)
    }

    pub const fn intersection(self, other: TcpFlags) -> TcpFlags {
        TcpFlags(self.0 & other.0)
    }

    pub const fn difference(self, other: TcpFlags) -> TcpFlags {
        TcpFlags(self.0 & !other.0)
    }

    /// The flags not in `self`: masked to the six, or `!SYN` would carry two bits that aren't flags.
    pub const fn complement(self) -> TcpFlags {
        TcpFlags(!self.0 & TcpFlags::ALL)
    }

    /// Whether conntrack accepts this combination (PSH is ignored).
    pub const fn is_valid(self) -> bool {
        VALID[self.0 as usize]
    }
}

/// The combinations netfilter's `tcp_valid_flags` accepts, before PSH is added back.
const VALID_COMBOS: [u8; 9] = [0x02, 0x22, 0x12, 0x04, 0x14, 0x11, 0x31, 0x10, 0x30];

/// Indexed by the flag byte, built at compile time: `const fn` can't use `for` or iterators, so `while`.
pub const VALID: [bool; 64] = {
    let mut table = [false; 64];
    let mut i = 0;
    while i < VALID_COMBOS.len() {
        let c = VALID_COMBOS[i];
        table[c as usize] = true;
        table[(c | TcpFlags::PSH.0) as usize] = true;
        i += 1;
    }
    table
};

impl BitOr for TcpFlags {
    type Output = TcpFlags;
    fn bitor(self, rhs: TcpFlags) -> TcpFlags {
        self.union(rhs)
    }
}

impl BitOrAssign for TcpFlags {
    fn bitor_assign(&mut self, rhs: TcpFlags) {
        self.0 |= rhs.0;
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

/// `TcpFlags(SYN | ACK)`, `TcpFlags(empty)`.
impl fmt::Debug for TcpFlags {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "TcpFlags(empty)");
        }
        let names: Vec<&str> = TcpFlags::NAMES
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .map(|(_, name)| *name)
            .collect();
        write!(f, "TcpFlags({})", names.join(" | "))
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
