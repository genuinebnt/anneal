pub struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(src: &'a str) -> Self {
        Cursor { src, pos: 0 }
    }

    /// Byte offset of the next non-whitespace character.
    fn skip_ws(&self) -> usize {
        let rest = &self.src[self.pos..];
        self.pos + (rest.len() - rest.trim_start().len())
    }

    pub fn number(&mut self) -> Option<u64> {
        let start = self.skip_ws();
        let rest = &self.src[start..];
        let len = rest.bytes().take_while(u8::is_ascii_digit).count();
        let n = rest[..len].parse().ok()?;
        self.pos = start + len;
        Some(n)
    }

    pub fn ident(&mut self) -> Option<&'a str> {
        let src = self.src;
        let start = self.skip_ws();
        let rest = &src[start..];
        let b = rest.as_bytes();
        if !b
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            return None;
        }
        let len = b
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
            .count();
        self.pos = start + len;
        Some(&rest[..len])
    }

    pub fn rest(&self) -> &'a str {
        let src = self.src;
        &src[self.pos..]
    }
}
