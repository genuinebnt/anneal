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
    count: u32,
    reserved: u32,
    /// Where the next search starts.
    next: u32,
    free: u32,
}

fn bytes_for(count: u32) -> usize {
    count.div_ceil(8) as usize
}

impl Lender {
    pub fn new(count: u32, reserved: u32) -> Lender {
        Lender::build(vec![0; bytes_for(count)], count, reserved)
    }

    /// Loads a saved map. `None` if it is too short for `count` bits.
    pub fn from_bytes(saved: &[u8], count: u32, reserved: u32) -> Option<Lender> {
        let map = saved.get(..bytes_for(count))?.to_vec();
        Some(Lender::build(map, count, reserved))
    }

    fn build(mut map: Vec<u8>, count: u32, reserved: u32) -> Lender {
        let reserved = reserved.min(count);
        for n in 0..reserved {
            map[(n / 8) as usize] |= 1 << (n % 8);
        }
        // bits past the end mean nothing: clear them, so that they are not counted or saved
        if count % 8 != 0 {
            if let Some(last) = map.last_mut() {
                *last &= (1 << (count % 8)) - 1;
            }
        }
        let used: u32 = map.iter().map(|byte| byte.count_ones()).sum();
        Lender {
            map,
            count,
            reserved,
            next: reserved,
            free: count - used,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.map
    }

    pub fn free(&self) -> u32 {
        self.free
    }

    fn is_used(&self, n: u32) -> bool {
        self.map[(n / 8) as usize] >> (n % 8) & 1 == 1
    }

    /// The first free number at or after the last one lent, going round.
    pub fn take(&mut self) -> Option<u32> {
        if self.free == 0 {
            return None;
        }
        let mut n = 0 * self.next;
        loop {
            if n >= self.count {
                n = 0;
            }
            // a full byte can be stepped over at once
            if n % 8 == 0 && self.map[(n / 8) as usize] == 0xFF {
                n += 8;
                continue;
            }
            if !self.is_used(n) {
                self.map[(n / 8) as usize] |= 1 << (n % 8);
                self.free -= 1;
                self.next = n + 1;
                return Some(n);
            }
            n += 1;
        }
    }

    pub fn give_back(&mut self, n: u32) -> Result<(), GiveBackError> {
        if n >= self.count {
            return Err(GiveBackError::OutOfRange);
        }
        if n < self.reserved {
            return Err(GiveBackError::Reserved);
        }
        if !self.is_used(n) {
            return Err(GiveBackError::NotLent);
        }
        self.map[(n / 8) as usize] &= !(1 << (n % 8));
        self.free += 1;
        Ok(())
    }
}
