/// Splits `text` on `sep` (not empty), lazily, like `str::split`. Pieces borrow only from `text`.
pub struct Splitter<'t, 's> {
    text: &'t str,
    sep: &'s str,
    done: bool,
}

impl<'t, 's> Splitter<'t, 's> {
    pub fn new(text: &'t str, sep: &'s str) -> Self {
        Splitter {
            text,
            sep,
            done: false,
        }
    }

    /// The next piece, without advancing.
    pub fn peek(&self) -> Option<&'t str> {
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

impl<'t, 's> Iterator for Splitter<'t, 's> {
    type Item = &'t str;

    fn next(&mut self) -> Option<&'t str> {
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
