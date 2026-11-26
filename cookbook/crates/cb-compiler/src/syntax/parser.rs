//! A hand-written, error-recovering parser: recursive descent for items,
//! statements, types and patterns, and Pratt (binding-power) parsing for expressions.
//!
//! Recovery rules (the contract the tests rely on):
//! * It never panics and always terminates: every loop either consumes a token
//!   or exits, nesting is capped (`MAX_DEPTH`), and a global fuel counter turns
//!   any bug into "end of file" rather than a hang.
//! * It always returns a complete tree: missing pieces become `Error` nodes
//!   whose empty span sits where the piece should have been.
//! * One mistake yields one diagnostic: errors at the same token are dropped, and
//!   errors right after a lexer error are suppressed (the lexer already spoke).
//! * Synchronisation points: `;`, `}`, item keywords, and closing delimiters.
//!
//! Further reading: <https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html>.

use super::ast::*;
use super::lexer::lex;
use super::token::{Comment, Token, TokenKind};
use crate::diag::{Applicability, Diagnostic, Edit, Suggestion};
use crate::source::Span;
use std::cell::Cell;

const MAX_DEPTH: u32 = 100;
const MAX_CHAIN: u32 = 300;
static EOF: TokenKind = TokenKind::Eof;

pub struct ParseResult {
    pub ast: Module,
    pub comments: Vec<Comment>,
    /// Lexer and parser diagnostics, in source order.
    pub diagnostics: Vec<Diagnostic>,
    /// True if the safety fuel ran out (indicates a parser bug; never expected).
    pub fuel_exhausted: bool,
}

/// Parse a whole file. Always succeeds in producing a [`Module`].
pub fn parse(src: &str) -> ParseResult {
    let (all, comments, mut diags) = lex(src);
    let mut toks: Vec<Token> = Vec::with_capacity(all.len());
    let mut gap: Vec<bool> = Vec::with_capacity(all.len());
    let mut pending = false;
    for t in all {
        if t.kind == TokenKind::Error {
            pending = true;
        } else {
            gap.push(pending);
            pending = false;
            toks.push(t);
        }
    }
    let fuel = toks.len() * 50 + 2000;
    let mut p = Parser {
        src,
        toks,
        gap,
        pos: 0,
        prev_end: 0,
        diags: Vec::new(),
        next_id: 0,
        depth: 0,
        no_struct: false,
        bailed: false,
        last_err: None,
        fuel: Cell::new(fuel),
        exhausted: Cell::new(false),
    };
    let ast = p.parse_module();
    diags.append(&mut p.diags);
    crate::diag::sort_and_dedup(&mut diags);
    ParseResult {
        ast,
        comments,
        diagnostics: diags,
        fuel_exhausted: p.exhausted.get(),
    }
}

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Token>,
    /// `gap[i]`: a lexer error token was dropped just before token `i`.
    gap: Vec<bool>,
    pos: usize,
    prev_end: u32,
    diags: Vec<Diagnostic>,
    next_id: u32,
    depth: u32,
    no_struct: bool,
    bailed: bool,
    last_err: Option<usize>,
    fuel: Cell<usize>,
    exhausted: Cell<bool>,
}

fn is_cmp(k: &TokenKind) -> bool {
    matches!(
        k,
        TokenKind::EqEq
            | TokenKind::NotEq
            | TokenKind::Lt
            | TokenKind::Le
            | TokenKind::Gt
            | TokenKind::Ge
    )
}

/// (op, left binding power, right binding power, assignment?) for infix tokens.
fn infix(k: &TokenKind) -> Option<(Option<BinOp>, u8, u8, bool)> {
    use TokenKind::*;
    Some(match k {
        Eq => (None, 1, 1, true),
        PlusEq => (Some(BinOp::Add), 1, 1, true),
        MinusEq => (Some(BinOp::Sub), 1, 1, true),
        StarEq => (Some(BinOp::Mul), 1, 1, true),
        SlashEq => (Some(BinOp::Div), 1, 1, true),
        PercentEq => (Some(BinOp::Rem), 1, 1, true),
        OrOr => (Some(BinOp::Or), 2, 3, false),
        AndAnd => (Some(BinOp::And), 4, 5, false),
        EqEq => (Some(BinOp::Eq), 6, 7, false),
        NotEq => (Some(BinOp::Ne), 6, 7, false),
        Lt => (Some(BinOp::Lt), 6, 7, false),
        Le => (Some(BinOp::Le), 6, 7, false),
        Gt => (Some(BinOp::Gt), 6, 7, false),
        Ge => (Some(BinOp::Ge), 6, 7, false),
        Plus => (Some(BinOp::Add), 8, 9, false),
        Minus => (Some(BinOp::Sub), 8, 9, false),
        Star => (Some(BinOp::Mul), 10, 11, false),
        Slash => (Some(BinOp::Div), 10, 11, false),
        Percent => (Some(BinOp::Rem), 10, 11, false),
        _ => return None,
    })
}
const AS_BP: u8 = 12;

enum Parsed {
    Stmt(Stmt),
    Tail(Expr),
}

enum PField {
    Field(FieldPat),
    Rest,
}

impl Parser<'_> {
    // ---------------------------------------------------------------- cursor

    fn kind(&self) -> &TokenKind {
        let f = self.fuel.get();
        if f == 0 {
            self.exhausted.set(true);
            return &EOF;
        }
        self.fuel.set(f - 1);
        &self.toks[self.pos].kind
    }
    fn nth(&self, n: usize) -> &TokenKind {
        self.toks.get(self.pos + n).map(|t| &t.kind).unwrap_or(&EOF)
    }
    fn at(&self, k: &TokenKind) -> bool {
        self.kind() == k
    }
    fn span(&self) -> Span {
        self.toks[self.pos].span
    }
    fn start(&self) -> u32 {
        self.span().start
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if t.kind != TokenKind::Eof {
            self.pos += 1;
            self.prev_end = t.span.end;
        }
        t
    }
    fn eat(&mut self, k: &TokenKind) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn id(&mut self) -> NodeId {
        self.next_id += 1;
        NodeId(self.next_id - 1)
    }
    fn span_from(&self, start: u32) -> Span {
        Span::new(start as usize, self.prev_end.max(start) as usize)
    }
    /// An empty span at the cursor, used for nodes that are missing from the source.
    /// Moving `prev_end` up keeps enclosing nodes' spans covering the placeholder.
    fn err_span(&mut self) -> Span {
        let p = self.start();
        self.prev_end = self.prev_end.max(p);
        Span::point(p as usize)
    }
    fn text(&self, span: Span) -> &str {
        span.slice(self.src)
    }
    fn found(&self) -> String {
        let t = &self.toks[self.pos];
        match &t.kind {
            TokenKind::Ident | TokenKind::Int(_) => format!("`{}`", self.text(t.span)),
            k => k.describe(),
        }
    }

    // ---------------------------------------------------------------- errors

    /// Should an error at the current token be dropped (cascade control)?
    fn suppressed(&mut self) -> bool {
        if self.bailed || self.gap[self.pos] || self.last_err == Some(self.pos) {
            return true;
        }
        self.last_err = Some(self.pos);
        false
    }
    fn unexpected(&mut self, code: &'static str, what: &str) {
        if self.suppressed() {
            return;
        }
        let msg = format!("expected {what}, found {}", self.found());
        let span = self.span();
        self.diags
            .push(Diagnostic::error(code, msg).primary(span, format!("expected {what}")));
    }
    fn expect(&mut self, k: TokenKind) -> bool {
        if self.at(&k) {
            self.bump();
            return true;
        }
        self.unexpected("E0101", &k.describe());
        false
    }
    /// Like `expect(Semi)` but blames the end of the previous token and offers to insert it.
    fn expect_semi(&mut self) {
        if self.eat(&TokenKind::Semi) {
            return;
        }
        if self.suppressed() {
            return;
        }
        let at = Span::point(self.prev_end as usize);
        let msg = format!("expected `;`, found {}", self.found());
        let mut d = Diagnostic::error("E0101", msg).primary(at, "expected `;` here");
        d.suggestions.push(Suggestion {
            message: "add a semicolon".into(),
            edits: vec![Edit {
                span: at,
                replacement: ";".into(),
            }],
            applicability: Applicability::MachineApplicable,
        });
        self.diags.push(d);
    }
    fn expect_ident(&mut self) -> Ident {
        if *self.kind() == TokenKind::Ident {
            let t = self.bump();
            let id = self.id();
            return Ident {
                id,
                span: t.span,
                name: self.text(t.span).to_string(),
            };
        }
        self.unexpected("E0101", "identifier");
        let span = self.err_span();
        let id = self.id();
        Ident {
            id,
            span,
            name: String::new(),
        }
    }
    fn unclosed(&mut self, open: Span, close: &TokenKind) {
        if self.suppressed() {
            return;
        }
        let o = match close {
            TokenKind::RParen => "(",
            TokenKind::RBracket => "[",
            TokenKind::Gt => "<",
            _ => "{",
        };
        let here = self.span();
        let mut d = Diagnostic::error("E0106", format!("unclosed delimiter `{o}`"))
            .primary(open, "unclosed delimiter");
        d.labels.push(crate::diag::Label {
            span: here,
            text: format!("expected {} before this", close.describe()),
            primary: false,
        });
        self.diags.push(d);
    }
    /// Give up on the rest of the file (nesting too deep): jump to EOF.
    fn bail(&mut self, msg: &str) {
        if !self.bailed {
            let span = self.span();
            self.diags.push(
                Diagnostic::error("E0113", msg.to_string())
                    .primary(span, "gave up here")
                    .note("the rest of the file is ignored"),
            );
        }
        self.bailed = true;
        self.pos = self.toks.len() - 1;
    }
    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            self.bail("this code is nested too deeply");
            return false;
        }
        self.depth += 1;
        true
    }

    // ---------------------------------------------------------------- items

    fn is_item_start(&self) -> bool {
        match self.nth(0) {
            TokenKind::Fn => *self.nth(1) == TokenKind::Ident,
            TokenKind::Struct | TokenKind::Enum | TokenKind::Const => true,
            _ => false,
        }
    }

    fn parse_module(&mut self) -> Module {
        let mut items = Vec::new();
        while !self.at(&TokenKind::Eof) {
            let before = self.pos;
            items.push(self.parse_item());
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.src.len();
        Module {
            items,
            span: Span::new(0, end),
            node_count: self.next_id,
        }
    }

    fn parse_item(&mut self) -> Item {
        let start = self.start();
        let kind = match self.kind() {
            TokenKind::Fn => ItemKind::Fn(self.parse_fn()),
            TokenKind::Struct => ItemKind::Struct(self.parse_struct()),
            TokenKind::Enum => ItemKind::Enum(self.parse_enum()),
            TokenKind::Const => ItemKind::Const(self.parse_const()),
            _ => {
                self.item_error();
                ItemKind::Error
            }
        };
        let span = self.span_from(start);
        Item {
            id: self.id(),
            span,
            kind,
        }
    }

    fn item_error(&mut self) {
        match self.kind() {
            TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                if !self.suppressed() {
                    let span = self.span();
                    self.diags.push(
                        Diagnostic::error(
                            "E0112",
                            format!("unexpected closing delimiter {}", self.found()),
                        )
                        .primary(span, "no matching opening delimiter"),
                    );
                }
            }
            _ => self.unexpected("E0105", "an item (`fn`, `struct`, `enum` or `const`)"),
        }
        self.bump();
        while !self.at(&TokenKind::Eof) && !self.is_item_start() {
            self.bump();
        }
    }

    fn parse_generics(&mut self) -> Vec<Ident> {
        if !self.at(&TokenKind::Lt) {
            return vec![];
        }
        let open = self.bump().span;
        self.comma_list(
            open,
            TokenKind::Gt,
            &[TokenKind::LBrace, TokenKind::LParen, TokenKind::Eq],
            |p| p.expect_ident(),
        )
    }

    fn parse_fn(&mut self) -> FnDecl {
        self.bump();
        let name = self.expect_ident();
        let generics = self.parse_generics();
        let params = if self.at(&TokenKind::LParen) {
            let open = self.bump().span;
            self.comma_list(
                open,
                TokenKind::RParen,
                &[TokenKind::LBrace, TokenKind::Arrow],
                |p| p.parse_param(),
            )
        } else {
            self.unexpected("E0101", "`(`");
            vec![]
        };
        let ret = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block();
        FnDecl {
            name,
            generics,
            params,
            ret,
            body,
        }
    }

    fn parse_param(&mut self) -> Param {
        let start = self.start();
        let mutable = self.eat(&TokenKind::Mut);
        let name = self.expect_ident();
        self.expect(TokenKind::Colon);
        let ty = self.parse_type();
        Param {
            id: self.id(),
            span: self.span_from(start),
            mutable,
            name,
            ty,
        }
    }

    fn parse_field_decl(&mut self) -> FieldDecl {
        let start = self.start();
        let name = self.expect_ident();
        self.expect(TokenKind::Colon);
        let ty = self.parse_type();
        FieldDecl {
            id: self.id(),
            span: self.span_from(start),
            name,
            ty,
        }
    }

    fn parse_struct(&mut self) -> StructDecl {
        self.bump();
        let name = self.expect_ident();
        let generics = self.parse_generics();
        let fields = if self.at(&TokenKind::LBrace) {
            let open = self.bump().span;
            self.comma_list(open, TokenKind::RBrace, &[], |p| p.parse_field_decl())
        } else {
            self.unexpected("E0101", "`{`");
            vec![]
        };
        StructDecl {
            name,
            generics,
            fields,
        }
    }

    fn parse_enum(&mut self) -> EnumDecl {
        self.bump();
        let name = self.expect_ident();
        let generics = self.parse_generics();
        let variants = if self.at(&TokenKind::LBrace) {
            let open = self.bump().span;
            self.comma_list(open, TokenKind::RBrace, &[], |p| p.parse_variant())
        } else {
            self.unexpected("E0101", "`{`");
            vec![]
        };
        EnumDecl {
            name,
            generics,
            variants,
        }
    }

    fn parse_variant(&mut self) -> Variant {
        let start = self.start();
        let name = self.expect_ident();
        let kind = if self.at(&TokenKind::LParen) {
            let open = self.bump().span;
            VariantKind::Tuple(self.comma_list(open, TokenKind::RParen, &[], |p| p.parse_type()))
        } else if self.at(&TokenKind::LBrace) {
            let open = self.bump().span;
            VariantKind::Struct(
                self.comma_list(open, TokenKind::RBrace, &[], |p| p.parse_field_decl()),
            )
        } else {
            VariantKind::Unit
        };
        Variant {
            id: self.id(),
            span: self.span_from(start),
            name,
            kind,
        }
    }

    fn parse_const(&mut self) -> ConstDecl {
        self.bump();
        let name = self.expect_ident();
        self.expect(TokenKind::Colon);
        let ty = self.parse_type();
        self.expect(TokenKind::Eq);
        let value = self.parse_expr();
        self.expect_semi();
        ConstDecl { name, ty, value }
    }

    // ---------------------------------------------------------------- lists

    fn at_list_stop(&self, close: &TokenKind, extra: &[TokenKind]) -> bool {
        let k = self.nth(0);
        *k == TokenKind::Eof
            || *k == TokenKind::Semi
            || self.is_item_start()
            || (*close != TokenKind::RBrace && *k == TokenKind::RBrace)
            || extra.contains(k)
    }

    /// Parse `elem (, elem)* ,? close` after the opening delimiter was consumed.
    fn comma_list<T>(
        &mut self,
        open: Span,
        close: TokenKind,
        extra_stops: &[TokenKind],
        mut elem: impl FnMut(&mut Self) -> T,
    ) -> Vec<T> {
        let mut out = Vec::new();
        loop {
            if self.at(&close) {
                self.bump();
                break;
            }
            if self.at_list_stop(&close, extra_stops) {
                self.unclosed(open, &close);
                break;
            }
            let before = self.pos;
            out.push(elem(self));
            if self.eat(&TokenKind::Comma)
                || self.at(&close)
                || self.at_list_stop(&close, extra_stops)
            {
                continue;
            }
            self.unexpected("E0101", &format!("`,` or {}", close.describe()));
            if self.pos == before {
                self.bump();
            }
        }
        out
    }

    // ---------------------------------------------------------------- types

    fn parse_type(&mut self) -> Type {
        if !self.enter() {
            let span = self.err_span();
            return Type {
                id: self.id(),
                span,
                kind: TypeKind::Error,
            };
        }
        let t = self.type_inner();
        self.depth -= 1;
        t
    }

    fn type_inner(&mut self) -> Type {
        let start = self.start();
        let kind = match self.kind() {
            TokenKind::Ident => {
                let name = self.expect_ident();
                let args = if self.at(&TokenKind::Lt) {
                    let open = self.bump().span;
                    self.comma_list(
                        open,
                        TokenKind::Gt,
                        &[TokenKind::LBrace, TokenKind::LParen, TokenKind::Eq],
                        |p| p.parse_type(),
                    )
                } else {
                    vec![]
                };
                TypeKind::Path { name, args }
            }
            TokenKind::LParen => {
                let open = self.bump().span;
                let mut tys = self.comma_list(open, TokenKind::RParen, &[], |p| p.parse_type());
                if tys.len() == 1 {
                    return tys.remove(0);
                }
                TypeKind::Tuple(tys)
            }
            TokenKind::LBracket => {
                self.bump();
                let inner = self.parse_type();
                self.expect(TokenKind::RBracket);
                TypeKind::Array(Box::new(inner))
            }
            TokenKind::Fn => {
                self.bump();
                let params = if self.at(&TokenKind::LParen) {
                    let open = self.bump().span;
                    self.comma_list(open, TokenKind::RParen, &[], |p| p.parse_type())
                } else {
                    self.unexpected("E0101", "`(`");
                    vec![]
                };
                let ret = if self.eat(&TokenKind::Arrow) {
                    Some(Box::new(self.parse_type()))
                } else {
                    None
                };
                TypeKind::Fn { params, ret }
            }
            TokenKind::Bang => {
                self.bump();
                TypeKind::Never
            }
            _ => {
                self.unexpected("E0103", "a type");
                let span = self.err_span();
                return Type {
                    id: self.id(),
                    span,
                    kind: TypeKind::Error,
                };
            }
        };
        Type {
            id: self.id(),
            span: self.span_from(start),
            kind,
        }
    }

    // ---------------------------------------------------------------- patterns

    fn parse_pattern(&mut self) -> Pattern {
        if !self.enter() {
            let span = self.err_span();
            return Pattern {
                id: self.id(),
                span,
                kind: PatKind::Error,
            };
        }
        let start = self.start();
        let first = self.pattern_atom();
        let p = if self.at(&TokenKind::Pipe) {
            let mut alts = vec![first];
            while self.eat(&TokenKind::Pipe) {
                alts.push(self.pattern_atom());
            }
            Pattern {
                id: self.id(),
                span: self.span_from(start),
                kind: PatKind::Or(alts),
            }
        } else {
            first
        };
        self.depth -= 1;
        p
    }

    fn pat(&mut self, start: u32, kind: PatKind) -> Pattern {
        Pattern {
            id: self.id(),
            span: self.span_from(start),
            kind,
        }
    }

    fn parse_path(&mut self) -> Path {
        let start = self.start();
        let mut segments = vec![self.expect_ident()];
        if self.at(&TokenKind::ColonColon) {
            self.bump();
            segments.push(self.expect_ident());
        }
        Path {
            id: self.id(),
            span: self.span_from(start),
            segments,
        }
    }

    fn pattern_atom(&mut self) -> Pattern {
        let start = self.start();
        match self.kind().clone() {
            TokenKind::Underscore => {
                self.bump();
                self.pat(start, PatKind::Wild)
            }
            TokenKind::Mut => {
                self.bump();
                let name = self.expect_ident();
                self.pat(
                    start,
                    PatKind::Binding {
                        mutable: true,
                        name,
                    },
                )
            }
            TokenKind::Int(v) => {
                self.bump();
                self.pat(start, PatKind::Lit(Lit::Int(v)))
            }
            TokenKind::Minus if matches!(self.nth(1), TokenKind::Int(_)) => {
                self.bump();
                let v = match self.bump().kind {
                    TokenKind::Int(v) => v,
                    _ => 0,
                };
                self.pat(start, PatKind::Lit(Lit::Int(v.wrapping_neg())))
            }
            TokenKind::Str(s) => {
                self.bump();
                self.pat(start, PatKind::Lit(Lit::Str(s)))
            }
            TokenKind::True | TokenKind::False => {
                let v = self.bump().kind == TokenKind::True;
                self.pat(start, PatKind::Lit(Lit::Bool(v)))
            }
            TokenKind::LParen => {
                let open = self.bump().span;
                let mut ps = self.comma_list(open, TokenKind::RParen, &[], |p| p.parse_pattern());
                if ps.is_empty() {
                    return self.pat(start, PatKind::Lit(Lit::Unit));
                }
                if ps.len() == 1
                    && !matches!(self.toks[self.pos.saturating_sub(2)].kind, TokenKind::Comma)
                {
                    return ps.remove(0);
                }
                self.pat(start, PatKind::Tuple(ps))
            }
            TokenKind::Ident => {
                if *self.nth(1) == TokenKind::ColonColon
                    || *self.nth(1) == TokenKind::LBrace && !self.no_struct_pat()
                {
                    return self.adt_pattern(start);
                }
                let name = self.expect_ident();
                if self.at(&TokenKind::LParen) {
                    // `Foo(...)` without an enum prefix: parse as an Adt pattern so sema can explain.
                    let path = Path {
                        id: self.id(),
                        span: name.span,
                        segments: vec![name],
                    };
                    return self.adt_pattern_rest(start, path);
                }
                self.pat(
                    start,
                    PatKind::Binding {
                        mutable: false,
                        name,
                    },
                )
            }
            _ => {
                self.unexpected("E0104", "a pattern");
                let span = self.err_span();
                Pattern {
                    id: self.id(),
                    span,
                    kind: PatKind::Error,
                }
            }
        }
    }

    fn no_struct_pat(&self) -> bool {
        false
    }

    fn adt_pattern(&mut self, start: u32) -> Pattern {
        let path = self.parse_path();
        self.adt_pattern_rest(start, path)
    }

    fn adt_pattern_rest(&mut self, start: u32, path: Path) -> Pattern {
        let fields = if self.at(&TokenKind::LParen) {
            let open = self.bump().span;
            PatFields::Tuple(self.comma_list(open, TokenKind::RParen, &[], |p| p.parse_pattern()))
        } else if self.at(&TokenKind::LBrace) {
            let open = self.bump().span;
            let items = self.comma_list(open, TokenKind::RBrace, &[], |p| p.parse_field_pat());
            let mut rest = false;
            let mut fields = Vec::new();
            for it in items {
                match it {
                    PField::Field(f) => fields.push(f),
                    PField::Rest => rest = true,
                }
            }
            PatFields::Struct { fields, rest }
        } else {
            PatFields::Unit
        };
        self.pat(start, PatKind::Adt { path, fields })
    }

    fn parse_field_pat(&mut self) -> PField {
        if self.eat(&TokenKind::DotDot) {
            return PField::Rest;
        }
        let start = self.start();
        let mutable = self.eat(&TokenKind::Mut);
        let name = self.expect_ident();
        let pat = if !mutable && self.eat(&TokenKind::Colon) {
            self.parse_pattern()
        } else {
            let nid = self.id();
            let alias = Ident {
                id: nid,
                span: name.span,
                name: name.name.clone(),
            };
            let pid = self.id();
            Pattern {
                id: pid,
                span: name.span,
                kind: PatKind::Binding {
                    mutable,
                    name: alias,
                },
            }
        };
        PField::Field(FieldPat {
            id: self.id(),
            span: self.span_from(start),
            name,
            pat,
        })
    }

    // ---------------------------------------------------------------- blocks and statements

    fn parse_block(&mut self) -> Block {
        let start = self.start();
        if !self.at(&TokenKind::LBrace) {
            self.unexpected("E0101", "`{`");
            let span = self.err_span();
            return Block {
                id: self.id(),
                span,
                stmts: vec![],
                tail: None,
            };
        }
        if !self.enter() {
            let span = self.err_span();
            return Block {
                id: self.id(),
                span,
                stmts: vec![],
                tail: None,
            };
        }
        let open = self.bump().span;
        let saved = std::mem::replace(&mut self.no_struct, false);
        let mut stmts = Vec::new();
        let mut tail = None;
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at(&TokenKind::Eof) || self.is_item_start() {
                self.unclosed(open, &TokenKind::RBrace);
                break;
            }
            if self.eat(&TokenKind::Semi) {
                continue;
            }
            let before = self.pos;
            let s0 = self.start();
            match self.parse_stmt() {
                Parsed::Stmt(s) if self.pos != before => stmts.push(s),
                Parsed::Tail(e) if self.pos != before => {
                    tail = Some(Box::new(e));
                    // A tail must be last; anything else is reported by expect-`}`.
                    if !self.at(&TokenKind::RBrace) {
                        let e = tail.take().map(|b| *b);
                        if let Some(e) = e {
                            let id = self.id();
                            stmts.push(Stmt {
                                id,
                                span: e.span,
                                kind: StmtKind::Expr {
                                    expr: e,
                                    semi: true,
                                },
                            });
                        }
                    }
                }
                _ => {
                    // No progress: skip to a synchronisation point, remembering what we dropped.
                    self.skip_stmt();
                    let span = Span::new(s0 as usize, self.prev_end.max(s0) as usize);
                    let id = self.id();
                    stmts.push(Stmt {
                        id,
                        span,
                        kind: StmtKind::Error,
                    });
                }
            }
        }
        self.no_struct = saved;
        self.depth -= 1;
        Block {
            id: self.id(),
            span: self.span_from(start),
            stmts,
            tail,
        }
    }

    /// Skip tokens to the end of the current statement: past a `;`, or up to (not
    /// including) a `}` / item keyword / EOF. Balanced groups are skipped whole.
    fn skip_stmt(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.kind() {
                TokenKind::Eof => break,
                _ if self.is_item_start() => break,
                TokenKind::Semi if depth == 0 => {
                    self.bump();
                    break;
                }
                TokenKind::RBrace if depth == 0 => break,
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn stmt(&mut self, start: u32, kind: StmtKind) -> Stmt {
        Stmt {
            id: self.id(),
            span: self.span_from(start),
            kind,
        }
    }

    fn parse_stmt(&mut self) -> Parsed {
        let start = self.start();
        if self.at(&TokenKind::Let) {
            return Parsed::Stmt(self.parse_let(start));
        }
        let block_like = matches!(
            self.kind(),
            TokenKind::If
                | TokenKind::Match
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::LBrace
        );
        let expr = if block_like {
            self.primary()
        } else {
            self.parse_expr()
        };
        if self.at(&TokenKind::RBrace) {
            return Parsed::Tail(expr);
        }
        if self.eat(&TokenKind::Semi) {
            return Parsed::Stmt(self.stmt(start, StmtKind::Expr { expr, semi: true }));
        }
        if block_like {
            return Parsed::Stmt(self.stmt(start, StmtKind::Expr { expr, semi: false }));
        }
        self.expect_semi();
        Parsed::Stmt(self.stmt(start, StmtKind::Expr { expr, semi: true }))
    }

    fn parse_let(&mut self, start: u32) -> Stmt {
        self.bump();
        let pat = self.parse_pattern();
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        let init = if self.eat(&TokenKind::Eq) {
            Some(self.parse_expr())
        } else {
            if !self.suppressed() {
                let at = Span::point(self.prev_end as usize);
                self.diags.push(
                    Diagnostic::error("E0107", "`let` binding has no initialiser")
                        .primary(at, "expected `= value` here")
                        .help("every binding must be initialised, e.g. `let x = 0;`"),
                );
            }
            None
        };
        self.expect_semi();
        self.stmt(start, StmtKind::Let { pat, ty, init })
    }

    // ---------------------------------------------------------------- expressions

    fn mk(&mut self, start: u32, kind: ExprKind) -> Expr {
        Expr {
            id: self.id(),
            span: self.span_from(start),
            kind,
        }
    }
    fn error_expr(&mut self) -> Expr {
        let span = self.err_span();
        Expr {
            id: self.id(),
            span,
            kind: ExprKind::Error,
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.expr_bp(0)
    }

    /// An expression where `Name {` must not start a struct literal (`if`, `while`, `match` heads).
    fn parse_expr_no_struct(&mut self) -> Expr {
        let saved = std::mem::replace(&mut self.no_struct, true);
        let e = self.parse_expr();
        self.no_struct = saved;
        e
    }

    fn expr_bp(&mut self, min_bp: u8) -> Expr {
        if !self.enter() {
            return self.error_expr();
        }
        let e = self.expr_bp_inner(min_bp);
        self.depth -= 1;
        e
    }

    fn expr_bp_inner(&mut self, min_bp: u8) -> Expr {
        let start = self.start();
        let mut lhs = self.unary();
        let mut chain = 0;
        loop {
            chain += 1;
            if chain > MAX_CHAIN {
                self.bail("this operator chain is too long");
                break;
            }
            let k = self.kind().clone();
            if k == TokenKind::As {
                if AS_BP < min_bp {
                    break;
                }
                self.bump();
                let ty = self.parse_type();
                lhs = self.mk(start, ExprKind::Cast(Box::new(lhs), ty));
                continue;
            }
            let Some((op, lbp, rbp, assign)) = infix(&k) else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            let op_span = self.bump().span;
            let rhs = self.expr_bp(rbp);
            if assign {
                lhs = self.mk(
                    start,
                    ExprKind::Assign {
                        op,
                        target: Box::new(lhs),
                        value: Box::new(rhs),
                    },
                );
                continue;
            }
            let op = op.unwrap_or(BinOp::Add);
            let (lstart, rend) = (lhs.span.start, rhs.span.end);
            lhs = self.mk(start, ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)));
            if op.is_comparison() && is_cmp(self.kind()) && !self.suppressed() {
                let next = self.span();
                let mut d = Diagnostic::error("E0109", "comparison operators cannot be chained")
                    .primary(next, "chained comparison")
                    .secondary(op_span, "first comparison")
                    .help("use `&&` to combine comparisons, or parenthesise explicitly");
                d.suggestions.push(Suggestion {
                    message: "parenthesise the first comparison".into(),
                    edits: vec![
                        Edit {
                            span: Span::point(lstart as usize),
                            replacement: "(".into(),
                        },
                        Edit {
                            span: Span::point(rend as usize),
                            replacement: ")".into(),
                        },
                    ],
                    applicability: Applicability::MaybeIncorrect,
                });
                self.diags.push(d);
            }
        }
        lhs
    }

    fn unary(&mut self) -> Expr {
        let start = self.start();
        let op = match self.kind() {
            TokenKind::Minus => UnOp::Neg,
            TokenKind::Bang => UnOp::Not,
            _ => return self.postfix(),
        };
        if !self.enter() {
            return self.error_expr();
        }
        self.bump();
        let inner = self.unary();
        self.depth -= 1;
        self.mk(start, ExprKind::Unary(op, Box::new(inner)))
    }

    fn call_args(&mut self) -> Vec<Expr> {
        let open = self.bump().span;
        let saved = std::mem::replace(&mut self.no_struct, false);
        let args = self.comma_list(open, TokenKind::RParen, &[], |p| p.parse_expr());
        self.no_struct = saved;
        args
    }

    fn postfix(&mut self) -> Expr {
        let start = self.start();
        let mut e = self.primary();
        let mut chain = 0;
        loop {
            chain += 1;
            if chain > MAX_CHAIN {
                self.bail("this postfix chain is too long");
                break;
            }
            match self.kind() {
                TokenKind::LParen => {
                    let args = self.call_args();
                    e = self.mk(
                        start,
                        ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                    );
                }
                TokenKind::LBracket => {
                    let open = self.bump().span;
                    let saved = std::mem::replace(&mut self.no_struct, false);
                    let idx = self.parse_expr();
                    self.no_struct = saved;
                    if !self.eat(&TokenKind::RBracket) {
                        self.unclosed(open, &TokenKind::RBracket);
                    }
                    e = self.mk(start, ExprKind::Index(Box::new(e), Box::new(idx)));
                }
                TokenKind::Dot => {
                    self.bump();
                    match self.kind().clone() {
                        TokenKind::Int(v) => {
                            let t = self.bump();
                            let n = u32::try_from(v).unwrap_or(u32::MAX);
                            e = self.mk(start, ExprKind::TupleField(Box::new(e), n, t.span));
                        }
                        TokenKind::Ident => {
                            let name = self.expect_ident();
                            if self.at(&TokenKind::LParen) {
                                let args = self.call_args();
                                e = self.mk(
                                    start,
                                    ExprKind::MethodCall {
                                        recv: Box::new(e),
                                        method: name,
                                        args,
                                    },
                                );
                            } else {
                                e = self.mk(start, ExprKind::Field(Box::new(e), name));
                            }
                        }
                        _ => {
                            self.unexpected("E0101", "a field name or tuple index after `.`");
                            let name = self.expect_ident();
                            e = self.mk(start, ExprKind::Field(Box::new(e), name));
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        e
    }

    fn looks_like_struct_lit(&self) -> bool {
        *self.nth(0) == TokenKind::LBrace
            && (*self.nth(1) == TokenKind::RBrace
                || (*self.nth(1) == TokenKind::Ident && *self.nth(2) == TokenKind::Colon))
    }

    fn primary(&mut self) -> Expr {
        let start = self.start();
        match self.kind().clone() {
            TokenKind::Int(v) => {
                self.bump();
                self.mk(start, ExprKind::Lit(Lit::Int(v)))
            }
            TokenKind::Str(s) => {
                self.bump();
                self.mk(start, ExprKind::Lit(Lit::Str(s)))
            }
            TokenKind::True | TokenKind::False => {
                let v = self.bump().kind == TokenKind::True;
                self.mk(start, ExprKind::Lit(Lit::Bool(v)))
            }
            TokenKind::Ident => {
                let path = self.parse_path();
                if !self.no_struct && self.looks_like_struct_lit() {
                    return self.struct_lit(start, path);
                }
                self.mk(start, ExprKind::Path(path))
            }
            TokenKind::LParen => self.paren(start),
            TokenKind::LBracket => {
                let open = self.bump().span;
                let saved = std::mem::replace(&mut self.no_struct, false);
                let xs = self.comma_list(open, TokenKind::RBracket, &[], |p| p.parse_expr());
                self.no_struct = saved;
                self.mk(start, ExprKind::Array(xs))
            }
            TokenKind::LBrace => {
                let b = self.parse_block();
                self.mk(start, ExprKind::Block(b))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::Match => self.parse_match(),
            TokenKind::While => {
                self.bump();
                let cond = self.parse_expr_no_struct();
                let body = self.parse_block();
                self.mk(
                    start,
                    ExprKind::While {
                        cond: Box::new(cond),
                        body,
                    },
                )
            }
            TokenKind::Loop => {
                self.bump();
                let body = self.parse_block();
                self.mk(start, ExprKind::Loop(body))
            }
            TokenKind::Break => {
                self.bump();
                let v = if self.starts_expr() {
                    Some(Box::new(self.parse_expr()))
                } else {
                    None
                };
                self.mk(start, ExprKind::Break(v))
            }
            TokenKind::Continue => {
                self.bump();
                self.mk(start, ExprKind::Continue)
            }
            TokenKind::Return => {
                self.bump();
                let v = if self.starts_expr() {
                    Some(Box::new(self.parse_expr()))
                } else {
                    None
                };
                self.mk(start, ExprKind::Return(v))
            }
            _ => {
                self.unexpected("E0102", "an expression");
                self.error_expr()
            }
        }
    }

    fn starts_expr(&self) -> bool {
        use TokenKind::*;
        matches!(
            self.nth(0),
            Int(_)
                | Str(_)
                | True
                | False
                | Ident
                | LParen
                | LBracket
                | LBrace
                | If
                | Match
                | While
                | Loop
                | Break
                | Continue
                | Return
                | Minus
                | Bang
        ) && !(self.no_struct && *self.nth(0) == LBrace)
    }

    fn paren(&mut self, start: u32) -> Expr {
        let open = self.bump().span;
        let saved = std::mem::replace(&mut self.no_struct, false);
        if self.eat(&TokenKind::RParen) {
            self.no_struct = saved;
            return self.mk(start, ExprKind::Lit(Lit::Unit));
        }
        let first = self.parse_expr();
        let result = if self.at(&TokenKind::Comma) {
            let mut xs = vec![first];
            while self.eat(&TokenKind::Comma) {
                if self.at(&TokenKind::RParen) {
                    break;
                }
                let before = self.pos;
                xs.push(self.parse_expr());
                if self.pos == before {
                    break;
                }
            }
            if !self.eat(&TokenKind::RParen) {
                self.unclosed(open, &TokenKind::RParen);
            }
            self.mk(start, ExprKind::Tuple(xs))
        } else {
            if !self.eat(&TokenKind::RParen) {
                self.unclosed(open, &TokenKind::RParen);
            }
            self.mk(start, ExprKind::Paren(Box::new(first)))
        };
        self.no_struct = saved;
        result
    }

    fn struct_lit(&mut self, start: u32, path: Path) -> Expr {
        let open = self.bump().span;
        let saved = std::mem::replace(&mut self.no_struct, false);
        let fields = self.comma_list(open, TokenKind::RBrace, &[], |p| {
            let fstart = p.start();
            let name = p.expect_ident();
            p.expect(TokenKind::Colon);
            let value = p.parse_expr();
            FieldInit {
                id: p.id(),
                span: p.span_from(fstart),
                name,
                value,
            }
        });
        self.no_struct = saved;
        self.mk(start, ExprKind::Struct { path, fields })
    }

    fn parse_if(&mut self) -> Expr {
        let start = self.start();
        self.bump();
        let cond = self.parse_expr_no_struct();
        let then = self.parse_block();
        let els = if self.eat(&TokenKind::Else) {
            if self.at(&TokenKind::If) {
                if !self.enter() {
                    return self.error_expr();
                }
                let e = self.parse_if();
                self.depth -= 1;
                Some(Box::new(e))
            } else {
                let bstart = self.start();
                let b = self.parse_block();
                Some(Box::new(self.mk(bstart, ExprKind::Block(b))))
            }
        } else {
            None
        };
        self.mk(
            start,
            ExprKind::If {
                cond: Box::new(cond),
                then,
                els,
            },
        )
    }

    fn parse_match(&mut self) -> Expr {
        let start = self.start();
        self.bump();
        let scrut = self.parse_expr_no_struct();
        let mut arms = Vec::new();
        if !self.at(&TokenKind::LBrace) {
            self.unexpected("E0101", "`{`");
            return self.mk(
                start,
                ExprKind::Match {
                    scrut: Box::new(scrut),
                    arms,
                },
            );
        }
        let open = self.bump().span;
        let saved = std::mem::replace(&mut self.no_struct, false);
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            if self.at_list_stop(&TokenKind::RBrace, &[]) {
                self.unclosed(open, &TokenKind::RBrace);
                break;
            }
            let before = self.pos;
            let astart = self.start();
            let pat = self.parse_pattern();
            let guard = if self.eat(&TokenKind::If) {
                Some(self.parse_expr())
            } else {
                None
            };
            self.expect(TokenKind::FatArrow);
            let bstart = self.start();
            let body_is_block = self.at(&TokenKind::LBrace);
            let body = if body_is_block {
                let b = self.parse_block();
                self.mk(bstart, ExprKind::Block(b))
            } else {
                self.parse_expr()
            };
            arms.push(MatchArm {
                id: self.id(),
                span: self.span_from(astart),
                pat,
                guard,
                body,
            });
            if self.eat(&TokenKind::Comma) || self.at(&TokenKind::RBrace) {
                continue;
            }
            let blocky = body_is_block
                || matches!(
                    arms.last().map(|a| &a.body.kind),
                    Some(
                        ExprKind::If { .. }
                            | ExprKind::Match { .. }
                            | ExprKind::Loop(_)
                            | ExprKind::While { .. }
                    )
                );
            if !blocky && !self.at_list_stop(&TokenKind::RBrace, &[]) {
                self.unexpected("E0101", "`,` or `}`");
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.no_struct = saved;
        self.mk(
            start,
            ExprKind::Match {
                scrut: Box::new(scrut),
                arms,
            },
        )
    }
}
