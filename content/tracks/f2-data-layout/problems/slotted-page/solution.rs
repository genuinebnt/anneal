pub const PAGE_SIZE: usize = 4096;
const HEADER: usize = 8;
const SLOT: usize = 4;

/// `insert` found no room, even after compaction. The page is unchanged.
#[derive(Debug, PartialEq, Eq)]
pub struct PageFull;

/// A B-tree leaf page in its on-disk form. Everything lives in the one array, so the page is written to
/// and read from disk as is, and nothing here allocates:
///
///   0..8        header: cell count, start of the cell area, fragmented bytes, 0 (u16 LE each)
///   8..8+4n     slot array, sorted by key: (cell offset, cell length) per cell
///   ..          free gap
///   cell_start  cells, growing down from the end: key length (u16 LE), key, value
pub struct Page {
    buf: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new() -> Page {
        let mut p = Page {
            buf: [0; PAGE_SIZE],
        };
        p.set_u16(2, PAGE_SIZE);
        p
    }

    /// Takes a page image as read from disk.
    pub fn from_bytes(bytes: &[u8; PAGE_SIZE]) -> Page {
        Page { buf: *bytes }
    }

    /// The page image, ready to write to disk.
    pub fn as_bytes(&self) -> &[u8; PAGE_SIZE] {
        &self.buf
    }

    fn u16_at(&self, at: usize) -> usize {
        u16::from_le_bytes([self.buf[at], self.buf[at + 1]]) as usize
    }

    fn set_u16(&mut self, at: usize, v: usize) {
        self.buf[at..at + 2].copy_from_slice(&(v as u16).to_le_bytes());
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.u16_at(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn cell_start(&self) -> usize {
        self.u16_at(2)
    }

    fn fragmented(&self) -> usize {
        self.u16_at(4)
    }

    /// The contiguous free bytes between the slot array and the cells.
    fn gap(&self) -> usize {
        self.cell_start() - (HEADER + SLOT * self.len())
    }

    /// Bytes still available for slots and cells, counting fragmented ones.
    pub fn free_space(&self) -> usize {
        self.gap() + self.fragmented()
    }

    fn slot(&self, i: usize) -> (usize, usize) {
        (
            self.u16_at(HEADER + SLOT * i),
            self.u16_at(HEADER + SLOT * i + 2),
        )
    }

    fn set_slot(&mut self, i: usize, offset: usize, len: usize) {
        self.set_u16(HEADER + SLOT * i, offset);
        self.set_u16(HEADER + SLOT * i + 2, len);
    }

    /// Cell `i` as (key, value), borrowed from the page.
    fn cell(&self, i: usize) -> (&[u8], &[u8]) {
        let (offset, len) = self.slot(i);
        let cell = &self.buf[offset..offset + len];
        let k = u16::from_le_bytes([cell[0], cell[1]]) as usize;
        (&cell[2..2 + k], &cell[2 + k..])
    }

    /// Binary search over the slots: `Ok(i)` if slot `i` holds `key`, `Err(i)` where it would go.
    fn search(&self, key: &[u8]) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.cell(mid).0.cmp(key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Ok(mid),
            }
        }
        Err(lo)
    }

    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.search(key).ok().map(|i| self.cell(i).1)
    }

    /// Drops slot `i`; its cell's bytes become fragmented until the next compaction.
    fn remove_slot(&mut self, i: usize) {
        let n = self.len();
        let (_, len) = self.slot(i);
        self.buf.copy_within(
            HEADER + SLOT * (i + 1)..HEADER + SLOT * n,
            HEADER + SLOT * i,
        );
        self.set_u16(0, n - 1);
        self.set_u16(4, self.fragmented() + len);
    }

    /// Adds `key`, or replaces its value. Checks the space first, so a failed insert changes nothing.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<(), PageFull> {
        let cell = 2 + key.len() + value.len();
        let found = self.search(key);
        // Replacing frees the old slot and cell.
        let reclaimed = match found {
            Ok(i) => SLOT + self.slot(i).1,
            Err(_) => 0,
        };
        if SLOT + cell > self.free_space() + reclaimed {
            return Err(PageFull);
        }
        let at = match found {
            Ok(i) => {
                self.remove_slot(i);
                i
            }
            Err(i) => i,
        };
        if self.gap() < SLOT + cell {
            self.compact();
        }
        let n = self.len();
        self.buf.copy_within(
            HEADER + SLOT * at..HEADER + SLOT * n,
            HEADER + SLOT * (at + 1),
        );
        let offset = self.cell_start() - cell;
        self.buf[offset..offset + 2].copy_from_slice(&(key.len() as u16).to_le_bytes());
        self.buf[offset + 2..offset + 2 + key.len()].copy_from_slice(key);
        self.buf[offset + 2 + key.len()..offset + cell].copy_from_slice(value);
        self.set_slot(at, offset, cell);
        self.set_u16(0, n + 1);
        self.set_u16(2, offset);
        Ok(())
    }

    pub fn delete(&mut self, key: &[u8]) -> bool {
        match self.search(key) {
            Ok(i) => {
                self.remove_slot(i);
                true
            }
            Err(_) => false,
        }
    }

    /// Packs the cells against the end of the page in slot order and zeroes the free gap. The scratch
    /// page is on the stack: 4 KiB, no allocation.
    pub fn compact(&mut self) {
        let n = self.len();
        let mut out = [0u8; PAGE_SIZE];
        let mut end = PAGE_SIZE;
        for i in 0..n {
            let (offset, len) = self.slot(i);
            out[end - len..end].copy_from_slice(&self.buf[offset..offset + len]);
            out[HEADER + SLOT * i..HEADER + SLOT * i + 2]
                .copy_from_slice(&((end - len) as u16).to_le_bytes());
            out[HEADER + SLOT * i + 2..HEADER + SLOT * i + 4]
                .copy_from_slice(&(len as u16).to_le_bytes());
            end -= len;
        }
        out[0..2].copy_from_slice(&(n as u16).to_le_bytes());
        out[2..4].copy_from_slice(&(end as u16).to_le_bytes());
        self.buf = out;
    }

    /// The cells in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> + '_ {
        (0..self.len()).map(move |i| self.cell(i))
    }
}
