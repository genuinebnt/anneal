pub enum Macro {
    Append(String),
    Upper,
    Replace(String, String),
}

pub struct Editor {
    pub text: String,
    pub clipboard: String,
    pub macros: Vec<Macro>,
    log: Vec<String>,
}

impl Editor {
    pub fn new(text: &str) -> Self {
        Editor {
            text: String::from(text),
            clipboard: String::new(),
            macros: Vec::new(),
            log: Vec::new(),
        }
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// Adds "<n>. <what>" to the log, numbered from 1.
    fn note(&mut self, what: &str) {
        let n = self.log.len() + 1;
        self.log.push(format!("{n}. {what}"));
    }

    /// Applies `m` to the text and notes "append <s>", "upper" or "replace <a> with <b>".
    fn apply(&mut self, m: &Macro) {
        match m {
            Macro::Append(s) => {
                self.text.push_str(s);
                self.note(&format!("append {s}"));
            }
            Macro::Upper => {
                self.text.make_ascii_uppercase();
                self.note("upper");
            }
            Macro::Replace(a, b) => {
                self.text = self.text.replace(a.as_str(), b);
                self.note(&format!("replace {a} with {b}"));
            }
        }
    }

    /// Applies every macro, in order. The macros stay for next time.
    pub fn run_macros(&mut self) {
        for m in &self.macros {
            self.apply(m);
        }
    }

    /// Moves the text into the clipboard (replacing what was there), leaving the text empty, and notes
    /// "cut <n> bytes".
    pub fn cut(&mut self) {
        self.clipboard = self.text;
        self.note(&format!("cut {} bytes", self.clipboard.len()));
    }

    /// Appends the clipboard to the text (the clipboard keeps it) and notes "paste <clipboard>".
    pub fn paste(&mut self) {
        let clip = &self.clipboard;
        self.note(&format!("paste {clip}"));
        self.text.push_str(clip);
    }
}
