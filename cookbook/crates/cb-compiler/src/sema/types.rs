//! Semantic types and definition ids.

use std::fmt;
use std::sync::Arc;

/// Index into [`crate::sema::hir::Program::defs`]. Every function, struct, enum,
/// variant, field, constant, type parameter and local variable gets one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId(pub u32);

pub type Name = Arc<str>;

/// A resolved type. `Var` only exists while a function body is being inferred;
/// the typed HIR contains none (they are replaced by their solutions or `Error`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Bool,
    Str,
    Unit,
    /// The type of `return`, `break`, `panic(..)`: it never produces a value and
    /// is accepted where any type is expected.
    Never,
    Array(Box<Ty>),
    Tuple(Vec<Ty>),
    Fn(Vec<Ty>, Box<Ty>),
    /// A struct or enum with its type arguments.
    Adt(DefId, Name, Vec<Ty>),
    /// A generic parameter (rigid inside its own item).
    Param(Name),
    /// An inference variable.
    Var(u32),
    /// Poison type after an error; unifies with everything silently.
    Error,
}

impl Ty {
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    pub fn contains(&self, pred: &dyn Fn(&Ty) -> bool) -> bool {
        if pred(self) {
            return true;
        }
        match self {
            Ty::Array(t) => t.contains(pred),
            Ty::Tuple(ts) | Ty::Adt(_, _, ts) => ts.iter().any(|t| t.contains(pred)),
            Ty::Fn(ps, r) => ps.iter().any(|t| t.contains(pred)) || r.contains(pred),
            _ => false,
        }
    }
    pub fn has_var(&self) -> bool {
        self.contains(&|t| matches!(t, Ty::Var(_)))
    }
    pub fn has_error(&self) -> bool {
        self.contains(&|t| matches!(t, Ty::Error))
    }

    /// Apply `f` to every direct component, rebuilding the type.
    pub fn map_children(&self, f: &mut dyn FnMut(&Ty) -> Ty) -> Ty {
        match self {
            Ty::Array(t) => Ty::Array(Box::new(f(t))),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(&mut *f).collect()),
            Ty::Adt(d, n, ts) => Ty::Adt(*d, n.clone(), ts.iter().map(&mut *f).collect()),
            Ty::Fn(ps, r) => Ty::Fn(ps.iter().map(&mut *f).collect(), Box::new(f(r))),
            other => other.clone(),
        }
    }

    /// Replace generic parameters by name.
    pub fn subst(&self, map: &[(Name, Ty)]) -> Ty {
        match self {
            Ty::Param(n) => map
                .iter()
                .find(|(k, _)| k == n)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| self.clone()),
            _ => self.map_children(&mut |t| t.subst(map)),
        }
    }

    /// Does the type mention a function type (which cannot be compared with `==`)?
    pub fn has_fn(&self) -> bool {
        self.contains(&|t| matches!(t, Ty::Fn(..)))
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |f: &mut fmt::Formatter<'_>, ts: &[Ty]| -> fmt::Result {
            for (i, t) in ts.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{t}")?;
            }
            Ok(())
        };
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Bool => write!(f, "bool"),
            Ty::Str => write!(f, "str"),
            Ty::Unit => write!(f, "()"),
            Ty::Never => write!(f, "!"),
            Ty::Array(t) => write!(f, "[{t}]"),
            Ty::Tuple(ts) => {
                write!(f, "(")?;
                list(f, ts)?;
                if ts.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Ty::Fn(ps, r) => {
                write!(f, "fn(")?;
                list(f, ps)?;
                write!(f, ")")?;
                if **r != Ty::Unit {
                    write!(f, " -> {r}")?;
                }
                Ok(())
            }
            Ty::Adt(_, n, args) => {
                write!(f, "{n}")?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    list(f, args)?;
                    write!(f, ">")?;
                }
                Ok(())
            }
            Ty::Param(n) => write!(f, "{n}"),
            Ty::Var(_) => write!(f, "_"),
            Ty::Error => write!(f, "{{error}}"),
        }
    }
}
