//! Work the quiz out by reasoning: `size_of`, `align_of`, `offset_of!` and `Layout` are off limits in this
//! file (the tests use them to check you).

#[allow(dead_code)]
pub struct Q1 {
    pub a: u8,
    pub b: u32,
    pub c: u8,
}

#[allow(dead_code)]
#[repr(C)]
pub struct Q2 {
    pub a: u8,
    pub b: u32,
    pub c: u8,
}

pub type Q3 = (u8, u64, u8);

#[allow(dead_code)]
#[repr(C)]
pub struct Q4 {
    pub a: u64,
    pub b: u8,
}

pub type Q5 = [Q4; 2];

#[allow(dead_code)]
#[repr(C)]
pub struct Q6 {
    pub tag: u8,
    pub inner: Q2,
}

pub type Q7 = (u8, [u16; 3]);

pub type Q8 = [u64; 0];

/// `(size_of, align_of)` of `Q1` to `Q8`, in order.
pub const LAYOUT: [(usize, usize); 8] = [(0, 0); 8];

/// One submission queue entry. The C consumer reads it by field offset, so it stays `#[repr(C)]`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Sqe {
    pub opcode: u8,
    pub fd: i32,
    pub flags: u8,
    pub off: u64,
    pub ioprio: u16,
    pub addr: u64,
    pub len: u32,
    pub user_data: u64,
    pub rw_flags: u32,
    pub buf_index: u16,
    pub personality: u16,
    pub file_index: u32,
}

pub const OP_NOP: u8 = 0;
pub const OP_READ: u8 = 22;
pub const OP_WRITE: u8 = 23;

impl Sqe {
    pub fn nop(user_data: u64) -> Sqe {
        Sqe {
            opcode: OP_NOP,
            user_data,
            ..Sqe::default()
        }
    }

    pub fn read(fd: i32, addr: u64, len: u32, off: u64, user_data: u64) -> Sqe {
        Sqe {
            opcode: OP_READ,
            fd,
            addr,
            len,
            off,
            user_data,
            ..Sqe::default()
        }
    }

    pub fn write(fd: i32, addr: u64, len: u32, off: u64, user_data: u64) -> Sqe {
        Sqe {
            opcode: OP_WRITE,
            fd,
            addr,
            len,
            off,
            user_data,
            ..Sqe::default()
        }
    }
}

/// A submission ring: `entries` slots (a power of two) and free-running `u32` counters.
pub struct SqRing {
    slots: Box<[Sqe]>,
    mask: u32,
    head: u32,
    tail: u32,
}

impl SqRing {
    pub fn new(entries: u32) -> SqRing {
        SqRing::starting_at(entries, 0)
    }

    /// Both counters start at `counter`. Panics unless `entries` is a power of two.
    pub fn starting_at(entries: u32, counter: u32) -> SqRing {
        todo!()
    }

    pub fn head(&self) -> u32 {
        self.head
    }

    pub fn tail(&self) -> u32 {
        self.tail
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        todo!()
    }

    /// Queues `sqe`, or hands it back when the ring is full.
    pub fn push(&mut self, sqe: Sqe) -> Result<(), Sqe> {
        todo!()
    }

    /// The oldest entry.
    pub fn pop(&mut self) -> Option<Sqe> {
        todo!()
    }
}
