/// An iterator whose items may borrow from the iterator itself. Each item must be dropped before the
/// next call to `next`, which `std::iter::Iterator` can't express.
pub trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>>;
}

/// Overlapping mutable windows of `size`, moving one element at a time.
pub struct WindowsMut<'s, T> {
    slice: &'s mut [T],
    size: usize,
    start: usize,
}

/// Panics if `size` is 0.
pub fn windows_mut<T>(slice: &mut [T], size: usize) -> WindowsMut<'_, T> {
    assert!(size > 0, "window size 0");
    WindowsMut {
        slice,
        size,
        start: 0,
    }
}

impl<'s, T> LendingIterator for WindowsMut<'s, T> {
    type Item<'a>
        = &'a mut [T]
    where
        Self: 'a;

    fn next(&mut self) -> Option<&mut [T]> {
        let end = self.start + self.size;
        if end > self.slice.len() {
            return None;
        }
        let window = &mut self.slice[self.start..end];
        self.start += 1;
        Some(window)
    }
}

/// Each line of `text`, trimmed and uppercased, written into one reused buffer.
pub struct UpperLines<'s> {
    lines: std::str::Lines<'s>,
    buf: String,
}

pub fn upper_lines(text: &str) -> UpperLines<'_> {
    UpperLines {
        lines: text.lines(),
        buf: String::new(),
    }
}

impl<'s> LendingIterator for UpperLines<'s> {
    type Item<'a>
        = &'a str
    where
        Self: 'a;

    fn next(&mut self) -> Option<&str> {
        let line = self.lines.next()?;
        self.buf.clear();
        for c in line.trim().chars() {
            self.buf.extend(c.to_uppercase());
        }
        Some(&self.buf)
    }
}

/// Counts the items of any lending iterator.
pub fn count<L: LendingIterator>(mut it: L) -> usize {
    let mut n = 0;
    while it.next().is_some() {
        n += 1;
    }
    n
}
