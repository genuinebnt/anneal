//! Pull-based operators over rows of integers.

use std::cell::Cell;
use std::rc::Rc;

pub type Row = Vec<i64>;

pub trait Executor {
    fn next(&mut self) -> Option<Row>;
}

/// A scan over in-memory rows that counts how many rows have been pulled from it.
pub struct VecScan {
    rows: std::vec::IntoIter<Row>,
    pulled: Rc<Cell<usize>>,
}

impl VecScan {
    pub fn new(rows: Vec<Row>) -> (VecScan, Rc<Cell<usize>>) {
        let pulled = Rc::new(Cell::new(0));
        (VecScan { rows: rows.into_iter(), pulled: pulled.clone() }, pulled)
    }
}

impl Executor for VecScan {
    fn next(&mut self) -> Option<Row> {
        let r = self.rows.next();
        if r.is_some() {
            self.pulled.set(self.pulled.get() + 1);
        }
        r
    }
}

pub struct Filter {
    _filter: (),
}

impl Filter {
    /// Passes on the child's rows for which `pred` holds.
    pub fn new(child: Box<dyn Executor>, pred: impl Fn(&Row) -> bool + 'static) -> Filter {
        todo!("3e-c1: remember the child and the predicate")
    }
}

impl Executor for Filter {
    fn next(&mut self) -> Option<Row> {
        todo!("3e-c1: the next child row that passes")
    }
}

pub struct Project {
    _project: (),
}

impl Project {
    /// Keeps the columns `cols` of the child's rows, in that order.
    pub fn new(child: Box<dyn Executor>, cols: Vec<usize>) -> Project {
        todo!("3e-c1: remember the child and the columns")
    }
}

impl Executor for Project {
    fn next(&mut self) -> Option<Row> {
        todo!("3e-c1: the chosen columns of the next child row")
    }
}

pub struct Limit {
    _limit: (),
}

impl Limit {
    /// Passes on at most `limit` rows after skipping the first `offset`.
    pub fn new(child: Box<dyn Executor>, limit: usize, offset: usize) -> Limit {
        todo!("3e-c1: remember the child, the limit and the offset")
    }
}

impl Executor for Limit {
    fn next(&mut self) -> Option<Row> {
        todo!("3e-c1: skip the offset once, then yield at most `limit` rows without pulling more")
    }
}

pub struct Concat {
    _concat: (),
}

impl Concat {
    /// Produces every row of `a`, then every row of `b`.
    pub fn new(a: Box<dyn Executor>, b: Box<dyn Executor>) -> Concat {
        todo!("3e-c1: remember both children")
    }
}

impl Executor for Concat {
    fn next(&mut self) -> Option<Row> {
        todo!("3e-c1: all of the first child, then all of the second")
    }
}
