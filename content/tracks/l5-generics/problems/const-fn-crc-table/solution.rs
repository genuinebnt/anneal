/// CRC-8 with polynomial `POLY`: initial value 0, no reflection, no final XOR.
pub struct Crc8<const POLY: u8>;

/// `table[b]` is the CRC of the single byte `b`.
pub const fn make_table(poly: u8) -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut b = 0;
    // `for` loops call Iterator::next, which isn't const; use `while`.
    while b < 256 {
        let mut crc = b as u8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ poly
            } else {
                crc << 1
            };
            bit += 1;
        }
        table[b] = crc;
        b += 1;
    }
    table
}

impl<const POLY: u8> Crc8<POLY> {
    /// Built at compile time, once per polynomial.
    pub const TABLE: [u8; 256] = make_table(POLY);

    pub const fn checksum(data: &[u8]) -> u8 {
        let mut crc = 0u8;
        let mut i = 0;
        while i < data.len() {
            crc = Self::TABLE[(crc ^ data[i]) as usize];
            i += 1;
        }
        crc
    }
}
