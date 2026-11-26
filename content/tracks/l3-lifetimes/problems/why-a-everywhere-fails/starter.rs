/// Splits `text` on `sep` (not empty), lazily, like `str::split`. Pieces borrow only from `text`.
pub struct Splitter<'a> {
    text: &'a str,
    sep: &'a str,
    done: bool,
}

impl<'a> Splitter<'a> {
    pub fn new(text: &'a str, sep: &'a str) -> Self {
        Splitter {
            text,
            sep,
            done: false,
        }
    }

    /// The next piece, without advancing.
    pub fn peek(&self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        Some(
            self.text
                .find(self.sep)
                .map_or(self.text, |i| &self.text[..i]),
        )
    }
}

impl<'a> Iterator for Splitter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        match self.text.find(self.sep) {
            Some(i) => {
                let piece = &self.text[..i];
                self.text = &self.text[i + self.sep.len()..];
                Some(piece)
            }
            None => {
                self.done = true;
                Some(self.text)
            }
        }
    }
}

/// Splits every line of `text` on `sep_char`, returning all the pieces in order.
pub fn split_lines(text: &str, sep_char: char) -> Vec<&str> {
    let sep = sep_char.to_string();
    let mut out = Vec::new();
    for line in text.lines() {
        out.extend(Splitter::new(line, &sep));
    }
    out
}
