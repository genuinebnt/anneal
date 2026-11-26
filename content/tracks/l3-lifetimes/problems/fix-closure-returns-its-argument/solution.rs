/// Transformations applied in order.
pub struct Pipeline {
    steps: Vec<Box<dyn Fn(&str) -> &str>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline { steps: Vec::new() }
    }

    pub fn add(&mut self, f: impl Fn(&str) -> &str + 'static) {
        self.steps.push(Box::new(f));
    }

    pub fn run<'a>(&self, s: &'a str) -> &'a str {
        self.steps.iter().fold(s, |acc, f| f(acc))
    }
}

/// Every line of `text`, trimmed, skipping blank ones.
pub fn trimmed_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

/// The first comma-separated field of every line.
pub fn first_fields(text: &str) -> Vec<&str> {
    fn first(s: &str) -> &str {
        s.split(',').next().unwrap_or("")
    }
    text.lines().map(first).collect()
}

/// A pipeline that strips leading '#'s, then surrounding whitespace.
pub fn comment_stripper() -> Pipeline {
    let mut p = Pipeline::new();
    p.add(|s| s.trim_start_matches('#'));
    p.add(|s| s.trim());
    p
}
