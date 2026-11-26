//! RECIPE 803 (parsing): a CommonMark-subset parser that produces the BLOCK model of a block editor: two-phase parsing, delimiter-run emphasis, printer, HTML and flat blocks.
//! LEVEL: intermediate
//! Use in: importing and exporting Markdown in a block editor, previews, static site generators.   Run: cargo run --release --example markdown_block_parser
//! COMMON USES: paste/import in Notion-like editors, docs tooling, chat message formatting, linters and formatters.
#![allow(dead_code)]
//!
//! HOW (phase 1, blocks): one line at a time. (a) walk down the chain of OPEN blocks and let each consume its continuation prefix (`>` for a quote,
//! indentation for a list item, ...); (b) try to open new blocks on what is left; (c) hand the rest of the line to the innermost block. A line that
//! matches nothing but follows an open paragraph is a LAZY CONTINUATION. A paragraph that closes gives up leading link reference definitions.
//! Whether a list is loose is decided at the end from line numbers: is there a gap between two items, or between two children of one item?
//! HOW (phase 2, inlines): runs only after phase 1, because `[a]` may refer to a definition further down. One left-to-right scan builds a linked list
//! of nodes; `*`/`_` runs go on a DELIMITER STACK and `[`/`![` on a bracket stack. `process_emphasis` pairs delimiters (left/right flanking rules,
//! the "rule of 3") and wraps the nodes between them in O(1) by splicing the list.
//! HOW (no quadratic blowup): `openers_bottom` remembers where a search for an opener already failed; backtick runs are indexed up front; "all
//! earlier `[` are now inactive" is one watermark instead of a loop; link destinations allow 32 nested parentheses (labels are also limited to
//! 999 chars, as the spec says, but their scans stop at the next bracket and would be linear without it); block
//! containers and inline formatting have a depth cap, so the recursive renderer cannot overflow the stack. A global STEP COUNTER counts the work of
//! the scanning loops and `pathological_inputs` checks that doubling the input about doubles it. (It counts the loops that could be super-linear, not
//! every instruction.)
//! OUTPUTS: typed AST with byte spans, HTML renderer, normalising Markdown PRINTER (indented code becomes fenced, setext becomes ATX when it fits,
//! reference links become inline links), and a FLAT BLOCK LIST (`id, type, depth, text with marks`) as editors store it, with the way back.
//!
//! PRINTER LIMIT: `parse(print(parse(x))) == parse(x)` holds on the 2000 seeded documents checked here, but those seeds are also the ones the printer
//! was debugged against. On 100,000 OTHER seeds of the same generator (measured once, not part of this run) 32 documents did not round-trip:
//! mostly dense mixes of literal and live `*` `_`, which the bounded escape search cannot reproduce, and quotes or lists that only a (dropped)
//! reference definition kept apart inside a tight list item. The generator does not produce every construct, so arbitrary input can fail more often.
//!
//! SUBSET. Not supported, or only partly: raw inline HTML (printed as text); HTML blocks of start conditions 3, 4, 5 and 7; named entities beyond a
//! small table; full Unicode punctuation and case folding for flanking rules and labels (ASCII plus a few common marks); GFM extensions other than
//! task-list items (no tables, strikethrough or bare autolinks). Nesting deeper than the caps is flattened, which the spec does not do.
//! MEASURED (once, over all 652 examples of spec 0.31.2): 614 render exactly. All 38 others are in the "Raw HTML" and "HTML blocks" sections or
//! need raw inline HTML. The embedded examples are taken from the spec's machine-readable test file (https://spec.commonmark.org/0.31.2/spec.json, spec text by John
//! MacFarlane, CC BY-SA 4.0), numbered as there. They are a SELECTION of the ones this parser passes, so passing them is a regression check, not a
//! conformance claim.
//! Read: https://spec.commonmark.org/0.31.2/ (the appendix "A parsing strategy" describes both phases); reference implementations
//! https://github.com/commonmark/commonmark.js and https://github.com/raphlinus/pulldown-cmark
//! TRY NEXT:
//!   - [S] Add `~~strikethrough~~` (GFM): which of the delimiter fields change? Add examples before you start.
//!   - [S] Remove the `openers_bottom` update in `process_emphasis` and run again: which pathological input trips the step budget first?
//!   - [M] Make `from_flat` accept rows whose depth jumps by more than one (an editor can produce them by indenting) and decide what they mean.
//!   - [A] PROJECT: import with `to_flat` into the block CRDT of `crdt_block_editor_notion_like` (one `Move` per row, one `Mark` per mark) and export
//!     again; list what survives the round trip and what the CRDT model cannot hold.
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Write as _;

const MAX_BLOCK_DEPTH: usize = 64;
const MAX_INLINE_DEPTH: usize = 64;

thread_local! { static STEPS: Cell<u64> = const { Cell::new(0) }; }
fn tick(n: usize) {
    STEPS.with(|s| s.set(s.get() + n as u64));
}

// ------------------------------------------------------------------ typed AST
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Span {
    start: usize, // byte offsets into the source
    end: usize,
}
#[derive(Clone, Debug)]
struct Block {
    span: Span,
    kind: Kind,
}
/// Equality is about meaning: spans are positions, so a reprinted document has different ones.
impl PartialEq for Block {
    fn eq(&self, o: &Block) -> bool {
        self.kind == o.kind
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    Quote(Vec<Block>),
    List(List),
    Code { info: String, text: String },
    Html(String),
    Break,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct ListMeta {
    ordered: bool,
    delim: char, // bullet char, or '.' / ')'
    start: u32,
    tight: bool,
}
#[derive(Clone, Debug, PartialEq)]
struct List {
    meta: ListMeta,
    items: Vec<Item>,
}
#[derive(Clone, Debug)]
struct Item {
    span: Span,
    task: Option<bool>,
    blocks: Vec<Block>,
}
impl PartialEq for Item {
    fn eq(&self, o: &Item) -> bool {
        self.task == o.task && self.blocks == o.blocks
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Inline {
    Text(String),
    Soft,
    Hard,
    Code(String),
    Emph(char, Vec<Inline>), // the delimiter char is kept so that printing preserves how neighbours pair up
    Strong(char, Vec<Inline>),
    Link {
        dest: String,
        title: String,
        kids: Vec<Inline>,
    },
    Image {
        dest: String,
        title: String,
        kids: Vec<Inline>,
    },
}
type Refs = HashMap<String, (String, String)>;

// ------------------------------------------------------------------ phase 1: block structure
#[derive(Clone, Debug, PartialEq)]
enum NK {
    Doc,
    Quote,
    List {
        ordered: bool,
        delim: u8,
        start: u32,
    },
    Item {
        width: usize,
    },
    Para,
    Heading(u8),
    Fenced {
        ch: u8,
        len: usize,
        indent: usize,
    },
    Indented,
    Html(u8),
    Break,
    Gone, // a paragraph that held only link reference definitions
}
struct Node {
    kind: NK,
    parent: usize,
    kids: Vec<usize>,
    open: bool,
    text: String,
    info: String,
    start_line: usize,
    end_line: usize,
    span: Span,
}

/// A position in one line, aware of tab stops: a tab can be only PARTLY consumed by a container prefix.
#[derive(Clone, Copy)]
struct Cur<'a> {
    s: &'a str,
    off: usize,
    col: usize,
    partial_tab: bool,
}
impl<'a> Cur<'a> {
    fn byte(&self, i: usize) -> u8 {
        self.s.as_bytes().get(i).copied().unwrap_or(0)
    }
    /// (byte offset, column) of the next char that is not a space or tab.
    fn nonspace(&self) -> (usize, usize) {
        let (mut i, mut c) = (self.off, self.col);
        loop {
            match self.byte(i) {
                b' ' => c += 1,
                b'\t' => c += 4 - c % 4,
                _ => break,
            }
            i += 1;
        }
        tick(i - self.off);
        (i, c)
    }
    fn indent(&self) -> usize {
        self.nonspace().1 - self.col
    }
    fn blank(&self) -> bool {
        self.nonspace().0 == self.s.len()
    }
    fn advance_cols(&mut self, mut n: usize) {
        while n > 0 && self.off < self.s.len() {
            if self.byte(self.off) == b'\t' {
                let to_stop = 4 - self.col % 4;
                if to_stop > n {
                    self.col += n;
                    self.partial_tab = true;
                    return;
                }
                n -= to_stop;
                self.col += to_stop;
            } else {
                n -= 1;
                self.col += 1;
            }
            self.partial_tab = false;
            self.off += 1;
        }
    }
    fn to_nonspace(&mut self) {
        let (i, c) = self.nonspace();
        self.off = i;
        self.col = c;
        self.partial_tab = false;
    }
    /// Skip `n` bytes of non-tab text (a marker).
    fn bump(&mut self, n: usize) {
        self.off += n;
        self.col += n;
    }
    fn optional_space(&mut self) {
        if matches!(self.byte(self.off), b' ' | b'\t') {
            self.advance_cols(1);
        }
    }
    fn rest(&self) -> String {
        if self.partial_tab {
            " ".repeat(4 - self.col % 4) + &self.s[self.off + 1..]
        } else {
            self.s[self.off..].to_string()
        }
    }
}

fn fence_start(s: &str) -> Option<(u8, usize, &str)> {
    let ch = *s.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = s.bytes().take_while(|b| *b == ch).count();
    let info = s[len..].trim();
    (len >= 3 && !(ch == b'`' && info.contains('`'))).then_some((ch, len, info))
}
fn atx(s: &str) -> Option<(u8, String)> {
    let n = s.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&n) || !matches!(s.as_bytes().get(n), None | Some(b' ' | b'\t')) {
        return None;
    }
    let mut body = s[n..].trim_matches([' ', '\t']);
    let stripped = body.trim_end_matches('#');
    if stripped.is_empty() || stripped.ends_with([' ', '\t']) {
        body = stripped.trim_end_matches([' ', '\t']);
    }
    Some((n as u8, body.to_string()))
}
fn thematic(s: &str) -> bool {
    let ch = s.bytes().next().unwrap_or(0);
    if !matches!(ch, b'*' | b'-' | b'_') {
        return false;
    }
    tick(s.len());
    s.bytes().all(|b| b == ch || b == b' ' || b == b'\t')
        && s.bytes().filter(|b| *b == ch).count() >= 3
}
fn setext(s: &str) -> Option<u8> {
    let t = s.trim_end_matches([' ', '\t']);
    let ch = t.bytes().next()?;
    ((ch == b'=' || ch == b'-') && t.bytes().all(|b| b == ch)).then_some(if ch == b'=' {
        1
    } else {
        2
    })
}
const BLOCK_TAGS: &str = " address article aside base basefont blockquote body caption center col colgroup dd details dialog dir div dl dt fieldset figcaption figure footer form frame frameset h1 h2 h3 h4 h5 h6 head header hr html iframe legend li link main menu menuitem nav noframes ol optgroup option p param search section summary table tbody td tfoot th thead title tr track ul ";
/// HTML-block-lite: start conditions 1 (raw text elements), 2 (comment) and 6 (known block tags).
fn html_start(s: &str) -> Option<u8> {
    let rest = s.strip_prefix('<')?;
    if rest.starts_with("!--") {
        return Some(2);
    }
    let closing = rest.starts_with('/');
    let name: String = rest
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .take(11)
        .collect::<String>()
        .to_ascii_lowercase();
    let after = &rest[(closing as usize + name.len()).min(rest.len())..];
    let ends = after.is_empty() || after.starts_with([' ', '\t', '>']);
    if !closing && ["script", "pre", "style", "textarea"].contains(&name.as_str()) && ends {
        return Some(1);
    }
    (!name.is_empty()
        && BLOCK_TAGS.contains(&format!(" {name} "))
        && (ends || after.starts_with("/>")))
    .then_some(6)
}
fn html_ends(kind: u8, line: &str) -> bool {
    match kind {
        1 => {
            let l = line.to_ascii_lowercase();
            ["</script>", "</pre>", "</style>", "</textarea>"]
                .iter()
                .any(|e| l.contains(e))
        }
        2 => line.contains("-->"),
        _ => false,
    }
}
/// (ordered, delimiter, start number, marker length)
fn list_marker(s: &str) -> Option<(bool, u8, u32, usize)> {
    let b = s.as_bytes();
    let (ordered, delim, start, len) = match b.first()? {
        c @ (b'-' | b'+' | b'*') => (false, *c, 1, 1),
        _ => {
            let n = b.iter().take(10).take_while(|c| c.is_ascii_digit()).count();
            if !(1..=9).contains(&n) || !matches!(b.get(n), Some(b'.' | b')')) {
                return None;
            }
            (true, b[n], s[..n].parse().ok()?, n + 1)
        }
    };
    matches!(b.get(len), None | Some(b' ' | b'\t')).then_some((ordered, delim, start, len))
}

struct BlockParser {
    n: Vec<Node>,
    refs: Refs,
    line: usize,
    line_start: usize,
    line_end: usize,
}
impl BlockParser {
    fn new() -> BlockParser {
        let doc = Node {
            kind: NK::Doc,
            parent: 0,
            kids: vec![],
            open: true,
            text: String::new(),
            info: String::new(),
            start_line: 0,
            end_line: 0,
            span: Span::default(),
        };
        BlockParser {
            n: vec![doc],
            refs: Refs::new(),
            line: 0,
            line_start: 0,
            line_end: 0,
        }
    }
    fn open_child(&self, i: usize) -> Option<usize> {
        self.n[i].kids.last().copied().filter(|k| self.n[*k].open)
    }
    fn tip(&self) -> usize {
        let mut i = 0;
        while let Some(k) = self.open_child(i) {
            i = k;
        }
        i
    }
    fn depth(&self, mut i: usize) -> usize {
        let mut d = 0;
        while i != 0 {
            i = self.n[i].parent;
            d += 1;
        }
        tick(d);
        d
    }
    /// A line that belongs to block `i` extends it and all its ancestors.
    fn touch(&mut self, mut i: usize) {
        loop {
            self.n[i].end_line = self.line;
            self.n[i].span.end = self.line_end;
            if i == 0 {
                return;
            }
            i = self.n[i].parent;
        }
    }
    fn close_below(&mut self, container: usize) {
        if let Some(k) = self.open_child(container) {
            self.close_below(k);
            self.finalize(k);
        }
    }
    fn finalize(&mut self, i: usize) {
        self.n[i].open = false;
        match self.n[i].kind {
            NK::Para => {
                self.take_ref_defs(i);
                if self.n[i].text.trim().is_empty() {
                    // it was only definitions. Keep the node: its LINES still matter when deciding whether a list is loose.
                    self.n[i].kind = NK::Gone;
                }
            }
            NK::Indented => {
                // trailing blank lines do not belong to an indented code block
                let t = &self.n[i].text;
                let mut end = t.len();
                while end > 0 {
                    let start = t[..end - 1].rfind('\n').map_or(0, |p| p + 1);
                    if !t[start..end].trim().is_empty() {
                        break;
                    }
                    end = start;
                }
                self.n[i].text.truncate(end);
            }
            NK::Html(_) => {
                // an unclosed `<!--` inside a container swallows the blank lines after it; they carry no meaning
                let keep = self.n[i].text.trim_end_matches([' ', '\t', '\n']).len();
                self.n[i].text.truncate(keep);
                self.n[i].text.push('\n');
            }
            _ => {}
        }
    }
    /// Strip `[label]: dest "title"` definitions from the front of a paragraph. The first definition of a label wins.
    fn take_ref_defs(&mut self, i: usize) {
        let text: Vec<char> = self.n[i].text.chars().collect();
        let mut at = 0;
        while text.get(at) == Some(&'[') {
            let Some((label, dest, title, next)) = ref_def(&text, at) else {
                break;
            };
            self.refs.entry(label).or_insert((dest, title));
            at = next;
        }
        if at > 0 {
            self.n[i].text = text[at..].iter().collect();
        }
    }
    fn add(&mut self, mut parent: usize, kind: NK, start: usize) -> usize {
        self.close_below(parent);
        loop {
            let ok = match self.n[parent].kind {
                NK::Doc | NK::Quote | NK::Item { .. } => !matches!(kind, NK::Item { .. }),
                NK::List { .. } => matches!(kind, NK::Item { .. }),
                _ => false,
            };
            if ok {
                break;
            }
            self.finalize(parent);
            parent = self.n[parent].parent;
        }
        let id = self.n.len();
        let span = Span {
            start: self.line_start + start,
            end: self.line_end,
        };
        self.n.push(Node {
            kind,
            parent,
            kids: vec![],
            open: true,
            text: String::new(),
            info: String::new(),
            start_line: self.line,
            end_line: self.line,
            span,
        });
        self.n[parent].kids.push(id);
        self.touch(id);
        id
    }
    fn append(&mut self, i: usize, s: &str) {
        self.n[i].text.push_str(s);
        self.n[i].text.push('\n');
        self.touch(i);
    }

    fn feed(&mut self, line: &str) {
        let mut cur = Cur {
            s: line,
            off: 0,
            col: 0,
            partial_tab: false,
        };
        // (a) let the open blocks consume their prefixes
        let mut container = 0;
        while let Some(c) = self.open_child(container) {
            tick(1);
            let (nns, _) = cur.nonspace();
            let (indent, blank) = (cur.indent(), nns == line.len());
            let matched = match self.n[c].kind {
                NK::Quote => {
                    let hit = indent <= 3 && cur.byte(nns) == b'>';
                    if hit {
                        cur.to_nonspace();
                        cur.bump(1);
                        cur.optional_space();
                        self.touch(c);
                    }
                    hit
                }
                NK::List { .. } => true,
                NK::Item { width } => {
                    if blank {
                        // an item may start with at most one blank line
                        cur.to_nonspace();
                        !self.n[c].kids.is_empty() || self.n[c].start_line + 1 < self.line
                    } else if indent >= width {
                        cur.advance_cols(width);
                        true
                    } else {
                        false
                    }
                }
                NK::Fenced {
                    ch,
                    len,
                    indent: fence_indent,
                } => {
                    let t = &line[nns..];
                    let run = t.bytes().take_while(|b| *b == ch).count();
                    if indent <= 3 && run >= len && t[run..].trim().is_empty() {
                        self.touch(c);
                        self.finalize(c);
                        return;
                    }
                    cur.advance_cols(fence_indent.min(indent));
                    true
                }
                NK::Indented => {
                    if indent >= 4 {
                        cur.advance_cols(4);
                        true
                    } else if blank {
                        cur.to_nonspace();
                        true
                    } else {
                        false
                    }
                }
                NK::Html(k) => !(k == 6 && blank),
                NK::Para => !blank,
                _ => false,
            };
            if !matched {
                break;
            }
            container = c;
        }
        // An item that began with a blank line and gets a second one is closed (the check above let line 2 through only if it has content).
        if let NK::Item { .. } = self.n[container].kind {
            if cur.blank()
                && self.n[container].kids.is_empty()
                && self.line > self.n[container].start_line
            {
                container = self.n[container].parent;
            }
        }
        let tip = self.tip();
        let all_matched = container == tip;
        // (b) open new blocks
        let mut opened = false;
        while !matches!(
            self.n[container].kind,
            NK::Fenced { .. } | NK::Indented | NK::Html(_)
        ) {
            let (nns, _) = cur.nonspace();
            let indent = cur.indent();
            let s = &line[nns..];
            let in_para = self.n[container].kind == NK::Para;
            // would this line otherwise continue a paragraph (also lazily)?
            let para_tip = !opened && self.n[tip].kind == NK::Para;
            if indent >= 4 {
                if !para_tip && !cur.blank() && !(in_para) {
                    cur.advance_cols(4);
                    container = self.add(container, NK::Indented, nns);
                    opened = true;
                }
                break;
            }
            let room = self.depth(container) < MAX_BLOCK_DEPTH;
            if s.starts_with('>') && room {
                cur.to_nonspace();
                cur.bump(1);
                cur.optional_space();
                container = self.add(container, NK::Quote, nns);
            } else if let Some((level, body)) = atx(s) {
                container = self.add(container, NK::Heading(level), nns);
                self.n[container].text = body;
                self.n[container].open = false;
                return;
            } else if let Some((ch, len, info)) = fence_start(s) {
                container = self.add(container, NK::Fenced { ch, len, indent }, nns);
                self.n[container].info = info.to_string();
                return;
            } else if let Some(k) = html_start(s) {
                container = self.add(container, NK::Html(k), nns);
                opened = true;
                break; // the whole line is content
            } else if let (true, Some(level)) = (in_para, setext(s)) {
                self.take_ref_defs(container);
                if self.n[container].text.trim().is_empty() {
                    break; // the paragraph was only definitions: this line is ordinary text (or a thematic break below)
                }
                self.n[container].kind = NK::Heading(level);
                self.touch(container);
                self.n[container].open = false;
                return;
            } else if thematic(s) {
                container = self.add(container, NK::Break, nns);
                self.n[container].open = false;
                return;
            } else if let Some((ordered, delim, start, mlen)) = list_marker(s).filter(|_| room) {
                let empty = s[mlen..].trim().is_empty();
                // an empty item, or an ordered list not starting at 1, cannot interrupt a paragraph
                if in_para && (empty || (ordered && start != 1)) {
                    break;
                }
                cur.to_nonspace();
                cur.bump(mlen);
                let spaces = cur.indent();
                let pad = if empty || spaces >= 5 {
                    cur.optional_space();
                    1
                } else {
                    cur.advance_cols(spaces);
                    spaces
                };
                let width = indent + mlen + pad;
                let same = matches!(self.n[container].kind, NK::List { ordered: o, delim: d, .. } if o == ordered && d == delim);
                if !same {
                    if matches!(self.n[container].kind, NK::List { .. }) {
                        self.close_below(container);
                        self.finalize(container);
                        container = self.n[container].parent;
                    }
                    container = self.add(
                        container,
                        NK::List {
                            ordered,
                            delim,
                            start,
                        },
                        nns,
                    );
                }
                container = self.add(container, NK::Item { width }, nns);
            } else {
                break;
            }
            opened = true;
        }
        // (c) the rest of the line is text
        let blank = cur.blank();
        if !all_matched && !opened && !blank && self.n[tip].kind == NK::Para {
            cur.to_nonspace();
            self.append(tip, &cur.rest()); // lazy continuation
            return;
        }
        self.close_below(container);
        match self.n[container].kind {
            NK::Fenced { .. } | NK::Indented => self.append(container, &cur.rest()),
            NK::Html(k) => {
                let rest = cur.rest();
                self.append(container, &rest);
                if html_ends(k, &rest) {
                    self.finalize(container);
                }
            }
            NK::Para if !blank => {
                cur.to_nonspace();
                self.append(container, &cur.rest());
            }
            _ if !blank => {
                // a List cannot hold text: `add` closes it
                let (nns, _) = cur.nonspace();
                cur.to_nonspace();
                let p = self.add(container, NK::Para, nns);
                self.append(p, &cur.rest());
            }
            _ => {}
        }
    }
}

fn parse(src: &str) -> (Vec<Block>, Refs) {
    let mut p = BlockParser::new();
    let mut start = 0;
    while start < src.len() {
        let end = src[start..]
            .find(['\n', '\r'])
            .map_or(src.len(), |i| start + i);
        p.line += 1;
        p.line_start = start;
        p.line_end = end;
        let line = src[start..end].replace('\0', "\u{FFFD}");
        tick(line.len() + 1);
        p.feed(&line);
        start = end + if src[end..].starts_with("\r\n") { 2 } else { 1 };
    }
    p.close_below(0);
    let blocks = build(&p, 0);
    (blocks, p.refs)
}

/// Phase 2 driver: turn the node arena into the typed AST, parsing inlines now that every reference definition is known.
fn build(p: &BlockParser, i: usize) -> Vec<Block> {
    p.n[i]
        .kids
        .iter()
        .filter(|k| p.n[**k].kind != NK::Gone)
        .map(|k| build_one(p, *k))
        .collect()
}
fn build_one(p: &BlockParser, i: usize) -> Block {
    let n = &p.n[i];
    let kind = match &n.kind {
        NK::Para => Kind::Paragraph(inlines(n.text.trim_end_matches([' ', '\t', '\n']), &p.refs)),
        NK::Heading(l) => {
            Kind::Heading(*l, inlines(n.text.trim_matches([' ', '\t', '\n']), &p.refs))
        }
        NK::Quote => Kind::Quote(build(p, i)),
        NK::Break => Kind::Break,
        NK::Fenced { .. } => Kind::Code {
            info: unescape(&n.info),
            text: n.text.clone(),
        },
        NK::Indented => Kind::Code {
            info: String::new(),
            text: n.text.clone(),
        },
        NK::Html(_) => Kind::Html(n.text.clone()),
        NK::List {
            ordered,
            delim,
            start,
        } => {
            let gap = |ids: &[usize]| {
                ids.windows(2)
                    .any(|w| p.n[w[0]].end_line + 1 < p.n[w[1]].start_line)
            };
            let tight = !gap(&n.kids) && !n.kids.iter().any(|it| gap(&p.n[*it].kids));
            let items = n.kids.iter().map(|it| item(p, *it)).collect();
            Kind::List(List {
                meta: ListMeta {
                    ordered: *ordered,
                    delim: *delim as char,
                    start: *start,
                    tight,
                },
                items,
            })
        }
        NK::Doc | NK::Item { .. } | NK::Gone => unreachable!("only reached through their parent"),
    };
    Block { span: n.span, kind }
}
fn item(p: &BlockParser, i: usize) -> Item {
    let mut blocks = build(p, i);
    // GFM task list item: the first paragraph starts with "[ ] " or "[x] "
    let mut task = None;
    if let Some(Block {
        kind: Kind::Paragraph(inl),
        ..
    }) = blocks.first_mut()
    {
        let first = p.n[i]
            .kids
            .iter()
            .map(|k| &p.n[*k])
            .find(|n| n.kind != NK::Gone)
            .map_or("", |n| n.text.as_str());
        let b = first.as_bytes();
        if b.len() > 4
            && b[0] == b'['
            && matches!(b[1], b' ' | b'x' | b'X')
            && b[2] == b']'
            && b[3] == b' '
            && !first[4..].trim().is_empty()
        {
            task = Some(b[1] != b' ');
            *inl = inlines(first[4..].trim_matches([' ', '\n']), &p.refs);
        }
    }
    Item {
        span: p.n[i].span,
        task,
        blocks,
    }
}

// ------------------------------------------------------------------ link syntax shared by inlines and reference definitions
fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation() || "¡¿·«»–—‘’‚“”„†‡•…‰′″‹›€£¥¢§¶©®°±×÷→←".contains(c)
}
/// `&name;`, `&#123;` or `&#x1F;` at the start of `s`: (decoded, length in chars).
fn entity(s: &[char]) -> Option<(String, usize)> {
    if s.first() != Some(&'&') {
        return None;
    }
    let semi = s.iter().take(34).position(|c| *c == ';')?;
    let body: String = s[1..semi].iter().collect();
    let decoded = if let Some(num) = body.strip_prefix('#') {
        let (digits, radix, max) = match num.strip_prefix(['x', 'X']) {
            Some(h) => (h, 16, 6),
            None => (num, 10, 7),
        };
        if digits.is_empty() || digits.len() > max || !digits.chars().all(|c| c.is_digit(radix)) {
            return None;
        }
        let v = u32::from_str_radix(digits, radix).ok()?;
        char::from_u32(v)
            .filter(|c| *c != '\0')
            .unwrap_or('\u{FFFD}')
            .to_string()
    } else {
        let table = [
            ("amp", "&"),
            ("lt", "<"),
            ("gt", ">"),
            ("quot", "\""),
            ("apos", "'"),
            ("nbsp", "\u{a0}"),
            ("copy", "©"),
            ("reg", "®"),
            ("AElig", "Æ"),
            ("Dcaron", "Ď"),
            ("frac34", "¾"),
            ("HilbertSpace", "ℋ"),
            ("DifferentialD", "ⅆ"),
            ("ClockwiseContourIntegral", "∲"),
            ("ngE", "≧̸"),
            ("auml", "ä"),
            ("ouml", "ö"),
            ("uuml", "ü"),
            ("mdash", "—"),
            ("ndash", "–"),
            ("hellip", "…"),
            ("trade", "™"),
            ("times", "×"),
        ];
        table.iter().find(|(n, _)| *n == body)?.1.to_string()
    };
    Some((decoded, semi + 1))
}
/// Resolve backslash escapes and entities (used for destinations, titles and info strings).
fn unescape(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let (mut out, mut i) = (String::new(), 0);
    while i < c.len() {
        if c[i] == '\\' && c.get(i + 1).is_some_and(char::is_ascii_punctuation) {
            out.push(c[i + 1]);
            i += 2;
        } else if let Some((d, n)) = entity(&c[i..]) {
            out += &d;
            i += n;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}
fn skip_ws(c: &[char], mut i: usize) -> usize {
    let from = i;
    while matches!(c.get(i), Some(' ' | '\t' | '\n')) {
        i += 1;
    }
    tick(i - from);
    i
}
/// `[label]` at `i`: (raw inner text, index after `]`). At most 999 chars, no unescaped brackets.
fn link_label(c: &[char], i: usize) -> Option<(String, usize)> {
    if c.get(i) != Some(&'[') {
        return None;
    }
    let mut j = i + 1;
    while j < c.len() && j - i <= 1000 {
        tick(1);
        match c[j] {
            '\\' => j += 2,
            '[' => return None,
            ']' => return Some((c[i + 1..j].iter().collect(), j + 1)),
            _ => j += 1,
        }
    }
    None
}
fn normalize_label(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .to_uppercase()
}
fn link_dest(c: &[char], i: usize) -> Option<(String, usize)> {
    let mut j = i;
    if c.get(i) == Some(&'<') {
        j += 1;
        while j < c.len() {
            tick(1);
            match c[j] {
                '\\' => j += 2,
                '\n' | '<' => return None,
                '>' => return Some((unescape(&c[i + 1..j].iter().collect::<String>()), j + 1)),
                _ => j += 1,
            }
        }
        return None;
    }
    let mut open = 0;
    // only ASCII space and control characters end a bare destination (a non-breaking space is part of it)
    while j < c.len() && c[j] != ' ' && !c[j].is_ascii_control() {
        tick(1);
        match c[j] {
            '\\' if c.get(j + 1).is_some_and(char::is_ascii_punctuation) => j += 1,
            '(' => {
                open += 1;
                if open > 32 {
                    return None;
                }
            }
            ')' if open == 0 => break,
            ')' => open -= 1,
            _ => {}
        }
        j += 1;
    }
    (open == 0).then(|| (unescape(&c[i..j].iter().collect::<String>()), j))
}
fn link_title(c: &[char], i: usize) -> Option<(String, usize)> {
    let close = match c.get(i)? {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        _ => return None,
    };
    let mut j = i + 1;
    while j < c.len() {
        tick(1);
        match c[j] {
            '\\' => j += 2,
            x if x == close => {
                return Some((unescape(&c[i + 1..j].iter().collect::<String>()), j + 1));
            }
            '(' if close == ')' => return None,
            _ => j += 1,
        }
    }
    None
}
/// Index just past the end of line if only spaces remain on it.
fn line_end(c: &[char], mut i: usize) -> Option<usize> {
    while matches!(c.get(i), Some(' ' | '\t')) {
        i += 1;
    }
    match c.get(i) {
        None => Some(i),
        Some('\n') => Some(i + 1),
        _ => None,
    }
}
fn ref_def(c: &[char], at: usize) -> Option<(String, String, String, usize)> {
    let (label, i) = link_label(c, at)?;
    if label.trim().is_empty() || c.get(i) != Some(&':') {
        return None;
    }
    let i = skip_ws(c, i + 1);
    if i >= c.len() {
        return None;
    }
    let (dest, after_dest) = link_dest(c, i)?;
    if dest.is_empty() && c[i] != '<' {
        return None;
    }
    let t = skip_ws(c, after_dest);
    if t > after_dest {
        if let Some((title, after)) = link_title(c, t) {
            if let Some(end) = line_end(c, after) {
                return Some((normalize_label(&label), dest, title, end));
            }
        }
    }
    // no (valid) title: the definition must end on the destination's line
    line_end(c, after_dest).map(|end| (normalize_label(&label), dest, String::new(), end))
}

// ------------------------------------------------------------------ phase 2: inlines
#[derive(Debug)]
enum IK {
    Root,
    Text(String),
    Soft,
    Hard,
    Code(String),
    Emph(char),
    Strong(char),
    Link {
        dest: String,
        title: String,
        image: bool,
    },
}
struct INode {
    kind: IK,
    first: Option<usize>,
    last: Option<usize>,
    prev: Option<usize>,
    next: Option<usize>,
}
struct Delim {
    node: usize,
    ch: char,
    len: usize,
    orig: usize,
    can_open: bool,
    can_close: bool,
    prev: Option<usize>,
    next: Option<usize>,
}
struct Bracket {
    node: usize,
    image: bool,
    delim_bottom: Option<usize>,
    text_start: usize,
    has_inner_bracket: bool,
}
struct Inl {
    n: Vec<INode>,
    d: Vec<Delim>,
    d_tail: Option<usize>,
}
impl Inl {
    fn new_node(&mut self, kind: IK) -> usize {
        self.n.push(INode {
            kind,
            first: None,
            last: None,
            prev: None,
            next: None,
        });
        self.n.len() - 1
    }
    fn push(&mut self, parent: usize, kind: IK) -> usize {
        let id = self.new_node(kind);
        self.n[id].prev = self.n[parent].last;
        match self.n[parent].last {
            Some(l) => self.n[l].next = Some(id),
            None => self.n[parent].first = Some(id),
        }
        self.n[parent].last = Some(id);
        id
    }
    /// Move the siblings strictly between `after` and `before` (None = to the end) under a new node placed right after `after`. O(1).
    fn wrap(&mut self, after: usize, before: Option<usize>, kind: IK) {
        let w = self.new_node(kind);
        let first = self.n[after].next.filter(|f| Some(*f) != before);
        let last = match before {
            Some(b) => self.n[b].prev.filter(|l| *l != after),
            None => self.n[0].last.filter(|l| *l != after),
        };
        if let (Some(f), Some(l)) = (first, last) {
            self.n[w].first = Some(f);
            self.n[w].last = Some(l);
            self.n[f].prev = None;
            self.n[l].next = None;
        }
        self.n[w].prev = Some(after);
        self.n[w].next = before;
        self.n[after].next = Some(w);
        match before {
            Some(b) => self.n[b].prev = Some(w),
            None => self.n[0].last = Some(w),
        }
    }
    /// Remove a top-level node.
    fn unlink(&mut self, id: usize) {
        let (p, x) = (self.n[id].prev, self.n[id].next);
        match p {
            Some(p) => self.n[p].next = x,
            None => self.n[0].first = x,
        }
        match x {
            Some(x) => self.n[x].prev = p,
            None => self.n[0].last = p,
        }
    }
    fn remove_delim(&mut self, i: usize) {
        let (p, x) = (self.d[i].prev, self.d[i].next);
        if let Some(p) = p {
            self.d[p].next = x;
        }
        match x {
            Some(x) => self.d[x].prev = p,
            None => self.d_tail = p,
        }
    }
    fn shorten(&mut self, delim: usize, by: usize) {
        self.d[delim].len -= by;
        if let IK::Text(t) = &mut self.n[self.d[delim].node].kind {
            t.truncate(t.len() - by);
        }
    }
    /// Pair up emphasis delimiters above `bottom` (an index into `d`; indices grow along the stack, so they can be compared).
    fn process_emphasis(&mut self, bottom: Option<usize>) {
        let floor = bottom.map_or(-1, |b| b as isize);
        // for each (char, closer can also open, closer length mod 3): no opener exists at or below this index
        let mut openers_bottom = [[[floor; 3]; 2]; 2];
        // the first delimiter above `bottom`
        let mut closer = match bottom {
            Some(b) => self.d[b].next.filter(|_| self.d_tail != Some(b)),
            None => {
                let mut first = self.d_tail;
                while let Some(p) = first.and_then(|f| self.d[f].prev) {
                    tick(1);
                    first = Some(p);
                }
                first
            }
        };
        while let Some(c) = closer {
            tick(1);
            if !self.d[c].can_close {
                closer = self.d[c].next;
                continue;
            }
            let (ch, can_open, orig) = (self.d[c].ch, self.d[c].can_open, self.d[c].orig);
            let key = ((ch == '_') as usize, can_open as usize, orig % 3);
            let mut op = self.d[c].prev;
            let found = loop {
                let Some(o) = op.filter(|o| *o as isize > openers_bottom[key.0][key.1][key.2])
                else {
                    break None;
                };
                tick(1);
                let d = &self.d[o];
                let rule_of_3 = (can_open || d.can_close)
                    && (d.orig + orig).is_multiple_of(3)
                    && !(d.orig.is_multiple_of(3) && orig.is_multiple_of(3));
                if d.ch == ch && d.can_open && !rule_of_3 {
                    break Some(o);
                }
                op = d.prev;
            };
            let Some(o) = found else {
                openers_bottom[key.0][key.1][key.2] = self.d[c].prev.map_or(floor, |p| p as isize);
                closer = self.d[c].next;
                if !can_open {
                    self.remove_delim(c);
                }
                continue;
            };
            let used = if self.d[c].len >= 2 && self.d[o].len >= 2 {
                2
            } else {
                1
            };
            self.shorten(o, used);
            self.shorten(c, used);
            let (on, cn) = (self.d[o].node, self.d[c].node);
            self.wrap(
                on,
                Some(cn),
                if used == 2 {
                    IK::Strong(ch)
                } else {
                    IK::Emph(ch)
                },
            );
            // delimiters between the two can never match any more
            self.d[o].next = Some(c);
            self.d[c].prev = Some(o);
            if self.d[o].len == 0 {
                self.unlink(on);
                self.remove_delim(o);
            }
            if self.d[c].len == 0 {
                closer = self.d[c].next;
                self.unlink(cn);
                self.remove_delim(c);
            }
        }
        // everything above `bottom` is spent
        if let Some(b) = bottom {
            self.d[b].next = None;
        }
        self.d_tail = bottom;
    }
}

fn autolink(c: &[char], i: usize) -> Option<(String, String, usize)> {
    let mut j = i + 1;
    while j < c.len() && c[j] != '>' && c[j] != '<' && !c[j].is_whitespace() && !c[j].is_control() {
        j += 1;
    }
    tick(j - i);
    if c.get(j) != Some(&'>') {
        return None;
    }
    let body: String = c[i + 1..j].iter().collect();
    if let Some((scheme, _)) = body.split_once(':') {
        if (2..=32).contains(&scheme.len())
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        {
            return Some((body.clone(), body, j + 1));
        }
    }
    let (local, domain) = body.split_once('@')?;
    let label_ok = |l: &str| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    let local_ok = !local.is_empty()
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(c));
    (local_ok && domain.split('.').all(label_ok)).then(|| (format!("mailto:{body}"), body, j + 1))
}

fn inlines(src: &str, refs: &Refs) -> Vec<Inline> {
    let c: Vec<char> = src.chars().collect();
    tick(c.len());
    // start positions of every backtick run, by length: finding a closing run is then a binary search, not a rescan
    let mut ticks: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '`' {
            let n = c[i..].iter().take_while(|x| **x == '`').count();
            ticks.entry(n).or_default().push(i);
            i += n;
        } else {
            i += 1;
        }
    }
    let mut s = Inl {
        n: vec![],
        d: vec![],
        d_tail: None,
    };
    s.new_node(IK::Root);
    let mut brackets: Vec<Bracket> = vec![];
    let mut link_floor = 0; // non-image brackets below this stack index are inactive (links cannot contain links)
    let mut text = String::new();
    let flush = |s: &mut Inl, text: &mut String| {
        if !text.is_empty() {
            s.push(0, IK::Text(std::mem::take(text)));
        }
    };
    let mut i = 0;
    while i < c.len() {
        tick(1);
        match c[i] {
            '\\' if c.get(i + 1) == Some(&'\n') => {
                flush(&mut s, &mut text);
                s.push(0, IK::Hard);
                i = skip_ws(&c, i + 2);
            }
            '\\' if c.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                text.push(c[i + 1]);
                i += 2;
            }
            '\n' => {
                let hard = text.ends_with("  ");
                text.truncate(text.trim_end_matches(' ').len());
                flush(&mut s, &mut text);
                s.push(0, if hard { IK::Hard } else { IK::Soft });
                i = skip_ws(&c, i + 1);
            }
            '`' => {
                let n = c[i..].iter().take_while(|x| **x == '`').count();
                let runs = ticks.get(&n).map_or(&[][..], Vec::as_slice);
                match runs.get(runs.partition_point(|p| *p < i + n)) {
                    Some(&end) => {
                        tick(end - i);
                        let mut code: String = c[i + n..end]
                            .iter()
                            .map(|x| if *x == '\n' { ' ' } else { *x })
                            .collect();
                        if code.len() >= 2
                            && code.starts_with(' ')
                            && code.ends_with(' ')
                            && code.chars().any(|x| x != ' ')
                        {
                            code = code[1..code.len() - 1].to_string();
                        }
                        flush(&mut s, &mut text);
                        s.push(0, IK::Code(code));
                        i = end + n;
                    }
                    None => {
                        text.extend(&c[i..i + n]);
                        i += n;
                    }
                }
            }
            '<' => match autolink(&c, i) {
                Some((dest, shown, next)) => {
                    flush(&mut s, &mut text);
                    let link = s.push(
                        0,
                        IK::Link {
                            dest,
                            title: String::new(),
                            image: false,
                        },
                    );
                    s.push(link, IK::Text(shown));
                    i = next;
                }
                None => {
                    text.push('<');
                    i += 1;
                }
            },
            '&' => match entity(&c[i..]) {
                Some((d, n)) => {
                    text += &d;
                    i += n;
                }
                None => {
                    text.push('&');
                    i += 1;
                }
            },
            ch @ ('*' | '_') => {
                let n = c[i..].iter().take_while(|x| **x == ch).count();
                tick(n);
                let before = if i == 0 { ' ' } else { c[i - 1] };
                let after = c.get(i + n).copied().unwrap_or(' ');
                let left = !after.is_whitespace()
                    && (!is_punct(after) || before.is_whitespace() || is_punct(before));
                let right = !before.is_whitespace()
                    && (!is_punct(before) || after.is_whitespace() || is_punct(after));
                let (can_open, can_close) = if ch == '*' {
                    (left, right)
                } else {
                    (
                        left && (!right || is_punct(before)),
                        right && (!left || is_punct(after)),
                    )
                };
                flush(&mut s, &mut text);
                let node = s.push(0, IK::Text(ch.to_string().repeat(n)));
                if can_open || can_close {
                    let id = s.d.len();
                    s.d.push(Delim {
                        node,
                        ch,
                        len: n,
                        orig: n,
                        can_open,
                        can_close,
                        prev: s.d_tail,
                        next: None,
                    });
                    if let Some(t) = s.d_tail {
                        s.d[t].next = Some(id);
                    }
                    s.d_tail = Some(id);
                }
                i += n;
            }
            '[' | '!' if c[i] == '[' || c.get(i + 1) == Some(&'[') => {
                let image = c[i] == '!';
                flush(&mut s, &mut text);
                let node = s.push(0, IK::Text(if image { "![".into() } else { "[".into() }));
                if let Some(b) = brackets.last_mut() {
                    b.has_inner_bracket = true;
                }
                i += 1 + image as usize;
                brackets.push(Bracket {
                    node,
                    image,
                    delim_bottom: s.d_tail,
                    text_start: i,
                    has_inner_bracket: false,
                });
            }
            ']' => {
                let close = i;
                i += 1;
                let Some(b) = brackets.pop() else {
                    text.push(']');
                    continue;
                };
                let active = b.image || brackets.len() >= link_floor;
                link_floor = link_floor.min(brackets.len());
                let target = if active {
                    link_target(&c, i, &b, close, refs)
                } else {
                    None
                };
                let Some((dest, title, next)) = target else {
                    text.push(']');
                    continue;
                };
                flush(&mut s, &mut text);
                // emphasis inside the link text is resolved first, so it cannot reach across the link boundary
                s.process_emphasis(b.delim_bottom);
                s.wrap(
                    b.node,
                    None,
                    IK::Link {
                        dest,
                        title,
                        image: b.image,
                    },
                );
                s.unlink(b.node);
                if !b.image {
                    link_floor = brackets.len();
                }
                i = next;
            }
            ch => {
                text.push(ch);
                i += 1;
            }
        }
    }
    flush(&mut s, &mut text);
    s.process_emphasis(None);
    convert(&s, 0, 0)
}
/// What follows a closing bracket: an inline `(dest "title")`, a full or collapsed reference, or a shortcut reference.
fn link_target(
    c: &[char],
    i: usize,
    b: &Bracket,
    close: usize,
    refs: &Refs,
) -> Option<(String, String, usize)> {
    if c.get(i) == Some(&'(') {
        let inline = || {
            let j = skip_ws(c, i + 1);
            let (dest, j) = if c.get(j) == Some(&')') {
                (String::new(), j)
            } else {
                link_dest(c, j)?
            };
            let k = skip_ws(c, j);
            let (title, k) = match link_title(c, k).filter(|_| k > j) {
                Some((t, after)) => (t, skip_ws(c, after)),
                None => (String::new(), k),
            };
            (c.get(k) == Some(&')')).then_some((dest, title, k + 1))
        };
        if let Some(r) = inline() {
            return Some(r);
        }
    }
    let own_label = || {
        (!b.has_inner_bracket && close - b.text_start <= 999)
            .then(|| c[b.text_start..close].iter().collect::<String>())
    };
    let (label, next) = match link_label(c, i) {
        Some((l, after)) if !l.trim().is_empty() => (l, after),
        Some((l, after)) if l.is_empty() => (own_label()?, after),
        _ => (own_label()?, i),
    };
    let (d, t) = refs.get(&normalize_label(&label))?;
    Some((d.clone(), t.clone(), next))
}
fn push_text(out: &mut Vec<Inline>, t: &str) {
    if t.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(Inline::Text(prev)) => prev.push_str(t),
        _ => out.push(Inline::Text(t.to_string())),
    }
}
/// Linked list to tree. Recursion is capped: below the cap only the text survives.
fn convert(s: &Inl, parent: usize, depth: usize) -> Vec<Inline> {
    let mut out = vec![];
    let mut cur = s.n[parent].first;
    while let Some(i) = cur {
        cur = s.n[i].next;
        let nested = !matches!(
            s.n[i].kind,
            IK::Root | IK::Text(_) | IK::Soft | IK::Hard | IK::Code(_)
        );
        if nested && depth >= MAX_INLINE_DEPTH {
            let mut stack = vec![i];
            while let Some(x) = stack.pop() {
                if let IK::Text(t) | IK::Code(t) = &s.n[x].kind {
                    push_text(&mut out, t);
                }
                let mut kids = vec![];
                let mut k = s.n[x].first;
                while let Some(child) = k {
                    kids.push(child);
                    k = s.n[child].next;
                }
                stack.extend(kids.into_iter().rev());
            }
            continue;
        }
        match &s.n[i].kind {
            IK::Root => {}
            IK::Text(t) => push_text(&mut out, t),
            IK::Soft => out.push(Inline::Soft),
            IK::Hard => out.push(Inline::Hard),
            IK::Code(t) => out.push(Inline::Code(t.clone())),
            IK::Emph(d) => out.push(Inline::Emph(*d, convert(s, i, depth + 1))),
            IK::Strong(d) => out.push(Inline::Strong(*d, convert(s, i, depth + 1))),
            IK::Link { dest, title, image } => {
                let (dest, title, kids) = (dest.clone(), title.clone(), convert(s, i, depth + 1));
                out.push(if *image {
                    Inline::Image { dest, title, kids }
                } else {
                    Inline::Link { dest, title, kids }
                });
            }
        }
    }
    out
}

// ------------------------------------------------------------------ HTML renderer
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
/// Percent-encode a URL the way the reference implementation does. This is NOT a safety filter: `javascript:` URLs pass through.
fn url(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.+!*'(),;/?:@=#$~%".contains(&b) {
            out.push(b as char);
        } else if b == b'&' {
            out.push_str("&amp;");
        } else {
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
}
fn plain(inl: &[Inline], out: &mut String) {
    for x in inl {
        match x {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Soft | Inline::Hard => out.push('\n'),
            Inline::Emph(_, k)
            | Inline::Strong(_, k)
            | Inline::Link { kids: k, .. }
            | Inline::Image { kids: k, .. } => plain(k, out),
        }
    }
}
fn html_inlines(inl: &[Inline], o: &mut String) {
    let title_attr = |t: &str| {
        if t.is_empty() {
            String::new()
        } else {
            format!(" title=\"{}\"", esc(t))
        }
    };
    for x in inl {
        match x {
            Inline::Text(t) => o.push_str(&esc(t)),
            Inline::Soft => o.push('\n'),
            Inline::Hard => o.push_str("<br />\n"),
            Inline::Code(t) => write!(o, "<code>{}</code>", esc(t)).unwrap(),
            Inline::Emph(_, k) | Inline::Strong(_, k) => {
                let tag = if matches!(x, Inline::Emph(..)) {
                    "em"
                } else {
                    "strong"
                };
                write!(o, "<{tag}>").unwrap();
                html_inlines(k, o);
                write!(o, "</{tag}>").unwrap();
            }
            Inline::Link { dest, title, kids } => {
                write!(o, "<a href=\"{}\"{}>", url(dest), title_attr(title)).unwrap();
                html_inlines(kids, o);
                o.push_str("</a>");
            }
            Inline::Image { dest, title, kids } => {
                let mut alt = String::new();
                plain(kids, &mut alt);
                write!(
                    o,
                    "<img src=\"{}\" alt=\"{}\"{} />",
                    url(dest),
                    esc(&alt),
                    title_attr(title)
                )
                .unwrap();
            }
        }
    }
}
fn cr(o: &mut String) {
    if !o.is_empty() && !o.ends_with('\n') {
        o.push('\n');
    }
}
fn html_blocks(blocks: &[Block], tight: bool, o: &mut String) {
    for b in blocks {
        match &b.kind {
            Kind::Paragraph(i) if tight => html_inlines(i, o),
            Kind::Paragraph(i) => {
                cr(o);
                o.push_str("<p>");
                html_inlines(i, o);
                o.push_str("</p>\n");
            }
            Kind::Heading(l, i) => {
                cr(o);
                write!(o, "<h{l}>").unwrap();
                html_inlines(i, o);
                writeln!(o, "</h{l}>").unwrap();
            }
            Kind::Break => {
                cr(o);
                o.push_str("<hr />\n");
            }
            Kind::Code { info, text } => {
                cr(o);
                let class = info
                    .split_whitespace()
                    .next()
                    .map_or(String::new(), |l| format!(" class=\"language-{}\"", esc(l)));
                writeln!(o, "<pre><code{class}>{}</code></pre>", esc(text)).unwrap();
            }
            Kind::Html(t) => {
                cr(o);
                o.push_str(t);
                cr(o);
            }
            Kind::Quote(k) => {
                cr(o);
                o.push_str("<blockquote>\n");
                html_blocks(k, false, o);
                cr(o);
                o.push_str("</blockquote>\n");
            }
            Kind::List(l) => {
                cr(o);
                let tag = if l.meta.ordered { "ol" } else { "ul" };
                let start = if l.meta.ordered && l.meta.start != 1 {
                    format!(" start=\"{}\"", l.meta.start)
                } else {
                    String::new()
                };
                writeln!(o, "<{tag}{start}>").unwrap();
                for it in &l.items {
                    o.push_str("<li>");
                    if let Some(done) = it.task {
                        write!(
                            o,
                            "<input {}disabled=\"\" type=\"checkbox\"> ",
                            if done { "checked=\"\" " } else { "" }
                        )
                        .unwrap();
                    }
                    html_blocks(&it.blocks, l.meta.tight, o);
                    o.push_str("</li>\n");
                }
                writeln!(o, "</{tag}>").unwrap();
            }
        }
    }
}
fn to_html(blocks: &[Block]) -> String {
    let mut o = String::new();
    html_blocks(blocks, false, &mut o);
    o
}

// ------------------------------------------------------------------ Markdown printer (normalising)
/// Which literal `*` / `_` (numbered in printing order) to leave unescaped. See `printed_inlines`.
struct Bare<'a> {
    seen: usize,
    keep: &'a dyn Fn(usize) -> bool,
    /// How to write a hard break: `\` is punctuation and a space is not, so the choice can change how a neighbouring delimiter pairs.
    hard: &'static str,
}
/// Escape what could be read as syntax. `line_start` says whether `t` begins a line, where more characters are special.
fn escape_text(t: &str, line_start: bool, bare: &mut Bare, o: &mut String) {
    let digits = if line_start {
        t.bytes().take_while(u8::is_ascii_digit).count()
    } else {
        0
    };
    for (i, ch) in t.char_indices() {
        let special = "\\`*_[]<&!#".contains(ch)
            || (line_start && i == 0 && "-+=>~".contains(ch))
            || (digits > 0 && i == digits && (ch == '.' || ch == ')'));
        if ch == '\n' {
            o.push_str("&#10;");
            continue;
        }
        let mut leave = false;
        if ch == '*' || ch == '_' {
            leave = (bare.keep)(bare.seen);
            bare.seen += 1;
        }
        if special && !leave {
            o.push('\\');
        }
        o.push(ch);
    }
}
fn has_break(inl: &[Inline]) -> bool {
    inl.iter().any(|x| match x {
        Inline::Soft | Inline::Hard => true,
        Inline::Emph(_, k)
        | Inline::Strong(_, k)
        | Inline::Link { kids: k, .. }
        | Inline::Image { kids: k, .. } => has_break(k),
        Inline::Text(_) | Inline::Code(_) => false,
    })
}
/// Escaping is not always neutral: in `**o*.**` one literal `*` belongs to a delimiter run, and the pairing rules (flanking, rule of 3) depend on
/// the LENGTH of that run, so escaping it changes the meaning. No local rule gets this right, so the printer CHECKS its output by parsing it again
/// as a whole block (`wrap` turns the inline text into that block, which also catches a bare `*` that would start a list): first with everything
/// escaped, then with all literal delimiters bare, then with exactly one bare or exactly one escaped.
fn printed_block(want: &Kind, inl: &[Inline], wrap: impl Fn(String) -> String) -> String {
    let attempt = |keep: &dyn Fn(usize) -> bool, hard: &'static str| {
        let (mut text, mut bare) = (
            String::new(),
            Bare {
                seen: 0,
                keep,
                hard,
            },
        );
        print_inlines(inl, &mut bare, &mut text);
        (wrap(text), bare.seen)
    };
    let means_the_same = |s: &str| matches!(&parse(s).0[..], [b] if b.kind == *want);
    let (safe, literals) = attempt(&|_| false, "\\\n");
    if means_the_same(&safe) {
        return safe;
    }
    // bounded: at most 4 + 4 * MAX_SEARCH reparses of this one block
    const MAX_SEARCH: usize = 24;
    for hard in ["\\\n", "  \n"] {
        for all_bare in [false, true] {
            let candidate = attempt(&|_| all_bare, hard).0;
            if means_the_same(&candidate) {
                return candidate;
            }
        }
        for k in 0..literals.min(MAX_SEARCH) {
            for candidate in [attempt(&|n| n == k, hard).0, attempt(&|n| n != k, hard).0] {
                if means_the_same(&candidate) {
                    return candidate;
                }
            }
        }
    }
    safe
}
fn longest_run(s: &str, ch: char) -> usize {
    s.split(|c| c != ch).map(str::len).max().unwrap_or(0)
}
fn print_dest(dest: &str, title: &str, o: &mut String) {
    let quoted = |s: &str, special: &str, o: &mut String| {
        for ch in s.chars() {
            if ch == '\n' {
                o.push_str("&#10;");
                continue;
            }
            if special.contains(ch) {
                o.push('\\');
            }
            o.push(ch);
        }
    };
    o.push('(');
    if dest.is_empty() || dest.contains(|c: char| c.is_whitespace() || c.is_control()) {
        o.push('<');
        quoted(dest, "\\<>&", o);
        o.push('>');
    } else {
        quoted(dest, "\\()<>&", o);
    }
    if !title.is_empty() {
        o.push_str(" \"");
        quoted(title, "\\\"&", o);
        o.push('"');
    }
    o.push(')');
}
fn as_autolink(dest: &str, kids: &[Inline]) -> Option<String> {
    let [Inline::Text(shown)] = kids else {
        return None;
    };
    let source = format!("<{shown}>");
    let chars: Vec<char> = source.chars().collect();
    autolink(&chars, 0)
        .filter(|(d, _, next)| d == dest && *next == chars.len())
        .map(|_| source)
}
fn print_inlines(inl: &[Inline], bare: &mut Bare, o: &mut String) {
    for x in inl {
        match x {
            Inline::Text(t) => {
                let start = o.is_empty() || o.ends_with('\n');
                escape_text(t, start, bare, o);
            }
            Inline::Soft => o.push('\n'),
            Inline::Hard => o.push_str(bare.hard),
            Inline::Code(t) => {
                // the shortest backtick string that does not occur as a run inside (one pass: rescanning per candidate length is super-linear)
                let runs: std::collections::HashSet<usize> =
                    t.split(|c| c != '`').map(str::len).collect();
                let mut n = 1;
                while runs.contains(&n) {
                    n += 1;
                }
                let fence = "`".repeat(n);
                let pad = t.starts_with('`')
                    || t.ends_with('`')
                    || (t.starts_with(' ') && t.ends_with(' ') && t.chars().any(|c| c != ' '));
                let sp = if pad { " " } else { "" };
                write!(o, "{fence}{sp}{t}{sp}{fence}").unwrap();
            }
            Inline::Emph(d, k) | Inline::Strong(d, k) => {
                let m = d
                    .to_string()
                    .repeat(if matches!(x, Inline::Emph(..)) { 1 } else { 2 });
                o.push_str(&m);
                print_inlines(k, bare, o);
                o.push_str(&m);
            }
            Inline::Link { dest, title, kids }
                if title.is_empty() && as_autolink(dest, kids).is_some() =>
            {
                // `[x](x)` would be a bracket link, which may not sit inside another link; an autolink may
                o.push_str(&as_autolink(dest, kids).unwrap_or_default());
            }
            Inline::Link { dest, title, kids } | Inline::Image { dest, title, kids } => {
                o.push_str(if matches!(x, Inline::Image { .. }) {
                    "!["
                } else {
                    "["
                });
                print_inlines(kids, bare, o);
                o.push(']');
                print_dest(dest, title, o);
            }
        }
    }
}
/// `min_width`: a following HTML block keeps its own indentation, so list items must need MORE than that to continue, or they would absorb it.
fn print_block(b: &Block, min_width: usize, after_paragraph: bool) -> String {
    match &b.kind {
        Kind::Paragraph(i) => printed_block(&b.kind, i, |s| s),
        Kind::Heading(l, i) => printed_block(&b.kind, i, |s| {
            if has_break(i) {
                format!("{s}\n{}", if *l == 1 { "===" } else { "---" }) // only a setext heading can span lines
            } else {
                format!("{} {s}", "#".repeat(*l as usize))
                    .trim_end()
                    .to_string()
            }
        }),
        Kind::Break => "___".into(), // `---` could turn a paragraph into a heading, `***` after a `*` bullet changes meaning
        Kind::Code { info, text } => {
            let ch = if info.contains('`') { '~' } else { '`' };
            let fence = ch.to_string().repeat(3.max(longest_run(text, ch) + 1));
            let mut head = fence.clone();
            if info.starts_with(ch) {
                head.push(' ');
            }
            for c in info.chars() {
                if c == '\\' || c == '&' {
                    head.push('\\');
                }
                head.push(c);
            }
            format!("{head}\n{text}{fence}")
        }
        Kind::Html(t) => t.trim_end_matches('\n').to_string(),
        Kind::Quote(k) => prefix_lines(&print_blocks(k, false), "> ", "> "),
        Kind::List(l) => {
            let items: Vec<String> = l
                .items
                .iter()
                .enumerate()
                .map(|(n, it)| {
                    let marker = if l.meta.ordered {
                        format!("{}{}", l.meta.start as u64 + n as u64, l.meta.delim)
                    } else {
                        l.meta.delim.to_string()
                    };
                    let mut body = print_blocks(&it.blocks, l.meta.tight);
                    // A loose list normally shows it with a blank line. With ONE item holding fewer than two blocks there is nowhere to put
                    // one (the source got it from a definition that is gone by now), so write definitions that render as nothing.
                    if !l.meta.tight && l.items.len() == 1 && it.blocks.len() < 2 {
                        let invisible = "[//]: #";
                        body = if body.is_empty() {
                            format!("{invisible}\n\n{invisible}")
                        } else {
                            format!("{body}\n\n{invisible}")
                        };
                    }
                    if let Some(done) = it.task {
                        body = format!("[{}] {body}", if done { 'x' } else { ' ' });
                    }
                    // an empty item cannot interrupt a paragraph (it got here because definitions, now gone, were in it)
                    if body.is_empty() && n == 0 && after_paragraph {
                        body = "[//]: #".into();
                    }
                    let first = format!(
                        "{marker}{}",
                        " ".repeat(min_width.saturating_sub(marker.len()).clamp(1, 4))
                    );
                    prefix_lines(&body, &first, &" ".repeat(first.len()))
                })
                .collect();
            items.join(if l.meta.tight { "\n" } else { "\n\n" })
        }
    }
}
fn prefix_lines(body: &str, first: &str, rest: &str) -> String {
    if body.is_empty() {
        return first.trim_end().to_string();
    }
    body.split('\n')
        .enumerate()
        .map(|(i, line)| {
            let p = if i == 0 { first } else { rest };
            if line.is_empty() {
                p.trim_end().to_string()
            } else {
                format!("{p}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn print_blocks(blocks: &[Block], tight: bool) -> String {
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| &blocks[p].kind);
        match (prev, &b.kind) {
            (None, _) => {}
            // Two lists of the same type were kept apart by something that printing drops (a definition): without it they would merge.
            (Some(Kind::List(x)), Kind::List(y))
                if (x.meta.ordered, x.meta.delim) == (y.meta.ordered, y.meta.delim) =>
            {
                out.push_str("\n\n[//]: #\n\n")
            }
            // a paragraph right after a quote would be read as its lazy continuation: end the quote's paragraph first
            (Some(Kind::Quote(_)), Kind::Paragraph(_)) if tight => out.push_str("\n>\n"),
            _ => out.push_str(if tight { "\n" } else { "\n\n" }),
        }
        let min_width = match blocks.get(i + 1).map(|n| &n.kind) {
            Some(Kind::Html(h)) => h.len() - h.trim_start_matches(' ').len() + 1,
            _ => 0,
        };
        out += &print_block(
            b,
            min_width,
            tight && matches!(prev, Some(Kind::Paragraph(_))),
        );
    }
    out
}
fn to_markdown(blocks: &[Block]) -> String {
    let s = print_blocks(blocks, false);
    if s.is_empty() { s } else { s + "\n" }
}

// ------------------------------------------------------------------ flat block list, as a block editor stores a page
#[derive(Clone, Debug, PartialEq)]
enum RowKind {
    Paragraph,
    Heading(u8),
    Quote, // a container: its content is the following rows with a greater depth
    /// One list item. Its first paragraph is the row's text, further blocks follow one level deeper.
    Item {
        list: ListMeta,
        first_of_list: bool,
        task: Option<bool>,
        has_text: bool,
    },
    Code(String),
    Html,
    Divider,
}
/// A mark carries an instance number so that two adjacent marks of the same kind stay two.
#[derive(Clone, Debug, PartialEq)]
enum Mark {
    Emph(char),
    Strong(char),
    Link { dest: String, title: String },
    Image { dest: String, title: String },
}
#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Text(String),
    Code(String),
    Soft,
    Hard,
    Empty, // keeps a mark with no content, such as `[](url)`
}
#[derive(Clone, Debug, PartialEq)]
struct Run {
    piece: Piece,
    marks: Vec<(u32, Mark)>, // outermost first
}
#[derive(Clone, Debug, PartialEq)]
struct Row {
    id: u32,
    depth: usize,
    kind: RowKind,
    text: Vec<Run>,
}
fn flatten(inl: &[Inline], marks: &mut Vec<(u32, Mark)>, next: &mut u32, out: &mut Vec<Run>) {
    for x in inl {
        let piece = match x {
            Inline::Text(t) => Piece::Text(t.clone()),
            Inline::Code(t) => Piece::Code(t.clone()),
            Inline::Soft => Piece::Soft,
            Inline::Hard => Piece::Hard,
            Inline::Emph(_, kids)
            | Inline::Strong(_, kids)
            | Inline::Link { kids, .. }
            | Inline::Image { kids, .. } => {
                let mark = match x {
                    Inline::Emph(d, _) => Mark::Emph(*d),
                    Inline::Strong(d, _) => Mark::Strong(*d),
                    Inline::Link { dest, title, .. } => Mark::Link {
                        dest: dest.clone(),
                        title: title.clone(),
                    },
                    Inline::Image { dest, title, .. } => Mark::Image {
                        dest: dest.clone(),
                        title: title.clone(),
                    },
                    _ => unreachable!(),
                };
                *next += 1;
                marks.push((*next, mark));
                if kids.is_empty() {
                    out.push(Run {
                        piece: Piece::Empty,
                        marks: marks.clone(),
                    });
                } else {
                    flatten(kids, marks, next, out);
                }
                marks.pop();
                continue;
            }
        };
        out.push(Run {
            piece,
            marks: marks.clone(),
        });
    }
}
/// Rebuild the inline tree: neighbouring runs that share the mark instance at `level` belong to one node.
fn unflatten(runs: &[Run], level: usize) -> Vec<Inline> {
    let (mut out, mut i) = (vec![], 0);
    while i < runs.len() {
        let Some((id, mark)) = runs[i].marks.get(level) else {
            match &runs[i].piece {
                Piece::Text(t) => out.push(Inline::Text(t.clone())),
                Piece::Code(t) => out.push(Inline::Code(t.clone())),
                Piece::Soft => out.push(Inline::Soft),
                Piece::Hard => out.push(Inline::Hard),
                Piece::Empty => {}
            }
            i += 1;
            continue;
        };
        let n = runs[i..]
            .iter()
            .take_while(|r| r.marks.get(level).is_some_and(|m| m.0 == *id))
            .count();
        let kids = unflatten(&runs[i..i + n], level + 1);
        out.push(match mark {
            Mark::Emph(d) => Inline::Emph(*d, kids),
            Mark::Strong(d) => Inline::Strong(*d, kids),
            Mark::Link { dest, title } => Inline::Link {
                dest: dest.clone(),
                title: title.clone(),
                kids,
            },
            Mark::Image { dest, title } => Inline::Image {
                dest: dest.clone(),
                title: title.clone(),
                kids,
            },
        });
        i += n;
    }
    out
}
fn to_flat(blocks: &[Block]) -> Vec<Row> {
    fn walk(blocks: &[Block], depth: usize, mark: &mut u32, rows: &mut Vec<Row>) {
        for b in blocks {
            let mut text = vec![];
            let push = |rows: &mut Vec<Row>, kind, text| {
                rows.push(Row {
                    id: rows.len() as u32 + 1,
                    depth,
                    kind,
                    text,
                })
            };
            match &b.kind {
                Kind::Paragraph(i) => {
                    flatten(i, &mut vec![], mark, &mut text);
                    push(rows, RowKind::Paragraph, text);
                }
                Kind::Heading(l, i) => {
                    flatten(i, &mut vec![], mark, &mut text);
                    push(rows, RowKind::Heading(*l), text);
                }
                Kind::Break => push(rows, RowKind::Divider, text),
                Kind::Code { info, text: code } => push(
                    rows,
                    RowKind::Code(info.clone()),
                    vec![Run {
                        piece: Piece::Text(code.clone()),
                        marks: vec![],
                    }],
                ),
                Kind::Html(h) => push(
                    rows,
                    RowKind::Html,
                    vec![Run {
                        piece: Piece::Text(h.clone()),
                        marks: vec![],
                    }],
                ),
                Kind::Quote(k) => {
                    push(rows, RowKind::Quote, text);
                    walk(k, depth + 1, mark, rows);
                }
                Kind::List(l) => {
                    for (n, it) in l.items.iter().enumerate() {
                        let (lead, rest) = match it.blocks.split_first() {
                            Some((
                                Block {
                                    kind: Kind::Paragraph(i),
                                    ..
                                },
                                rest,
                            )) => (Some(i), rest),
                            _ => (None, &it.blocks[..]),
                        };
                        let mut text = vec![];
                        if let Some(i) = lead {
                            flatten(i, &mut vec![], mark, &mut text);
                        }
                        let kind = RowKind::Item {
                            list: l.meta,
                            first_of_list: n == 0,
                            task: it.task,
                            has_text: lead.is_some(),
                        };
                        rows.push(Row {
                            id: rows.len() as u32 + 1,
                            depth,
                            kind,
                            text,
                        });
                        walk(rest, depth + 1, mark, rows);
                    }
                }
            }
        }
    }
    let mut rows = vec![];
    walk(blocks, 0, &mut 0, &mut rows);
    rows
}
fn from_flat(rows: &[Row]) -> Vec<Block> {
    fn level(rows: &[Row], at: &mut usize, depth: usize) -> Vec<Block> {
        let mut out: Vec<Block> = vec![];
        while let Some(row) = rows.get(*at).filter(|r| r.depth == depth) {
            *at += 1;
            let raw = || {
                row.text
                    .iter()
                    .filter_map(|r| {
                        if let Piece::Text(t) = &r.piece {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<String>()
            };
            let kind = match &row.kind {
                RowKind::Paragraph => Kind::Paragraph(unflatten(&row.text, 0)),
                RowKind::Heading(l) => Kind::Heading(*l, unflatten(&row.text, 0)),
                RowKind::Divider => Kind::Break,
                RowKind::Code(info) => Kind::Code {
                    info: info.clone(),
                    text: raw(),
                },
                RowKind::Html => Kind::Html(raw()),
                RowKind::Quote => Kind::Quote(level(rows, at, depth + 1)),
                RowKind::Item {
                    list,
                    first_of_list,
                    task,
                    has_text,
                } => {
                    let mut blocks = vec![];
                    if *has_text {
                        blocks.push(Block {
                            span: Span::default(),
                            kind: Kind::Paragraph(unflatten(&row.text, 0)),
                        });
                    }
                    blocks.extend(level(rows, at, depth + 1));
                    let item = Item {
                        span: Span::default(),
                        task: *task,
                        blocks,
                    };
                    match out.last_mut() {
                        Some(Block {
                            kind: Kind::List(l),
                            ..
                        }) if !*first_of_list => {
                            l.items.push(item);
                            continue;
                        }
                        _ => Kind::List(List {
                            meta: *list,
                            items: vec![item],
                        }),
                    }
                }
            };
            out.push(Block {
                span: Span::default(),
                kind,
            });
        }
        out
    }
    level(rows, &mut 0, 0)
}

// ------------------------------------------------------------------ checks
// (spec example number, markdown, expected HTML)
const SPEC: &[(u16, &str, &str)] = &[
    // SPEC-EXAMPLES-BEGIN
    (
        1,
        "\tfoo\tbaz\t\tbim\n",
        "<pre><code>foo\tbaz\t\tbim\n</code></pre>\n",
    ),
    (
        2,
        "  \tfoo\tbaz\t\tbim\n",
        "<pre><code>foo\tbaz\t\tbim\n</code></pre>\n",
    ),
    (
        4,
        "  - foo\n\n\tbar\n",
        "<ul>\n<li>\n<p>foo</p>\n<p>bar</p>\n</li>\n</ul>\n",
    ),
    (
        5,
        "- foo\n\n\t\tbar\n",
        "<ul>\n<li>\n<p>foo</p>\n<pre><code>  bar\n</code></pre>\n</li>\n</ul>\n",
    ),
    (
        7,
        "-\t\tfoo\n",
        "<ul>\n<li>\n<pre><code>  foo\n</code></pre>\n</li>\n</ul>\n",
    ),
    (
        8,
        "    foo\n\tbar\n",
        "<pre><code>foo\nbar\n</code></pre>\n",
    ),
    (10, "#\tFoo\n", "<h1>Foo</h1>\n"),
    (
        12,
        "\\!\\\"\\#\\$\\%\\&\\'\\(\\)\\*\\+\\,\\-\\.\\/\\:\\;\\<\\=\\>\\?\\@\\[\\\\\\]\\^\\_\\`\\{\\|\\}\\~\n",
        "<p>!&quot;#$%&amp;'()*+,-./:;&lt;=&gt;?@[\\]^_`{|}~</p>\n",
    ),
    (
        14,
        "\\*not emphasized*\n\\<br/> not a tag\n\\[not a link](/foo)\n\\`not code`\n1\\. not a list\n\\* not a list\n\\# not a heading\n\\[foo]: /url \"not a reference\"\n\\&ouml; not a character entity\n",
        "<p>*not emphasized*\n&lt;br/&gt; not a tag\n[not a link](/foo)\n`not code`\n1. not a list\n* not a list\n# not a heading\n[foo]: /url &quot;not a reference&quot;\n&amp;ouml; not a character entity</p>\n",
    ),
    (16, "foo\\\nbar\n", "<p>foo<br />\nbar</p>\n"),
    (18, "    \\[\\]\n", "<pre><code>\\[\\]\n</code></pre>\n"),
    (
        20,
        "<https://example.com?find=\\*>\n",
        "<p><a href=\"https://example.com?find=%5C*\">https://example.com?find=\\*</a></p>\n",
    ),
    (
        23,
        "[foo]\n\n[foo]: /bar\\* \"ti\\*tle\"\n",
        "<p><a href=\"/bar*\" title=\"ti*tle\">foo</a></p>\n",
    ),
    (
        25,
        "&nbsp; &amp; &copy; &AElig; &Dcaron;\n&frac34; &HilbertSpace; &DifferentialD;\n&ClockwiseContourIntegral; &ngE;\n",
        "<p>\u{a0} &amp; © Æ Ď\n¾ ℋ ⅆ\n∲ ≧̸</p>\n",
    ),
    (27, "&#X22; &#XD06; &#xcab;\n", "<p>&quot; ആ ಫ</p>\n"),
    (30, "&MadeUpEntity;\n", "<p>&amp;MadeUpEntity;</p>\n"),
    (
        34,
        "``` f&ouml;&ouml;\nfoo\n```\n",
        "<pre><code class=\"language-föö\">foo\n</code></pre>\n",
    ),
    (
        36,
        "    f&ouml;f&ouml;\n",
        "<pre><code>f&amp;ouml;f&amp;ouml;\n</code></pre>\n",
    ),
    (39, "foo&#10;&#10;bar\n", "<p>foo\n\nbar</p>\n"),
    (
        42,
        "- `one\n- two`\n",
        "<ul>\n<li>`one</li>\n<li>two`</li>\n</ul>\n",
    ),
    (43, "***\n---\n___\n", "<hr />\n<hr />\n<hr />\n"),
    (45, "===\n", "<p>===</p>\n"),
    (48, "    ***\n", "<pre><code>***\n</code></pre>\n"),
    (51, " - - -\n", "<hr />\n"),
    (53, "-     -      -      -\n", "<hr />\n"),
    (56, " *-*\n", "<p><em>-</em></p>\n"),
    (59, "Foo\n---\nbar\n", "<h2>Foo</h2>\n<p>bar</p>\n"),
    (
        62,
        "# foo\n## foo\n### foo\n#### foo\n##### foo\n###### foo\n",
        "<h1>foo</h1>\n<h2>foo</h2>\n<h3>foo</h3>\n<h4>foo</h4>\n<h5>foo</h5>\n<h6>foo</h6>\n",
    ),
    (
        64,
        "#5 bolt\n\n#hashtag\n",
        "<p>#5 bolt</p>\n<p>#hashtag</p>\n",
    ),
    (
        66,
        "# foo *bar* \\*baz\\*\n",
        "<h1>foo <em>bar</em> *baz*</h1>\n",
    ),
    (
        68,
        " ### foo\n  ## foo\n   # foo\n",
        "<h3>foo</h3>\n<h2>foo</h2>\n<h1>foo</h1>\n",
    ),
    (
        71,
        "## foo ##\n  ###   bar    ###\n",
        "<h2>foo</h2>\n<h3>bar</h3>\n",
    ),
    (73, "### foo ###     \n", "<h3>foo</h3>\n"),
    (75, "# foo#\n", "<h1>foo#</h1>\n"),
    (77, "****\n## foo\n****\n", "<hr />\n<h2>foo</h2>\n<hr />\n"),
    (
        80,
        "Foo *bar*\n=========\n\nFoo *bar*\n---------\n",
        "<h1>Foo <em>bar</em></h1>\n<h2>Foo <em>bar</em></h2>\n",
    ),
    (
        82,
        "  Foo *bar\nbaz*\t\n====\n",
        "<h1>Foo <em>bar\nbaz</em></h1>\n",
    ),
    (
        85,
        "    Foo\n    ---\n\n    Foo\n---\n",
        "<pre><code>Foo\n---\n\nFoo\n</code></pre>\n<hr />\n",
    ),
    (
        88,
        "Foo\n= =\n\nFoo\n--- -\n",
        "<p>Foo\n= =</p>\n<p>Foo</p>\n<hr />\n",
    ),
    (90, "Foo\\\n----\n", "<h2>Foo\\</h2>\n"),
    (
        93,
        "> foo\nbar\n===\n",
        "<blockquote>\n<p>foo\nbar\n===</p>\n</blockquote>\n",
    ),
    (
        96,
        "---\nFoo\n---\nBar\n---\nBaz\n",
        "<hr />\n<h2>Foo</h2>\n<h2>Bar</h2>\n<p>Baz</p>\n",
    ),
    (98, "---\n---\n", "<hr />\n<hr />\n"),
    (
        101,
        "> foo\n-----\n",
        "<blockquote>\n<p>foo</p>\n</blockquote>\n<hr />\n",
    ),
    (
        104,
        "Foo\nbar\n\n---\n\nbaz\n",
        "<p>Foo\nbar</p>\n<hr />\n<p>baz</p>\n",
    ),
    (
        107,
        "    a simple\n      indented code block\n",
        "<pre><code>a simple\n  indented code block\n</code></pre>\n",
    ),
    (
        109,
        "1.  foo\n\n    - bar\n",
        "<ol>\n<li>\n<p>foo</p>\n<ul>\n<li>bar</li>\n</ul>\n</li>\n</ol>\n",
    ),
    (
        111,
        "    chunk1\n\n    chunk2\n  \n \n \n    chunk3\n",
        "<pre><code>chunk1\n\nchunk2\n\n\n\nchunk3\n</code></pre>\n",
    ),
    (
        114,
        "    foo\nbar\n",
        "<pre><code>foo\n</code></pre>\n<p>bar</p>\n",
    ),
    (
        116,
        "        foo\n    bar\n",
        "<pre><code>    foo\nbar\n</code></pre>\n",
    ),
    (
        119,
        "```\n<\n >\n```\n",
        "<pre><code>&lt;\n &gt;\n</code></pre>\n",
    ),
    (121, "``\nfoo\n``\n", "<p><code>foo</code></p>\n"),
    (
        124,
        "````\naaa\n```\n``````\n",
        "<pre><code>aaa\n```\n</code></pre>\n",
    ),
    (
        127,
        "`````\n\n```\naaa\n",
        "<pre><code>\n```\naaa\n</code></pre>\n",
    ),
    (130, "```\n```\n", "<pre><code></code></pre>\n"),
    (
        133,
        "   ```\n   aaa\n    aaa\n  aaa\n   ```\n",
        "<pre><code>aaa\n aaa\naaa\n</code></pre>\n",
    ),
    (
        136,
        "   ```\naaa\n  ```\n",
        "<pre><code>aaa\n</code></pre>\n",
    ),
    (
        139,
        "~~~~~~\naaa\n~~~ ~~\n",
        "<pre><code>aaa\n~~~ ~~\n</code></pre>\n",
    ),
    (
        142,
        "```ruby\ndef foo(x)\n  return 3\nend\n```\n",
        "<pre><code class=\"language-ruby\">def foo(x)\n  return 3\nend\n</code></pre>\n",
    ),
    (145, "``` aa ```\nfoo\n", "<p><code>aa</code>\nfoo</p>\n"),
    (
        149,
        "<table>\n  <tr>\n    <td>\n           hi\n    </td>\n  </tr>\n</table>\n\nokay.\n",
        "<table>\n  <tr>\n    <td>\n           hi\n    </td>\n  </tr>\n</table>\n<p>okay.</p>\n",
    ),
    (
        154,
        "<div id=\"foo\" class=\"bar\n  baz\">\n</div>\n",
        "<div id=\"foo\" class=\"bar\n  baz\">\n</div>\n",
    ),
    (
        159,
        "<div><a href=\"bar\">*foo*</a></div>\n",
        "<div><a href=\"bar\">*foo*</a></div>\n",
    ),
    (
        172,
        "<style\n  type=\"text/css\">\nh1 {color:red;}\n\np {color:blue;}\n</style>\nokay\n",
        "<style\n  type=\"text/css\">\nh1 {color:red;}\n\np {color:blue;}\n</style>\n<p>okay</p>\n",
    ),
    (
        177,
        "<!-- foo -->*bar*\n*baz*\n",
        "<!-- foo -->*bar*\n<p><em>baz</em></p>\n",
    ),
    (
        185,
        "Foo\n<div>\nbar\n</div>\n",
        "<p>Foo</p>\n<div>\nbar\n</div>\n",
    ),
    (
        192,
        "[foo]: /url \"title\"\n\n[foo]\n",
        "<p><a href=\"/url\" title=\"title\">foo</a></p>\n",
    ),
    (
        194,
        "[Foo*bar\\]]:my_(url) 'title (with parens)'\n\n[Foo*bar\\]]\n",
        "<p><a href=\"my_(url)\" title=\"title (with parens)\">Foo*bar]</a></p>\n",
    ),
    (
        197,
        "[foo]: /url 'title\n\nwith blank line'\n\n[foo]\n",
        "<p>[foo]: /url 'title</p>\n<p>with blank line'</p>\n<p>[foo]</p>\n",
    ),
    (200, "[foo]: <>\n\n[foo]\n", "<p><a href=\"\">foo</a></p>\n"),
    (
        204,
        "[foo]\n\n[foo]: first\n[foo]: second\n",
        "<p><a href=\"first\">foo</a></p>\n",
    ),
    (207, "[foo]: /url\n", ""),
    (
        210,
        "[foo]: /url\n\"title\" ok\n",
        "<p>&quot;title&quot; ok</p>\n",
    ),
    (
        213,
        "Foo\n[bar]: /baz\n\n[bar]\n",
        "<p>Foo\n[bar]: /baz</p>\n<p>[bar]</p>\n",
    ),
    (
        216,
        "[foo]: /url\n===\n[foo]\n",
        "<p>===\n<a href=\"/url\">foo</a></p>\n",
    ),
    (219, "aaa\n\nbbb\n", "<p>aaa</p>\n<p>bbb</p>\n"),
    (221, "aaa\n\n\nbbb\n", "<p>aaa</p>\n<p>bbb</p>\n"),
    (
        223,
        "aaa\n             bbb\n                                       ccc\n",
        "<p>aaa\nbbb\nccc</p>\n",
    ),
    (
        225,
        "    aaa\nbbb\n",
        "<pre><code>aaa\n</code></pre>\n<p>bbb</p>\n",
    ),
    (
        227,
        "  \n\naaa\n  \n\n# aaa\n\n  \n",
        "<p>aaa</p>\n<h1>aaa</h1>\n",
    ),
    (
        228,
        "> # Foo\n> bar\n> baz\n",
        "<blockquote>\n<h1>Foo</h1>\n<p>bar\nbaz</p>\n</blockquote>\n",
    ),
    (
        230,
        "   > # Foo\n   > bar\n > baz\n",
        "<blockquote>\n<h1>Foo</h1>\n<p>bar\nbaz</p>\n</blockquote>\n",
    ),
    (
        232,
        "> # Foo\n> bar\nbaz\n",
        "<blockquote>\n<h1>Foo</h1>\n<p>bar\nbaz</p>\n</blockquote>\n",
    ),
    (
        234,
        "> foo\n---\n",
        "<blockquote>\n<p>foo</p>\n</blockquote>\n<hr />\n",
    ),
    (
        236,
        ">     foo\n    bar\n",
        "<blockquote>\n<pre><code>foo\n</code></pre>\n</blockquote>\n<pre><code>bar\n</code></pre>\n",
    ),
    (
        238,
        "> foo\n    - bar\n",
        "<blockquote>\n<p>foo\n- bar</p>\n</blockquote>\n",
    ),
    (240, ">\n>  \n> \n", "<blockquote>\n</blockquote>\n"),
    (
        242,
        "> foo\n\n> bar\n",
        "<blockquote>\n<p>foo</p>\n</blockquote>\n<blockquote>\n<p>bar</p>\n</blockquote>\n",
    ),
    (
        244,
        "> foo\n>\n> bar\n",
        "<blockquote>\n<p>foo</p>\n<p>bar</p>\n</blockquote>\n",
    ),
    (
        246,
        "> aaa\n***\n> bbb\n",
        "<blockquote>\n<p>aaa</p>\n</blockquote>\n<hr />\n<blockquote>\n<p>bbb</p>\n</blockquote>\n",
    ),
    (
        248,
        "> bar\n\nbaz\n",
        "<blockquote>\n<p>bar</p>\n</blockquote>\n<p>baz</p>\n",
    ),
    (
        250,
        "> > > foo\nbar\n",
        "<blockquote>\n<blockquote>\n<blockquote>\n<p>foo\nbar</p>\n</blockquote>\n</blockquote>\n</blockquote>\n",
    ),
    (
        253,
        "A paragraph\nwith two lines.\n\n    indented code\n\n> A block quote.\n",
        "<p>A paragraph\nwith two lines.</p>\n<pre><code>indented code\n</code></pre>\n<blockquote>\n<p>A block quote.</p>\n</blockquote>\n",
    ),
    (
        255,
        "- one\n\n two\n",
        "<ul>\n<li>one</li>\n</ul>\n<p>two</p>\n",
    ),
    (
        258,
        " -    one\n\n      two\n",
        "<ul>\n<li>\n<p>one</p>\n<p>two</p>\n</li>\n</ul>\n",
    ),
    (261, "-one\n\n2.two\n", "<p>-one</p>\n<p>2.two</p>\n"),
    (
        263,
        "1.  foo\n\n    ```\n    bar\n    ```\n\n    baz\n\n    > bam\n",
        "<ol>\n<li>\n<p>foo</p>\n<pre><code>bar\n</code></pre>\n<p>baz</p>\n<blockquote>\n<p>bam</p>\n</blockquote>\n</li>\n</ol>\n",
    ),
    (266, "1234567890. not ok\n", "<p>1234567890. not ok</p>\n"),
    (269, "-1. not ok\n", "<p>-1. not ok</p>\n"),
    (
        271,
        "  10.  foo\n\n           bar\n",
        "<ol start=\"10\">\n<li>\n<p>foo</p>\n<pre><code>bar\n</code></pre>\n</li>\n</ol>\n",
    ),
    (
        274,
        "1.      indented code\n\n   paragraph\n\n       more code\n",
        "<ol>\n<li>\n<pre><code> indented code\n</code></pre>\n<p>paragraph</p>\n<pre><code>more code\n</code></pre>\n</li>\n</ol>\n",
    ),
    (
        277,
        "-  foo\n\n   bar\n",
        "<ul>\n<li>\n<p>foo</p>\n<p>bar</p>\n</li>\n</ul>\n",
    ),
    (279, "-   \n  foo\n", "<ul>\n<li>foo</li>\n</ul>\n"),
    (
        282,
        "- foo\n-   \n- bar\n",
        "<ul>\n<li>foo</li>\n<li></li>\n<li>bar</li>\n</ul>\n",
    ),
    (
        285,
        "foo\n*\n\nfoo\n1.\n",
        "<p>foo\n*</p>\n<p>foo\n1.</p>\n",
    ),
    (
        287,
        "  1.  A paragraph\n      with two lines.\n\n          indented code\n\n      > A block quote.\n",
        "<ol>\n<li>\n<p>A paragraph\nwith two lines.</p>\n<pre><code>indented code\n</code></pre>\n<blockquote>\n<p>A block quote.</p>\n</blockquote>\n</li>\n</ol>\n",
    ),
    (
        290,
        "  1.  A paragraph\nwith two lines.\n\n          indented code\n\n      > A block quote.\n",
        "<ol>\n<li>\n<p>A paragraph\nwith two lines.</p>\n<pre><code>indented code\n</code></pre>\n<blockquote>\n<p>A block quote.</p>\n</blockquote>\n</li>\n</ol>\n",
    ),
    (
        293,
        "> 1. > Blockquote\n> continued here.\n",
        "<blockquote>\n<ol>\n<li>\n<blockquote>\n<p>Blockquote\ncontinued here.</p>\n</blockquote>\n</li>\n</ol>\n</blockquote>\n",
    ),
    (
        295,
        "- foo\n - bar\n  - baz\n   - boo\n",
        "<ul>\n<li>foo</li>\n<li>bar</li>\n<li>baz</li>\n<li>boo</li>\n</ul>\n",
    ),
    (
        298,
        "- - foo\n",
        "<ul>\n<li>\n<ul>\n<li>foo</li>\n</ul>\n</li>\n</ul>\n",
    ),
    (
        301,
        "- foo\n- bar\n+ baz\n",
        "<ul>\n<li>foo</li>\n<li>bar</li>\n</ul>\n<ul>\n<li>baz</li>\n</ul>\n",
    ),
    (
        302,
        "1. foo\n2. bar\n3) baz\n",
        "<ol>\n<li>foo</li>\n<li>bar</li>\n</ol>\n<ol start=\"3\">\n<li>baz</li>\n</ol>\n",
    ),
    (
        304,
        "The number of windows in my house is\n14.  The number of doors is 6.\n",
        "<p>The number of windows in my house is\n14.  The number of doors is 6.</p>\n",
    ),
    (
        306,
        "- foo\n\n- bar\n\n\n- baz\n",
        "<ul>\n<li>\n<p>foo</p>\n</li>\n<li>\n<p>bar</p>\n</li>\n<li>\n<p>baz</p>\n</li>\n</ul>\n",
    ),
    (
        308,
        "- foo\n- bar\n\n<!-- -->\n\n- baz\n- bim\n",
        "<ul>\n<li>foo</li>\n<li>bar</li>\n</ul>\n<!-- -->\n<ul>\n<li>baz</li>\n<li>bim</li>\n</ul>\n",
    ),
    (
        310,
        "- a\n - b\n  - c\n   - d\n  - e\n - f\n- g\n",
        "<ul>\n<li>a</li>\n<li>b</li>\n<li>c</li>\n<li>d</li>\n<li>e</li>\n<li>f</li>\n<li>g</li>\n</ul>\n",
    ),
    (
        312,
        "- a\n - b\n  - c\n   - d\n    - e\n",
        "<ul>\n<li>a</li>\n<li>b</li>\n<li>c</li>\n<li>d\n- e</li>\n</ul>\n",
    ),
    (
        314,
        "- a\n- b\n\n- c\n",
        "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n<li>\n<p>c</p>\n</li>\n</ul>\n",
    ),
    (
        315,
        "* a\n*\n\n* c\n",
        "<ul>\n<li>\n<p>a</p>\n</li>\n<li></li>\n<li>\n<p>c</p>\n</li>\n</ul>\n",
    ),
    (
        317,
        "- a\n- b\n\n  [ref]: /url\n- d\n",
        "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n</li>\n<li>\n<p>d</p>\n</li>\n</ul>\n",
    ),
    (
        319,
        "- a\n  - b\n\n    c\n- d\n",
        "<ul>\n<li>a\n<ul>\n<li>\n<p>b</p>\n<p>c</p>\n</li>\n</ul>\n</li>\n<li>d</li>\n</ul>\n",
    ),
    (
        321,
        "- a\n  > b\n  ```\n  c\n  ```\n- d\n",
        "<ul>\n<li>a\n<blockquote>\n<p>b</p>\n</blockquote>\n<pre><code>c\n</code></pre>\n</li>\n<li>d</li>\n</ul>\n",
    ),
    (
        323,
        "- a\n  - b\n",
        "<ul>\n<li>a\n<ul>\n<li>b</li>\n</ul>\n</li>\n</ul>\n",
    ),
    (
        325,
        "* foo\n  * bar\n\n  baz\n",
        "<ul>\n<li>\n<p>foo</p>\n<ul>\n<li>bar</li>\n</ul>\n<p>baz</p>\n</li>\n</ul>\n",
    ),
    (327, "`hi`lo`\n", "<p><code>hi</code>lo`</p>\n"),
    (328, "`foo`\n", "<p><code>foo</code></p>\n"),
    (330, "` `` `\n", "<p><code>``</code></p>\n"),
    (332, "` a`\n", "<p><code> a</code></p>\n"),
    (
        334,
        "`\u{a0}`\n`  `\n",
        "<p><code>\u{a0}</code>\n<code>  </code></p>\n",
    ),
    (336, "``\nfoo \n``\n", "<p><code>foo </code></p>\n"),
    (338, "`foo\\`bar`\n", "<p><code>foo\\</code>bar`</p>\n"),
    (340, "` foo `` bar `\n", "<p><code>foo `` bar</code></p>\n"),
    (
        342,
        "[not a `link](/foo`)\n",
        "<p>[not a <code>link](/foo</code>)</p>\n",
    ),
    (
        345,
        "`<https://foo.bar.`baz>`\n",
        "<p><code>&lt;https://foo.bar.</code>baz&gt;`</p>\n",
    ),
    (347, "```foo``\n", "<p>```foo``</p>\n"),
    (350, "*foo bar*\n", "<p><em>foo bar</em></p>\n"),
    (353, "*\u{a0}a\u{a0}*\n", "<p>*\u{a0}a\u{a0}*</p>\n"),
    (357, "_foo bar_\n", "<p><em>foo bar</em></p>\n"),
    (360, "foo_bar_\n", "<p>foo_bar_</p>\n"),
    (364, "foo-_(bar)_\n", "<p>foo-<em>(bar)</em></p>\n"),
    (367, "*foo bar\n*\n", "<p>*foo bar\n*</p>\n"),
    (371, "_foo bar _\n", "<p>_foo bar _</p>\n"),
    (
        375,
        "_пристаням_стремятся\n",
        "<p>_пристаням_стремятся</p>\n",
    ),
    (378, "**foo bar**\n", "<p><strong>foo bar</strong></p>\n"),
    (382, "__foo bar__\n", "<p><strong>foo bar</strong></p>\n"),
    (385, "a__\"foo\"__\n", "<p>a__&quot;foo&quot;__</p>\n"),
    (
        389,
        "__foo, __bar__, baz__\n",
        "<p><strong>foo, <strong>bar</strong>, baz</strong></p>\n",
    ),
    (
        393,
        "*(**foo**)*\n",
        "<p><em>(<strong>foo</strong>)</em></p>\n",
    ),
    (396, "**foo**bar\n", "<p><strong>foo</strong>bar</p>\n"),
    (400, "__foo__bar\n", "<p>__foo__bar</p>\n"),
    (403, "__(bar)__.\n", "<p><strong>(bar)</strong>.</p>\n"),
    (
        407,
        "_foo _bar_ baz_\n",
        "<p><em>foo <em>bar</em> baz</em></p>\n",
    ),
    (
        410,
        "*foo **bar** baz*\n",
        "<p><em>foo <strong>bar</strong> baz</em></p>\n",
    ),
    (
        411,
        "*foo**bar**baz*\n",
        "<p><em>foo<strong>bar</strong>baz</em></p>\n",
    ),
    (412, "*foo**bar*\n", "<p><em>foo**bar</em></p>\n"),
    (
        414,
        "*foo **bar***\n",
        "<p><em>foo <strong>bar</strong></em></p>\n",
    ),
    (
        415,
        "*foo**bar***\n",
        "<p><em>foo<strong>bar</strong></em></p>\n",
    ),
    (
        418,
        "*foo **bar *baz* bim** bop*\n",
        "<p><em>foo <strong>bar <em>baz</em> bim</strong> bop</em></p>\n",
    ),
    (
        421,
        "**** is not an empty strong emphasis\n",
        "<p>**** is not an empty strong emphasis</p>\n",
    ),
    (
        425,
        "__foo __bar__ baz__\n",
        "<p><strong>foo <strong>bar</strong> baz</strong></p>\n",
    ),
    (
        428,
        "**foo *bar* baz**\n",
        "<p><strong>foo <em>bar</em> baz</strong></p>\n",
    ),
    (
        429,
        "**foo*bar*baz**\n",
        "<p><strong>foo<em>bar</em>baz</strong></p>\n",
    ),
    (
        432,
        "**foo *bar **baz**\nbim* bop**\n",
        "<p><strong>foo <em>bar <strong>baz</strong>\nbim</em> bop</strong></p>\n",
    ),
    (436, "foo ***\n", "<p>foo ***</p>\n"),
    (439, "foo *****\n", "<p>foo *****</p>\n"),
    (443, "*foo**\n", "<p><em>foo</em>*</p>\n"),
    (446, "**foo***\n", "<p><strong>foo</strong>*</p>\n"),
    (450, "foo _*_\n", "<p>foo <em>*</em></p>\n"),
    (453, "foo __*__\n", "<p>foo <strong>*</strong></p>\n"),
    (457, "____foo_\n", "<p>___<em>foo</em></p>\n"),
    (461, "*_foo_*\n", "<p><em><em>foo</em></em></p>\n"),
    (
        464,
        "****foo****\n",
        "<p><strong><strong>foo</strong></strong></p>\n",
    ),
    (
        468,
        "_____foo_____\n",
        "<p><em><strong><strong>foo</strong></strong></em></p>\n",
    ),
    (
        471,
        "**foo **bar baz**\n",
        "<p>**foo <strong>bar baz</strong></p>\n",
    ),
    (478, "*a `*`*\n", "<p><em>a <code>*</code></em></p>\n"),
    (
        482,
        "[link](/uri \"title\")\n",
        "<p><a href=\"/uri\" title=\"title\">link</a></p>\n",
    ),
    (485, "[link]()\n", "<p><a href=\"\">link</a></p>\n"),
    (
        489,
        "[link](</my uri>)\n",
        "<p><a href=\"/my%20uri\">link</a></p>\n",
    ),
    (
        495,
        "[link](\\(foo\\))\n",
        "<p><a href=\"(foo)\">link</a></p>\n",
    ),
    (
        499,
        "[link](<foo(and(bar)>)\n",
        "<p><a href=\"foo(and(bar)\">link</a></p>\n",
    ),
    (
        503,
        "[link](foo%20b&auml;)\n",
        "<p><a href=\"foo%20b%C3%A4\">link</a></p>\n",
    ),
    (
        507,
        "[link](/url\u{a0}\"title\")\n",
        "<p><a href=\"/url%C2%A0%22title%22\">link</a></p>\n",
    ),
    (511, "[link] (/uri)\n", "<p>[link] (/uri)</p>\n"),
    (
        515,
        "[link \\[bar](/uri)\n",
        "<p><a href=\"/uri\">link [bar</a></p>\n",
    ),
    (
        519,
        "[foo *[bar [baz](/uri)](/uri)*](/uri)\n",
        "<p>[foo <em>[bar <a href=\"/uri\">baz</a>](/uri)</em>](/uri)</p>\n",
    ),
    (523, "*foo [bar* baz]\n", "<p><em>foo [bar</em> baz]</p>\n"),
    (
        528,
        "[link [foo [bar]]][ref]\n\n[ref]: /uri\n",
        "<p><a href=\"/uri\">link [foo [bar]]</a></p>\n",
    ),
    (
        531,
        "[![moon](moon.jpg)][ref]\n\n[ref]: /uri\n",
        "<p><a href=\"/uri\"><img src=\"moon.jpg\" alt=\"moon\" /></a></p>\n",
    ),
    (
        535,
        "[foo *bar][ref]*\n\n[ref]: /uri\n",
        "<p><a href=\"/uri\">foo *bar</a>*</p>\n",
    ),
    (
        540,
        "[ẞ]\n\n[SS]: /url\n",
        "<p><a href=\"/url\">ẞ</a></p>\n",
    ),
    (
        544,
        "[foo]: /url1\n\n[foo]: /url2\n\n[bar][foo]\n",
        "<p><a href=\"/url1\">bar</a></p>\n",
    ),
    (
        548,
        "[[[foo]]]\n\n[[[foo]]]: /url\n",
        "<p>[[[foo]]]</p>\n<p>[[[foo]]]: /url</p>\n",
    ),
    (
        552,
        "[\n ]\n\n[\n ]: /uri\n",
        "<p>[\n]</p>\n<p>[\n]: /uri</p>\n",
    ),
    (
        556,
        "[foo] \n[]\n\n[foo]: /url \"title\"\n",
        "<p><a href=\"/url\" title=\"title\">foo</a>\n[]</p>\n",
    ),
    (
        560,
        "[[bar [foo]\n\n[foo]: /url\n",
        "<p>[[bar <a href=\"/url\">foo</a></p>\n",
    ),
    (
        564,
        "[foo*]: /url\n\n*[foo*]\n",
        "<p>*<a href=\"/url\">foo*</a></p>\n",
    ),
    (
        568,
        "[foo](not a link)\n\n[foo]: /url1\n",
        "<p><a href=\"/url1\">foo</a>(not a link)</p>\n",
    ),
    (
        572,
        "![foo](/url \"title\")\n",
        "<p><img src=\"/url\" alt=\"foo\" title=\"title\" /></p>\n",
    ),
    (
        575,
        "![foo [bar](/url)](/url2)\n",
        "<p><img src=\"/url2\" alt=\"foo bar\" /></p>\n",
    ),
    (
        578,
        "![foo](train.jpg)\n",
        "<p><img src=\"train.jpg\" alt=\"foo\" /></p>\n",
    ),
    (581, "![](/url)\n", "<p><img src=\"/url\" alt=\"\" /></p>\n"),
    (
        584,
        "![foo][]\n\n[foo]: /url \"title\"\n",
        "<p><img src=\"/url\" alt=\"foo\" title=\"title\" /></p>\n",
    ),
    (
        587,
        "![foo] \n[]\n\n[foo]: /url \"title\"\n",
        "<p><img src=\"/url\" alt=\"foo\" title=\"title\" />\n[]</p>\n",
    ),
    (
        590,
        "![[foo]]\n\n[[foo]]: /url \"title\"\n",
        "<p>![[foo]]</p>\n<p>[[foo]]: /url &quot;title&quot;</p>\n",
    ),
    (
        594,
        "<http://foo.bar.baz>\n",
        "<p><a href=\"http://foo.bar.baz\">http://foo.bar.baz</a></p>\n",
    ),
    (
        596,
        "<irc://foo.bar:2233/baz>\n",
        "<p><a href=\"irc://foo.bar:2233/baz\">irc://foo.bar:2233/baz</a></p>\n",
    ),
    (
        599,
        "<made-up-scheme://foo,bar>\n",
        "<p><a href=\"made-up-scheme://foo,bar\">made-up-scheme://foo,bar</a></p>\n",
    ),
    (
        602,
        "<https://foo.bar/baz bim>\n",
        "<p>&lt;https://foo.bar/baz bim&gt;</p>\n",
    ),
    (
        604,
        "<foo@bar.example.com>\n",
        "<p><a href=\"mailto:foo@bar.example.com\">foo@bar.example.com</a></p>\n",
    ),
    (607, "<>\n", "<p>&lt;&gt;</p>\n"),
    (610, "<foo.bar.baz>\n", "<p>&lt;foo.bar.baz&gt;</p>\n"),
    (633, "foo  \nbaz\n", "<p>foo<br />\nbaz</p>\n"),
    (635, "foo       \nbaz\n", "<p>foo<br />\nbaz</p>\n"),
    (638, "*foo  \nbar*\n", "<p><em>foo<br />\nbar</em></p>\n"),
    (640, "`code  \nspan`\n", "<p><code>code   span</code></p>\n"),
    (645, "foo  \n", "<p>foo</p>\n"),
    (648, "foo\nbaz\n", "<p>foo\nbaz</p>\n"),
    (649, "foo \n baz\n", "<p>foo\nbaz</p>\n"),
    (650, "hello $.;'there\n", "<p>hello $.;'there</p>\n"),
    (651, "Foo χρῆν\n", "<p>Foo χρῆν</p>\n"),
    // SPEC-EXAMPLES-END
];

fn render(md: &str) -> String {
    to_html(&parse(md).0)
}
fn spec_examples() {
    let failed: Vec<u16> = SPEC
        .iter()
        .filter(|(_, md, html)| render(md) != *html)
        .map(|e| e.0)
        .collect();
    assert!(
        failed.is_empty(),
        "spec examples now failing: {failed:?}; first: {:?}",
        SPEC.iter()
            .find(|e| e.0 == failed[0])
            .map(|e| (e.1, e.2, render(e.1)))
    );
    assert!(SPEC.len() >= 120);
    println!("  {} spec 0.31.2 examples render exactly", SPEC.len());
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as usize % n.max(1)
    }
    fn pick<'a>(&mut self, v: &[&'a str]) -> &'a str {
        v[self.below(v.len())]
    }
}
/// A random line of inline Markdown: well-formed constructs mixed with stray punctuation.
fn gen_inline(r: &mut Rng, depth: usize) -> String {
    let mut parts = vec![];
    for _ in 0..1 + r.below(5) {
        let p = match if depth > 2 { 0 } else { r.below(14) } {
            1 => format!("*{}*", gen_inline(r, depth + 1)),
            2 => format!("**{}**", gen_inline(r, depth + 1)),
            3 => format!("_{}_", gen_inline(r, depth + 1)),
            4 => format!("__{}__", gen_inline(r, depth + 1)),
            5 => format!(
                "`{}`",
                r.pick(&["x", "a*b", "``", " y ", "<z>", "[l](u)", "\\"])
            ),
            6 => format!(
                "[{}]({}{})",
                gen_inline(r, depth + 1),
                r.pick(&["/u", "a%20b", "<a b>", "", "/x(1)", "u?a=1&b=2"]),
                r.pick(&["", " \"t\"", " 'q \"r\"'"])
            ),
            7 => format!("![{}](/i.png)", gen_inline(r, depth + 1)),
            8 => r
                .pick(&[
                    "[ref]",
                    "[Ref][]",
                    "[text][ref]",
                    "<http://e.com/a_b*c>",
                    "<me@x.org>",
                ])
                .to_string(),
            9 => r
                .pick(&[
                    "\\*", "&amp;", "&copy;", "&#42;", "&nope;", "a_b_c", "2*3*4", "<br>", "x<y",
                    "R&D", "100%",
                ])
                .to_string(),
            10 => r
                .pick(&[
                    "*", "**", "_", "[", "]", "(", ")", "!", "#", "+", "-", ">", "=", "~", "`",
                    "\\", "1.", "2)", "|", "\"",
                ])
                .to_string(),
            _ => r
                .pick(&[
                    "word",
                    "two words",
                    "x",
                    "é",
                    "end.",
                    "(paren)",
                    "\"quoted\"",
                    "a, b",
                ])
                .to_string(),
        };
        parts.push(p);
    }
    let sep = r.pick(&[" ", " ", " ", ""]);
    parts.join(sep)
}
/// A random block document, built from lines; nesting comes from prefixing whole sub-documents.
fn gen_doc(r: &mut Rng, depth: usize) -> String {
    let mut blocks = vec![];
    for _ in 0..1 + r.below(if depth == 0 { 6 } else { 3 }) {
        let b = match r.below(if depth < 3 { 12 } else { 7 }) {
            0 => format!("{} {}", "#".repeat(1 + r.below(6)), gen_inline(r, 0)),
            1 => format!("{}\n{}", gen_inline(r, 0), r.pick(&["===", "---"])),
            2 => r.pick(&["***", "- - -", "___"]).to_string(),
            3 => format!(
                "{}{}\n{}\n{}",
                r.pick(&["```", "~~~~"]),
                r.pick(&["", "rust", " a b"]),
                r.pick(&["let x = 1;", "  indented\n\n``` not a fence", "<tag>"]),
                "~~~~\n```"
            ),
            4 => format!(
                "    {}\n    {}",
                gen_inline(r, 3),
                r.pick(&["code", "- not a list"])
            ),
            5 => r
                .pick(&[
                    "<div>\n*raw*\n</div>",
                    "<!-- note -->",
                    "[ref]: /url \"Title\"",
                    "[ref]: <a b>",
                ])
                .to_string(),
            6 => (0..1 + r.below(3))
                .map(|_| gen_inline(r, 0))
                .collect::<Vec<_>>()
                .join(r.pick(&["\n", "\\\n", "  \n"])),
            7 | 8 => prefix_lines(&gen_doc(r, depth + 1), "> ", "> "),
            _ => {
                let marker = r.pick(&["-", "*", "+", "1.", "7)"]);
                let items: Vec<String> = (0..1 + r.below(3))
                    .map(|_| {
                        let body = format!(
                            "{}{}",
                            r.pick(&["", "", "", "[ ] ", "[x] "]),
                            gen_doc(r, depth + 1)
                        );
                        let first = format!("{marker} ");
                        prefix_lines(&body, &first, &" ".repeat(first.len()))
                    })
                    .collect();
                items.join(r.pick(&["\n", "\n\n"]))
            }
        };
        blocks.push(b);
    }
    blocks.join(r.pick(&["\n\n", "\n\n", "\n"]))
}
/// parse(print(parse(x))) == parse(x), the flat form is lossless, and printing is a fixed point.
fn round_trip(src: &str) -> Result<(), String> {
    let doc = parse(src).0;
    let printed = to_markdown(&doc);
    let again = parse(&printed).0;
    if again != doc {
        return Err(format!(
            "source  {src:?}\nprinted {printed:?}\nfirst  {:?}\nsecond {:?}",
            to_html(&doc),
            to_html(&again)
        ));
    }
    if to_markdown(&again) != printed {
        return Err(format!("printer is not a fixed point for {src:?}"));
    }
    if from_flat(&to_flat(&doc)) != doc {
        return Err(format!("flat block form lost information for {src:?}"));
    }
    Ok(())
}
fn round_trip_property() {
    let (mut blocks, mut rows, mut bad) = (0, 0, vec![]);
    for seed in 0..2000u64 {
        let src = gen_doc(&mut Rng::new(seed + 1), 0);
        let doc = parse(&src).0;
        blocks += doc.len();
        rows += to_flat(&doc).len();
        if let Err(e) = round_trip(&src) {
            bad.push((src.len(), seed, e));
        }
    }
    bad.sort();
    assert!(
        bad.is_empty(),
        "{} of 2000 documents failed; smallest (seed {}):\n{}",
        bad.len(),
        bad[0].1,
        bad[0].2
    );
    println!(
        "  parse(print(parse(x))) == parse(x), fixed-point printing and lossless flat form on 2000 random documents ({blocks} top blocks, {rows} flat rows)"
    );
}
fn fuzz_never_panics() {
    let alphabet: &[u8] = b"  \t\n\r*_`[]()<>!#-+=~\\&;:\"'./|0123456789abxX\0\xc3\xa9\xff";
    let mut r = Rng::new(0xF00D);
    let mut bytes_total = 0;
    for case in 0..20_000 {
        let len = r.below(160);
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                if case % 4 == 0 {
                    r.below(256) as u8
                } else {
                    alphabet[r.below(alphabet.len())]
                }
            })
            .collect();
        let src = String::from_utf8_lossy(&bytes);
        bytes_total += bytes.len();
        let (doc, _) = parse(&src);
        let _ = (to_html(&doc), to_markdown(&doc));
        assert_eq!(
            from_flat(&to_flat(&doc)),
            doc,
            "flat form, fuzz case {case}: {src:?}"
        );
        assert!(
            doc.iter()
                .all(|b| b.span.start <= b.span.end && b.span.end <= src.len()),
            "bad span, fuzz case {case}: {src:?}"
        );
    }
    println!(
        "  20000 random byte strings ({bytes_total} bytes): no panic, spans in range, flat form lossless"
    );
}
fn steps_for(src: &str) -> u64 {
    STEPS.with(|s| s.set(0));
    let doc = parse(src).0;
    let _ = (to_html(&doc), to_markdown(&doc));
    STEPS.with(Cell::get)
}
fn pathological_inputs() {
    type Gen = fn(usize) -> String;
    let cases: [(&str, Gen); 14] = [
        ("nested quotes on one line", |n| ">".repeat(n) + " a"),
        ("many lines in a deep quote", |n| {
            (">".repeat(60) + " a\n").repeat(n / 60 + 1)
        }),
        ("nested list markers", |n| "- ".repeat(n / 2) + "a"),
        ("nested emphasis", |n| {
            "*a **a ".repeat(n / 14) + "b" + &" a** a*".repeat(n / 14)
        }),
        ("long runs of *", |n| {
            "*".repeat(n / 2) + "a" + &"*".repeat(n / 2)
        }),
        ("openers with no closer", |n| {
            "a_ ".repeat(n / 3) + &"*a ".repeat(n / 3)
        }),
        ("closers with no opener", |n| "a** ".repeat(n / 4)),
        ("mixed delimiters", |n| "*a_ ".repeat(n / 4)),
        ("open brackets", |n| "[".repeat(n)),
        ("nested brackets", |n| {
            "[".repeat(n / 2) + &"]".repeat(n / 2)
        }),
        ("many links after many [", |n| {
            "[".repeat(n / 2) + &"[a](u)".repeat(n / 12)
        }),
        ("unclosed destinations", |n| "[a](".repeat(n / 4)),
        ("backtick runs of every length", |n| {
            (1..)
                .map(|k| "`".repeat(k) + "a")
                .scan(0, |t, s| {
                    *t += s.len();
                    (*t <= n).then_some(s)
                })
                .collect()
        }),
        ("many reference definitions", |n| "[a]: /u\n".repeat(n / 8)),
    ];
    let mut worst: (f64, &str) = (0.0, "");
    for (name, make) in cases {
        let (small, big) = (make(8_000), make(32_000));
        let (a, b) = (steps_for(&small), steps_for(&big));
        // growth of steps per input byte: about 1 when linear, about 4 when quadratic (the input is 4 times bigger)
        let growth = (b as f64 / big.len() as f64) / (a as f64 / small.len() as f64);
        assert!(
            growth < 1.6,
            "{name}: steps grew super-linearly: {a} for {} bytes, {b} for {} bytes",
            small.len(),
            big.len()
        );
        assert!(
            b < 200 * big.len() as u64,
            "{name}: {b} steps for {} bytes",
            big.len()
        );
        if growth > worst.0 {
            worst = (growth, name);
        }
    }
    println!(
        "  {} pathological families at 8k and 32k bytes: steps per byte grow at most {:.2}x ({})",
        cases.len(),
        worst.0,
        worst.1
    );
}

fn demo() {
    let src = "# Release *notes*\n\n> **Heads up:** read the [guide][g].\n\n- [x] parser\n- [ ] printer with `code`\n\n  1. nested\n\n```rust\nfn main() {}\n```\n\n[g]: /guide \"The guide\"\n";
    let doc = parse(src).0;
    println!("--- flat block list (id, depth, type, text with marks):");
    for row in to_flat(&doc) {
        let text: String = row
            .text
            .iter()
            .map(|r| {
                let marks: Vec<&str> = r
                    .marks
                    .iter()
                    .map(|m| match m.1 {
                        Mark::Emph(_) => "i",
                        Mark::Strong(_) => "b",
                        Mark::Link { .. } => "link",
                        Mark::Image { .. } => "img",
                    })
                    .collect();
                match &r.piece {
                    Piece::Text(t) => format!(
                        "{t:?}{}",
                        if marks.is_empty() {
                            String::new()
                        } else {
                            format!("{{{}}}", marks.join(","))
                        }
                    ),
                    other => format!("{other:?}"),
                }
            })
            .collect::<Vec<_>>()
            .join(" + ");
        let kind = match &row.kind {
            RowKind::Item { list, task, .. } => format!(
                "{}{}",
                if list.ordered { "numbered" } else { "bullet" },
                task.map_or("", |t| if t { " [x]" } else { " [ ]" })
            ),
            k => format!("{k:?}"),
        };
        println!("  {:>2} {}{kind}: {text}", row.id, "  ".repeat(row.depth));
    }
    println!(
        "--- normalised Markdown:\n{}--- HTML:\n{}",
        to_markdown(&doc),
        to_html(&doc)
    );
    assert_eq!(doc[0].span, Span { start: 0, end: 17 });
    assert_eq!(
        &src[doc[3].span.start..doc[3].span.end],
        "```rust\nfn main() {}\n```"
    );
    round_trip(src).unwrap();
}

fn main() {
    println!("Spec examples");
    spec_examples();
    println!("Round trip");
    round_trip_property();
    println!("Fuzz");
    fuzz_never_panics();
    println!("No quadratic blowup");
    pathological_inputs();
    demo();
    println!("OK");
}
