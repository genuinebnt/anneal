//! The abstract syntax tree. Every node carries a [`Span`] and a unique [`NodeId`].
//! Malformed input is represented by explicit `Error` variants so the tree is
//! always complete; later phases silently skip those nodes (no cascading errors).
//!
//! [`Node`] gives a uniform, read-only view of the tree for walkers (formatter,
//! highlighter, LSP "node at offset", the parser's own span-nesting tests).

use crate::source::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Clone, Debug)]
pub struct Ident {
    pub id: NodeId,
    pub span: Span,
    /// Empty when the identifier was missing in the source (parse error).
    pub name: String,
}

/// `name` or `Enum::Variant`.
#[derive(Clone, Debug)]
pub struct Path {
    pub id: NodeId,
    pub span: Span,
    pub segments: Vec<Ident>,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
    /// Number of NodeIds handed out; ids are `0..node_count`.
    pub node_count: u32,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: NodeId,
    pub span: Span,
    pub kind: ItemKind,
}

#[derive(Clone, Debug)]
pub enum ItemKind {
    Fn(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Const(ConstDecl),
    Error,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: Ident,
    pub generics: Vec<Ident>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub id: NodeId,
    pub span: Span,
    pub mutable: bool,
    pub name: Ident,
    pub ty: Type,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub name: Ident,
    pub generics: Vec<Ident>,
    pub fields: Vec<FieldDecl>,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub id: NodeId,
    pub span: Span,
    pub name: Ident,
    pub ty: Type,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: Ident,
    pub generics: Vec<Ident>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub id: NodeId,
    pub span: Span,
    pub name: Ident,
    pub kind: VariantKind,
}

#[derive(Clone, Debug)]
pub enum VariantKind {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<FieldDecl>),
}

#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct Type {
    pub id: NodeId,
    pub span: Span,
    pub kind: TypeKind,
}

#[derive(Clone, Debug)]
pub enum TypeKind {
    /// `int`, `Point`, `Option<int>`; builtin names are resolved by sema.
    Path {
        name: Ident,
        args: Vec<Type>,
    },
    Array(Box<Type>),
    /// `()` is the empty tuple.
    Tuple(Vec<Type>),
    Fn {
        params: Vec<Type>,
        ret: Option<Box<Type>>,
    },
    Never,
    Error,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub id: NodeId,
    pub span: Span,
    pub stmts: Vec<Stmt>,
    /// The final expression without a trailing `;`, which is the block's value.
    pub tail: Option<Box<Expr>>,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub id: NodeId,
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Let {
        pat: Pattern,
        ty: Option<Type>,
        init: Option<Expr>,
    },
    /// `semi == false` only for block-like expressions (`if`, `match`, loops, blocks).
    Expr {
        expr: Expr,
        semi: bool,
    },
    Error,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    Bool(bool),
    Str(String),
    Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Debug)]
pub struct FieldInit {
    pub id: NodeId,
    pub span: Span,
    pub name: Ident,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub id: NodeId,
    pub span: Span,
    pub pat: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Lit(Lit),
    Path(Path),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `target = value` (`op == None`) or `target op= value`.
    Assign {
        op: Option<BinOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    MethodCall {
        recv: Box<Expr>,
        method: Ident,
        args: Vec<Expr>,
    },
    Field(Box<Expr>, Ident),
    /// `t.0`; the span covers the digits.
    TupleField(Box<Expr>, u32, Span),
    Index(Box<Expr>, Box<Expr>),
    Struct {
        path: Path,
        fields: Vec<FieldInit>,
    },
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    If {
        cond: Box<Expr>,
        then: Block,
        els: Option<Box<Expr>>,
    },
    Match {
        scrut: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Block(Block),
    While {
        cond: Box<Expr>,
        body: Block,
    },
    Loop(Block),
    Break(Option<Box<Expr>>),
    Continue,
    Return(Option<Box<Expr>>),
    Cast(Box<Expr>, Type),
    Paren(Box<Expr>),
    Error,
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub id: NodeId,
    pub span: Span,
    pub kind: PatKind,
}

#[derive(Clone, Debug)]
pub enum PatKind {
    Wild,
    Binding {
        mutable: bool,
        name: Ident,
    },
    Lit(Lit),
    Tuple(Vec<Pattern>),
    /// A struct or enum-variant pattern, decided by name resolution:
    /// `Point { x, .. }`, `Shape::Circle(r)`, `Option::None`.
    Adt {
        path: Path,
        fields: PatFields,
    },
    Or(Vec<Pattern>),
    Error,
}

#[derive(Clone, Debug)]
pub enum PatFields {
    Unit,
    Tuple(Vec<Pattern>),
    Struct { fields: Vec<FieldPat>, rest: bool },
}

#[derive(Clone, Debug)]
pub struct FieldPat {
    pub id: NodeId,
    pub span: Span,
    pub name: Ident,
    pub pat: Pattern,
}

/// A borrowed view of any syntax node.
#[derive(Clone, Copy, Debug)]
pub enum Node<'a> {
    Module(&'a Module),
    Item(&'a Item),
    Param(&'a Param),
    Field(&'a FieldDecl),
    Variant(&'a Variant),
    Type(&'a Type),
    Block(&'a Block),
    Stmt(&'a Stmt),
    Expr(&'a Expr),
    Arm(&'a MatchArm),
    FieldInit(&'a FieldInit),
    Pattern(&'a Pattern),
    FieldPat(&'a FieldPat),
    Ident(&'a Ident),
    Path(&'a Path),
}

impl<'a> Node<'a> {
    pub fn span(&self) -> Span {
        match self {
            Node::Module(n) => n.span,
            Node::Item(n) => n.span,
            Node::Param(n) => n.span,
            Node::Field(n) => n.span,
            Node::Variant(n) => n.span,
            Node::Type(n) => n.span,
            Node::Block(n) => n.span,
            Node::Stmt(n) => n.span,
            Node::Expr(n) => n.span,
            Node::Arm(n) => n.span,
            Node::FieldInit(n) => n.span,
            Node::Pattern(n) => n.span,
            Node::FieldPat(n) => n.span,
            Node::Ident(n) => n.span,
            Node::Path(n) => n.span,
        }
    }

    /// `None` only for the module root.
    pub fn id(&self) -> Option<NodeId> {
        Some(match self {
            Node::Module(_) => return None,
            Node::Item(n) => n.id,
            Node::Param(n) => n.id,
            Node::Field(n) => n.id,
            Node::Variant(n) => n.id,
            Node::Type(n) => n.id,
            Node::Block(n) => n.id,
            Node::Stmt(n) => n.id,
            Node::Expr(n) => n.id,
            Node::Arm(n) => n.id,
            Node::FieldInit(n) => n.id,
            Node::Pattern(n) => n.id,
            Node::FieldPat(n) => n.id,
            Node::Ident(n) => n.id,
            Node::Path(n) => n.id,
        })
    }

    /// Direct children in source order.
    pub fn children(&self) -> Vec<Node<'a>> {
        let mut v: Vec<Node<'a>> = Vec::new();
        match *self {
            Node::Module(m) => v.extend(m.items.iter().map(Node::Item)),
            Node::Item(i) => match &i.kind {
                ItemKind::Fn(f) => {
                    v.push(Node::Ident(&f.name));
                    v.extend(f.generics.iter().map(Node::Ident));
                    v.extend(f.params.iter().map(Node::Param));
                    v.extend(f.ret.iter().map(Node::Type));
                    v.push(Node::Block(&f.body));
                }
                ItemKind::Struct(s) => {
                    v.push(Node::Ident(&s.name));
                    v.extend(s.generics.iter().map(Node::Ident));
                    v.extend(s.fields.iter().map(Node::Field));
                }
                ItemKind::Enum(e) => {
                    v.push(Node::Ident(&e.name));
                    v.extend(e.generics.iter().map(Node::Ident));
                    v.extend(e.variants.iter().map(Node::Variant));
                }
                ItemKind::Const(c) => {
                    v.push(Node::Ident(&c.name));
                    v.push(Node::Type(&c.ty));
                    v.push(Node::Expr(&c.value));
                }
                ItemKind::Error => {}
            },
            Node::Param(p) => {
                v.push(Node::Ident(&p.name));
                v.push(Node::Type(&p.ty));
            }
            Node::Field(f) => {
                v.push(Node::Ident(&f.name));
                v.push(Node::Type(&f.ty));
            }
            Node::Variant(x) => {
                v.push(Node::Ident(&x.name));
                match &x.kind {
                    VariantKind::Unit => {}
                    VariantKind::Tuple(ts) => v.extend(ts.iter().map(Node::Type)),
                    VariantKind::Struct(fs) => v.extend(fs.iter().map(Node::Field)),
                }
            }
            Node::Type(t) => match &t.kind {
                TypeKind::Path { name, args } => {
                    v.push(Node::Ident(name));
                    v.extend(args.iter().map(Node::Type));
                }
                TypeKind::Array(e) => v.push(Node::Type(e)),
                TypeKind::Tuple(ts) => v.extend(ts.iter().map(Node::Type)),
                TypeKind::Fn { params, ret } => {
                    v.extend(params.iter().map(Node::Type));
                    v.extend(ret.iter().map(|r| Node::Type(r)));
                }
                TypeKind::Never | TypeKind::Error => {}
            },
            Node::Block(b) => {
                v.extend(b.stmts.iter().map(Node::Stmt));
                v.extend(b.tail.iter().map(|e| Node::Expr(e)));
            }
            Node::Stmt(s) => match &s.kind {
                StmtKind::Let { pat, ty, init } => {
                    v.push(Node::Pattern(pat));
                    v.extend(ty.iter().map(Node::Type));
                    v.extend(init.iter().map(Node::Expr));
                }
                StmtKind::Expr { expr, .. } => v.push(Node::Expr(expr)),
                StmtKind::Error => {}
            },
            Node::Expr(e) => match &e.kind {
                ExprKind::Lit(_) | ExprKind::Continue | ExprKind::Error => {}
                ExprKind::Path(p) => v.push(Node::Path(p)),
                ExprKind::Unary(_, a) | ExprKind::Paren(a) => v.push(Node::Expr(a)),
                ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                    v.push(Node::Expr(a));
                    v.push(Node::Expr(b));
                }
                ExprKind::Assign { target, value, .. } => {
                    v.push(Node::Expr(target));
                    v.push(Node::Expr(value));
                }
                ExprKind::Call { callee, args } => {
                    v.push(Node::Expr(callee));
                    v.extend(args.iter().map(Node::Expr));
                }
                ExprKind::MethodCall { recv, method, args } => {
                    v.push(Node::Expr(recv));
                    v.push(Node::Ident(method));
                    v.extend(args.iter().map(Node::Expr));
                }
                ExprKind::Field(a, name) => {
                    v.push(Node::Expr(a));
                    v.push(Node::Ident(name));
                }
                ExprKind::TupleField(a, _, _) => v.push(Node::Expr(a)),
                ExprKind::Struct { path, fields } => {
                    v.push(Node::Path(path));
                    v.extend(fields.iter().map(Node::FieldInit));
                }
                ExprKind::Tuple(xs) | ExprKind::Array(xs) => v.extend(xs.iter().map(Node::Expr)),
                ExprKind::If { cond, then, els } => {
                    v.push(Node::Expr(cond));
                    v.push(Node::Block(then));
                    v.extend(els.iter().map(|e| Node::Expr(e)));
                }
                ExprKind::Match { scrut, arms } => {
                    v.push(Node::Expr(scrut));
                    v.extend(arms.iter().map(Node::Arm));
                }
                ExprKind::Block(b) | ExprKind::Loop(b) => v.push(Node::Block(b)),
                ExprKind::While { cond, body } => {
                    v.push(Node::Expr(cond));
                    v.push(Node::Block(body));
                }
                ExprKind::Break(x) | ExprKind::Return(x) => {
                    v.extend(x.iter().map(|e| Node::Expr(e)))
                }
                ExprKind::Cast(a, t) => {
                    v.push(Node::Expr(a));
                    v.push(Node::Type(t));
                }
            },
            Node::Arm(a) => {
                v.push(Node::Pattern(&a.pat));
                v.extend(a.guard.iter().map(Node::Expr));
                v.push(Node::Expr(&a.body));
            }
            Node::FieldInit(f) => {
                v.push(Node::Ident(&f.name));
                v.push(Node::Expr(&f.value));
            }
            Node::Pattern(p) => match &p.kind {
                PatKind::Wild | PatKind::Lit(_) | PatKind::Error => {}
                PatKind::Binding { name, .. } => v.push(Node::Ident(name)),
                PatKind::Tuple(ps) | PatKind::Or(ps) => v.extend(ps.iter().map(Node::Pattern)),
                PatKind::Adt { path, fields } => {
                    v.push(Node::Path(path));
                    match fields {
                        PatFields::Unit => {}
                        PatFields::Tuple(ps) => v.extend(ps.iter().map(Node::Pattern)),
                        PatFields::Struct { fields, .. } => {
                            v.extend(fields.iter().map(Node::FieldPat))
                        }
                    }
                }
            },
            Node::FieldPat(f) => {
                v.push(Node::Ident(&f.name));
                v.push(Node::Pattern(&f.pat));
            }
            Node::Path(p) => v.extend(p.segments.iter().map(Node::Ident)),
            Node::Ident(_) => {}
        }
        v
    }

    /// The innermost node whose span contains `offset` (descending from `self`).
    pub fn innermost_at(self, offset: usize) -> Node<'a> {
        let mut cur = self;
        'outer: loop {
            for c in cur.children() {
                if c.span().contains(offset) {
                    cur = c;
                    continue 'outer;
                }
            }
            return cur;
        }
    }
}

impl Module {
    pub fn node(&self) -> Node<'_> {
        Node::Module(self)
    }
}
