//! `write!` into a stack array via `core::fmt::Write`: formatting without `String`.
use core::fmt;

pub struct FmtBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> FmtBuf<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        // Only whole `&str`s are appended (see write_str), so this is valid UTF-8.
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
    pub fn clear(&mut self) {
        self.len = 0;
    }
}
impl<const N: usize> Default for FmtBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Write for FmtBuf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // All-or-nothing per chunk keeps the contents valid UTF-8 (no split char).
        if self.len + s.len() > N {
            return Err(fmt::Error);
        }
        self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}
