/// A handle for an interned string: four bytes, `Copy`, compared and hashed as an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(pub u32);

/// Each distinct string is stored once. Symbols are dense, handed out in first-seen order from 0.
pub struct Interner {
    // TODO: what you need to go from `&str` to `Symbol` and back
}

impl Interner {
    pub fn new() -> Self {
        todo!()
    }

    /// The symbol for `s`, creating it the first time `s` is seen.
    pub fn intern(&mut self, s: &str) -> Symbol {
        todo!()
    }

    /// The string behind a symbol this interner handed out.
    pub fn resolve(&self, sym: Symbol) -> &str {
        todo!()
    }

    /// The symbol for `s` if it was interned already; never creates one.
    pub fn get(&self, s: &str) -> Option<Symbol> {
        todo!()
    }

    /// Number of distinct strings.
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }
}
