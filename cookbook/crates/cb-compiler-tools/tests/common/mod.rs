//! Shared helpers for the integration tests.
#![allow(dead_code)]

use std::path::PathBuf;

pub fn compiler_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cb-compiler")
}

/// Run `f` on a thread with a big stack (the checker recurses on nested programs).
pub fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("test thread panicked")
}

pub fn peb_files(sub: &str) -> Vec<PathBuf> {
    let dir = compiler_dir().join(sub);
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "peb"))
        .collect();
    v.sort();
    v
}

/// (path, text) of every .peb file in tests/run, tests/ui and examples.
pub fn corpus() -> Vec<(PathBuf, String)> {
    let mut v = Vec::new();
    for d in ["tests/run", "tests/ui", "examples"] {
        for p in peb_files(d) {
            let t = std::fs::read_to_string(&p).unwrap();
            v.push((p, t));
        }
    }
    v
}

/// LSP position (line, UTF-16 character) to a byte offset, treating `\n`, `\r\n` and a lone
/// `\r` as line terminators. Independent of the server's own conversion (it is the oracle).
pub fn lsp_offset(text: &str, line: usize, character: usize) -> usize {
    let b = text.as_bytes();
    let (mut cur, mut start) = (0usize, 0usize);
    let mut i = 0;
    while cur < line && i < b.len() {
        match b[i] {
            b'\r' if b.get(i + 1) == Some(&b'\n') => {
                i += 2;
                cur += 1;
                start = i;
            }
            b'\r' | b'\n' => {
                i += 1;
                cur += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if cur < line {
        return text.len();
    }
    let mut units = 0;
    for (k, c) in text[start..].char_indices() {
        if c == '\n' || c == '\r' || units >= character {
            return start + k;
        }
        units += c.len_utf16();
        if units > character {
            return start + k;
        }
    }
    text.len()
}

/// Byte offset to LSP position (inverse of [`lsp_offset`] for in-line offsets).
pub fn lsp_position(text: &str, offset: usize) -> (usize, usize) {
    let b = text.as_bytes();
    let (mut line, mut start, mut i) = (0usize, 0usize, 0usize);
    while i < offset.min(b.len()) {
        match b[i] {
            b'\r' if b.get(i + 1) == Some(&b'\n') => {
                if i + 2 > offset {
                    break;
                }
                i += 2;
                line += 1;
                start = i;
            }
            b'\r' | b'\n' => {
                i += 1;
                line += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    (
        line,
        text[start..offset.min(text.len())].encode_utf16().count(),
    )
}

/// Apply LSP `TextEdit`s (all against the original text) the way a client does.
pub fn apply_lsp_edits(text: &str, edits: &serde_json::Value) -> String {
    let at = |p: &serde_json::Value| {
        lsp_offset(
            text,
            p["line"].as_u64().unwrap() as usize,
            p["character"].as_u64().unwrap() as usize,
        )
    };
    let mut es: Vec<(usize, usize, String)> = edits
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                at(&e["range"]["start"]),
                at(&e["range"]["end"]),
                e["newText"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    es.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    for (a, b, t) in es {
        out.replace_range(a..b, &t);
    }
    out
}

/// Tiny deterministic PRNG (xorshift64*), so failures are reproducible from the seed.
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    pub fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

/// Generate a syntactically valid program with random layout and comments.
/// It is not guaranteed to type check; it exercises the parser and formatter.
pub struct Gen {
    pub rng: Rng,
    out: String,
    depth: usize,
}

const NAMES: [&str; 8] = ["a", "b", "count", "total", "xs", "p", "value", "idx"];

impl Gen {
    pub fn program(seed: u64) -> String {
        let mut g = Gen {
            rng: Rng::new(seed),
            out: String::new(),
            depth: 0,
        };
        let n = 1 + g.rng.below(5);
        for i in 0..n {
            g.item(i);
            g.gap();
        }
        g.out
    }

    fn ws(&mut self) {
        let s = match self.rng.below(12) {
            0 => "\n",
            1 => "  ",
            2 => "\n\n",
            3 => "\t",
            _ => " ",
        };
        self.out.push_str(s);
    }

    fn gap(&mut self) {
        if self.rng.chance(30) {
            self.comment();
        }
        self.out
            .push_str(["\n", "\n\n", "\n\n\n", " "][self.rng.below(4)]);
    }

    fn comment(&mut self) {
        match self.rng.below(4) {
            0 => self.out.push_str("// a line comment\n"),
            1 => self.out.push_str("/// doc comment\n"),
            2 => self.out.push_str("/* block */ "),
            _ => self.out.push_str("/* multi\n   line */\n"),
        }
    }

    fn tok(&mut self, s: &str) {
        self.out.push_str(s);
        if self.rng.chance(15) {
            self.ws();
        }
        if self.rng.chance(4) {
            self.comment();
        }
    }

    fn name(&mut self) -> String {
        let n = *self.rng.pick(&NAMES);
        format!("{n}{}", self.rng.below(3))
    }

    fn ty(&mut self) {
        match self.rng.below(7) {
            0 => self.tok("int"),
            1 => self.tok("bool"),
            2 => self.tok("str"),
            3 => {
                self.tok("[");
                self.ty();
                self.tok("]");
            }
            4 => {
                self.tok("(");
                self.ty();
                self.tok(",");
                self.ty();
                self.tok(")");
            }
            5 => self.tok("Option<int>"),
            _ => {
                self.tok("fn(");
                self.ty();
                self.tok(") -> ");
                self.ty();
            }
        }
    }

    fn item(&mut self, i: usize) {
        match self.rng.below(5) {
            0 => {
                self.out.push_str(&format!("struct S{i} "));
                self.tok("{");
                for k in 0..self.rng.below(4) {
                    self.out.push_str(&format!("f{k}: "));
                    self.ty();
                    self.tok(",");
                    if self.rng.chance(20) {
                        self.out.push_str(" // field\n");
                    }
                }
                self.tok("}");
            }
            1 => {
                self.out.push_str(&format!("enum E{i}<T> "));
                self.tok("{");
                self.out.push_str("A, B(int, T), C { x: int, y: str }, ");
                self.tok("}");
            }
            2 => {
                self.out.push_str(&format!("const K{i}: int = "));
                self.expr();
                self.tok(";");
            }
            _ => {
                self.out.push_str(&format!("fn f{i}"));
                self.tok("(");
                for k in 0..self.rng.below(5) {
                    if self.rng.chance(20) {
                        self.out.push_str("mut ");
                    }
                    self.out.push_str(&format!("p{k}: "));
                    self.ty();
                    self.tok(",");
                }
                self.tok(")");
                if self.rng.chance(50) {
                    self.tok(" -> ");
                    self.ty();
                }
                self.ws();
                self.block();
            }
        }
    }

    fn block(&mut self) {
        self.tok("{");
        self.depth += 1;
        for _ in 0..self.rng.below(5) {
            self.stmt();
            self.ws();
        }
        if self.depth < 6 && self.rng.chance(50) {
            self.expr();
        }
        self.depth -= 1;
        self.tok("}");
    }

    fn stmt(&mut self) {
        match self.rng.below(7) {
            0 | 1 => {
                self.tok("let ");
                if self.rng.chance(20) {
                    self.out.push_str("mut ");
                }
                let n = self.name();
                self.out.push_str(&n);
                if self.rng.chance(30) {
                    self.tok(": ");
                    self.ty();
                }
                self.tok(" = ");
                self.expr();
                self.tok(";");
            }
            2 => {
                self.tok("while ");
                self.atom();
                self.ws();
                self.block();
            }
            3 => {
                let n = self.name();
                self.out.push_str(&n);
                self.tok(" += ");
                self.expr();
                self.tok(";");
            }
            4 => {
                self.tok("if ");
                self.atom();
                self.ws();
                self.block();
            }
            _ => {
                self.expr();
                self.tok(";");
            }
        }
        if self.rng.chance(15) {
            self.out.push_str(" // trailing\n");
        }
    }

    fn expr(&mut self) {
        self.depth += 1;
        if self.depth > 7 {
            self.atom();
            self.depth -= 1;
            return;
        }
        match self.rng.below(16) {
            0 | 1 | 2 => {
                self.atom();
                let op = *self
                    .rng
                    .pick(&["+", "-", "*", "/", "%", "==", "<", "&&", "||", "!="]);
                self.tok(&format!(" {op} "));
                // comparison operators do not chain: keep the right side parenthesised
                self.tok("(");
                self.expr();
                self.tok(")");
            }
            3 => {
                let n = self.name();
                self.out.push_str(&n);
                self.tok("(");
                let n = self.rng.below(5);
                for k in 0..n {
                    if k > 0 {
                        self.tok(", ");
                    }
                    self.expr();
                }
                if n > 0 && self.rng.chance(20) {
                    self.tok(",");
                }
                self.tok(")");
            }
            4 => {
                self.tok("if ");
                self.atom();
                self.ws();
                self.block();
                self.tok(" else ");
                self.block();
            }
            5 => {
                self.tok("match ");
                self.atom();
                self.tok(" {");
                for k in 0..1 + self.rng.below(3) {
                    self.pat(k);
                    if self.rng.chance(20) {
                        self.tok(" if ");
                        self.atom();
                    }
                    self.tok(" => ");
                    self.expr();
                    self.tok(",");
                    if self.rng.chance(15) {
                        self.out.push_str(" // arm\n");
                    }
                }
                self.tok("}");
            }
            6 => {
                self.tok("[");
                for k in 0..self.rng.below(6) {
                    if k > 0 {
                        self.tok(", ");
                    }
                    self.expr();
                }
                self.tok("]");
            }
            7 => {
                self.tok("(");
                self.expr();
                self.tok(", ");
                self.expr();
                self.tok(")");
            }
            8 => {
                self.tok("S0 {");
                self.out.push_str(" f0: ");
                self.expr();
                self.tok(", f1: ");
                self.expr();
                self.tok("}");
            }
            9 => {
                self.atom();
                self.tok(".len()");
            }
            10 => {
                self.tok("-");
                self.atom();
            }
            11 => {
                self.atom();
                self.tok(" as int");
            }
            12 => {
                self.tok("(");
                self.expr();
                self.tok(")");
            }
            _ => self.atom(),
        }
        self.depth -= 1;
    }

    fn pat(&mut self, k: usize) {
        match self.rng.below(6) {
            0 => self.out.push_str("_"),
            1 => self.out.push_str(&format!("{}", k as i64 - 1)),
            2 => self.out.push_str("E0::B(x, _)"),
            3 => self.out.push_str("1 | 2 | 3"),
            4 => self.out.push_str("(a, mut b)"),
            _ => self.out.push_str("S0 { f0, f1: 1_000, .. }"),
        }
    }

    fn atom(&mut self) {
        match self.rng.below(7) {
            0 => self.tok("1_000"),
            1 => self.tok("\"str \\n \\u{1F600} é\""),
            2 => self.tok("true"),
            3 => self.tok("()"),
            4 => {
                let n = self.name();
                self.tok(&n)
            }
            5 => self.tok("E0::A"),
            _ => self.tok("xs[0].f0.1"),
        }
    }
}

/// An in-process LSP client: sends messages straight into `Server::process`.
pub mod client {
    use pebble_tools::lsp::{Server, SyncKind};
    use serde_json::{Value, json};

    pub struct Client {
        pub server: Server,
        next_id: i64,
        pub last_diagnostics: std::collections::HashMap<String, Value>,
    }

    impl Client {
        pub fn new(sync: SyncKind) -> Client {
            Client {
                server: Server::new(sync),
                next_id: 1,
                last_diagnostics: Default::default(),
            }
        }

        pub fn started(sync: SyncKind) -> Client {
            let mut c = Client::new(sync);
            c.request("initialize", json!({"capabilities": {}}));
            c.notify("initialized", json!({}));
            c
        }

        fn absorb(&mut self, out: &[Value]) {
            for m in out {
                if m["method"] == "textDocument/publishDiagnostics" {
                    self.last_diagnostics.insert(
                        m["params"]["uri"].as_str().unwrap_or("").to_string(),
                        m["params"]["diagnostics"].clone(),
                    );
                }
            }
        }

        /// Send a request; returns the whole response object (`result` or `error`).
        pub fn request(&mut self, method: &str, params: Value) -> Value {
            let id = self.next_id;
            self.next_id += 1;
            let out = self
                .server
                .handle(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
            self.absorb(&out);
            out.into_iter()
                .find(|m| m["id"] == json!(id))
                .unwrap_or(Value::Null)
        }

        pub fn result(&mut self, method: &str, params: Value) -> Value {
            let r = self.request(method, params);
            assert!(r.get("error").is_none(), "{method} failed: {r}");
            r["result"].clone()
        }

        pub fn notify(&mut self, method: &str, params: Value) -> Vec<Value> {
            let out = self
                .server
                .handle(json!({"jsonrpc": "2.0", "method": method, "params": params}));
            self.absorb(&out);
            out
        }

        pub fn open(&mut self, uri: &str, text: &str) -> Value {
            self.notify("textDocument/didOpen", json!({"textDocument": {"uri": uri, "languageId": "pebble", "version": 1, "text": text}}));
            self.last_diagnostics
                .get(uri)
                .cloned()
                .unwrap_or(Value::Null)
        }

        pub fn at(&self, uri: &str, line: usize, character: usize) -> Value {
            json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}})
        }
    }

    /// Reference implementation of an LSP range edit on UTF-16 columns, independent of the server's.
    pub fn apply_edit_utf16(
        text: &str,
        sl: usize,
        sc: usize,
        el: usize,
        ec: usize,
        new: &str,
    ) -> String {
        let units: Vec<u16> = text.encode_utf16().collect();
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(
                units
                    .iter()
                    .enumerate()
                    .filter(|(_, u)| **u == 10)
                    .map(|(i, _)| i + 1),
            )
            .collect();
        let at = |l: usize, c: usize| -> usize {
            let Some(&s) = line_starts.get(l) else {
                return units.len();
            };
            let mut e = line_starts.get(l + 1).map(|n| n - 1).unwrap_or(units.len());
            if e > s && units[e - 1] == 13 && line_starts.get(l + 1).is_some() {
                e -= 1; // the `\r` of a CRLF terminator is not part of the line's content
            }
            (s + c).min(e)
        };
        let (a, b) = (at(sl, sc), at(el, ec));
        let mut out: Vec<u16> = units[..a].to_vec();
        out.extend(new.encode_utf16());
        out.extend(&units[b..]);
        String::from_utf16_lossy(&out)
    }
}
