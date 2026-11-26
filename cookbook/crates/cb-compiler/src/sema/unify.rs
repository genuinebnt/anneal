//! Unification with an occurs check: the engine of Hindley-Milner style
//! inference. Inference variables live in a union-find-like table of
//! `Option<Ty>` solutions (no path compression; types are tiny).
//!
//! Further reading: <https://course.ccs.neu.edu/cs4410sp19/lec_type-inference_notes.html>.

use super::types::Ty;

#[derive(Debug, PartialEq, Eq)]
pub enum UnifyError {
    /// The two types have different shapes (`int` vs `bool`, arity mismatch, ...).
    Mismatch(Ty, Ty),
    /// Binding the variable would create an infinite type like `T = [T]`.
    Occurs(u32, Ty),
}

#[derive(Default)]
pub struct Infer {
    solutions: Vec<Option<Ty>>,
}

impl Infer {
    pub fn new() -> Infer {
        Infer::default()
    }
    pub fn fresh(&mut self) -> Ty {
        self.solutions.push(None);
        Ty::Var(self.solutions.len() as u32 - 1)
    }
    /// Follow variable links at the top level only.
    pub fn shallow(&self, t: &Ty) -> Ty {
        let mut cur = t.clone();
        while let Ty::Var(v) = cur {
            match self.solutions.get(v as usize).and_then(|s| s.clone()) {
                Some(next) => cur = next,
                None => return Ty::Var(v),
            }
        }
        cur
    }
    /// Replace every solved variable, recursively. Unsolved ones stay as `Var`.
    pub fn zonk(&self, t: &Ty) -> Ty {
        match self.shallow(t) {
            Ty::Var(v) => Ty::Var(v),
            other => other.map_children(&mut |c| self.zonk(c)),
        }
    }
    fn occurs(&self, v: u32, t: &Ty) -> bool {
        match self.shallow(t) {
            Ty::Var(w) => v == w,
            Ty::Array(x) => self.occurs(v, &x),
            Ty::Tuple(ts) | Ty::Adt(_, _, ts) => ts.iter().any(|x| self.occurs(v, x)),
            Ty::Fn(ps, r) => ps.iter().any(|x| self.occurs(v, x)) || self.occurs(v, &r),
            _ => false,
        }
    }

    pub fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), UnifyError> {
        let a = self.shallow(a);
        let b = self.shallow(b);
        match (&a, &b) {
            (Ty::Error, _) | (_, Ty::Error) => Ok(()),
            (Ty::Var(x), Ty::Var(y)) if x == y => Ok(()),
            (Ty::Var(x), t) | (t, Ty::Var(x)) => {
                if self.occurs(*x, t) {
                    return Err(UnifyError::Occurs(*x, self.zonk(t)));
                }
                self.solutions[*x as usize] = Some(t.clone());
                Ok(())
            }
            (Ty::Array(x), Ty::Array(y)) => self.unify(x, y).map_err(|_| self.mismatch(&a, &b)),
            (Ty::Tuple(xs), Ty::Tuple(ys)) if xs.len() == ys.len() => {
                self.unify_all(xs, ys, &a, &b)
            }
            (Ty::Fn(px, rx), Ty::Fn(py, ry)) if px.len() == py.len() => {
                self.unify_all(px, py, &a, &b)?;
                self.unify(rx, ry).map_err(|_| self.mismatch(&a, &b))
            }
            (Ty::Adt(d1, _, xs), Ty::Adt(d2, _, ys)) if d1 == d2 && xs.len() == ys.len() => {
                self.unify_all(xs, ys, &a, &b)
            }
            _ if a == b => Ok(()),
            _ => Err(self.mismatch(&a, &b)),
        }
    }

    fn unify_all(&mut self, xs: &[Ty], ys: &[Ty], a: &Ty, b: &Ty) -> Result<(), UnifyError> {
        for (x, y) in xs.iter().zip(ys) {
            match self.unify(x, y) {
                Ok(()) => {}
                Err(e @ UnifyError::Occurs(..)) => return Err(e),
                Err(_) => return Err(self.mismatch(a, b)),
            }
        }
        Ok(())
    }

    fn mismatch(&self, a: &Ty, b: &Ty) -> UnifyError {
        UnifyError::Mismatch(self.zonk(a), self.zonk(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::types::DefId;

    fn adt(args: Vec<Ty>) -> Ty {
        Ty::Adt(DefId(1), "Option".into(), args)
    }

    #[test]
    fn binds_variables_and_zonks() {
        let mut i = Infer::new();
        let a = i.fresh();
        let b = i.fresh();
        i.unify(
            &Ty::Array(Box::new(a.clone())),
            &Ty::Array(Box::new(Ty::Int)),
        )
        .unwrap();
        i.unify(&b, &a).unwrap();
        assert_eq!(i.zonk(&b), Ty::Int);
    }

    #[test]
    fn occurs_check_rejects_infinite_types() {
        let mut i = Infer::new();
        let a = i.fresh();
        let err = i.unify(&a, &Ty::Array(Box::new(a.clone()))).unwrap_err();
        assert!(matches!(err, UnifyError::Occurs(0, _)));
        // Deep occurrence through a tuple inside an ADT.
        let b = i.fresh();
        let err = i
            .unify(&b, &adt(vec![Ty::Tuple(vec![Ty::Int, b.clone()])]))
            .unwrap_err();
        assert!(matches!(err, UnifyError::Occurs(..)));
    }

    #[test]
    fn mismatches_report_whole_types() {
        let mut i = Infer::new();
        let v = i.fresh();
        i.unify(&v, &Ty::Int).unwrap();
        let err = i
            .unify(&Ty::Array(Box::new(v)), &Ty::Array(Box::new(Ty::Bool)))
            .unwrap_err();
        assert_eq!(
            err,
            UnifyError::Mismatch(Ty::Array(Box::new(Ty::Int)), Ty::Array(Box::new(Ty::Bool)))
        );
        assert!(
            i.unify(
                &Ty::Tuple(vec![Ty::Int]),
                &Ty::Tuple(vec![Ty::Int, Ty::Int])
            )
            .is_err()
        );
    }

    #[test]
    fn functions_unify_pointwise_and_error_is_absorbing() {
        let mut i = Infer::new();
        let (a, r) = (i.fresh(), i.fresh());
        i.unify(
            &Ty::Fn(vec![a.clone()], Box::new(r.clone())),
            &Ty::Fn(vec![Ty::Int], Box::new(Ty::Str)),
        )
        .unwrap();
        assert_eq!(
            i.zonk(&Ty::Fn(vec![a], Box::new(r))),
            Ty::Fn(vec![Ty::Int], Box::new(Ty::Str))
        );
        assert!(i.unify(&Ty::Error, &Ty::Int).is_ok());
        assert!(i.unify(&adt(vec![Ty::Int]), &adt(vec![Ty::Bool])).is_err());
    }
}
