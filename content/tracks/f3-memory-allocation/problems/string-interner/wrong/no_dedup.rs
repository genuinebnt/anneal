use std::collections::HashMap;
use std::rc::Rc;

/// A handle for an interned string: four bytes, `Copy`, compared and hashed as an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(pub u32);

/// Each distinct string is stored once. Symbols are dense, handed out in first-seen order from 0.
#[derive(Default)]
pub struct Interner {
    map: HashMap<Rc<str>, Symbol>,
    strings: Vec<Rc<str>>,
}

impl Interner {
    pub fn new() -> Self {
        Interner::default()
    }

    pub fn intern(&mut self, s: &str) -> Symbol {
                let sym = Symbol(self.strings.len() as u32);
        let rc: Rc<str> = Rc::from(s);
        self.strings.push(rc.clone());
        self.map.insert(rc, sym);
        sym
    }

    /// The string behind a symbol this interner handed out.
    pub fn resolve(&self, sym: Symbol) -> &str {
        &self.strings[sym.0 as usize]
    }

    pub fn get(&self, s: &str) -> Option<Symbol> {
        self.map.get(s).copied()
    }

    /// Number of distinct strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}
