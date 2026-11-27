/// Why a number could not be taken back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveBackError {
    OutOfRange,
    Reserved,
    NotLent,
}

/// Lends out the numbers below `count`, keeping one bit for each.
pub struct Lender {
    /// Bit `n % 8` of byte `n / 8` is set while `n` is not free.
    map: Vec<u8>,
    // TODO: whatever else you need
}

impl Lender {
    pub fn new(count: u32, reserved: u32) -> Lender {
        todo!()
    }

    /// Loads a saved map. `None` if it is too short for `count` bits.
    pub fn from_bytes(saved: &[u8], count: u32, reserved: u32) -> Option<Lender> {
        todo!()
    }

    pub fn bytes(&self) -> &[u8] {
        todo!()
    }

    pub fn free(&self) -> u32 {
        todo!()
    }

    /// The first free number at or after the last one lent, going round.
    pub fn take(&mut self) -> Option<u32> {
        todo!()
    }

    pub fn give_back(&mut self, n: u32) -> Result<(), GiveBackError> {
        todo!()
    }
}
