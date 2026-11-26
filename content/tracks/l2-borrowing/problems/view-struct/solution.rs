pub struct Document {
    pub title: String,
    pub tags: Vec<String>,
    pub lines: Vec<String>,
    pub words: usize,
}

/// The title and tags, borrowed mutably together.
pub struct Header<'a> {
    pub title: &'a mut String,
    pub tags: &'a mut Vec<String>,
}

/// The lines and word count, borrowed mutably together.
pub struct Body<'a> {
    pub lines: &'a mut Vec<String>,
    pub words: &'a mut usize,
}

impl Document {
    pub fn header(&mut self) -> Header<'_> {
        Header {
            title: &mut self.title,
            tags: &mut self.tags,
        }
    }

    pub fn body(&mut self) -> Body<'_> {
        Body {
            lines: &mut self.lines,
            words: &mut self.words,
        }
    }

    /// Both views at once.
    pub fn split(&mut self) -> (Header<'_>, Body<'_>) {
        let Document {
            title,
            tags,
            lines,
            words,
        } = self;
        (Header { title, tags }, Body { lines, words })
    }
}

impl Header<'_> {
    /// Adds `tag` unless it's already there. Returns whether it was added.
    pub fn tag(&mut self, tag: &str) -> bool {
        if self.tags.iter().any(|t| t == tag) {
            return false;
        }
        self.tags.push(tag.to_string());
        true
    }

    /// Sets the title and adds the tag "edited" (once).
    pub fn retitle(&mut self, title: &str) {
        self.title.clear();
        self.title.push_str(title);
        self.tag("edited");
    }
}

impl Body<'_> {
    /// Appends a line and adds its whitespace-separated words to the count.
    pub fn push_line(&mut self, line: &str) {
        *self.words += line.split_whitespace().count();
        self.lines.push(line.to_string());
    }
}

/// Adds every word of the body that starts with '#' as a tag, without the '#' (skipping a bare "#"), in order,
/// unless it's already a tag. Returns how many tags were added.
pub fn hashtags(doc: &mut Document) -> usize {
    let (mut header, body) = doc.split();
    let mut added = 0;
    for line in body.lines.iter() {
        for word in line.split_whitespace() {
            if let Some(tag) = word.strip_prefix('#').filter(|t| !t.is_empty()) {
                if header.tag(tag) {
                    added += 1;
                }
            }
        }
    }
    added
}
