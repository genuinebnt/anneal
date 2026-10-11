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
    // @begin 3e-c1
    child: Box<dyn Executor>,
    pred: Box<dyn Fn(&Row) -> bool>,
    //~ _filter: (),
    // @end
}

impl Filter {
    /// Passes on the child's rows for which `pred` holds.
    pub fn new(child: Box<dyn Executor>, pred: impl Fn(&Row) -> bool + 'static) -> Filter {
        // @begin 3e-c1
        Filter { child, pred: Box::new(pred) }
        //~ todo!("3e-c1: remember the child and the predicate")
        // @end
    }
}

impl Executor for Filter {
    fn next(&mut self) -> Option<Row> {
        // @begin 3e-c1
        while let Some(r) = self.child.next() {
            if (self.pred)(&r) {
                return Some(r);
            }
        }
        None
        //~ todo!("3e-c1: the next child row that passes")
        // @end
    }
}

pub struct Project {
    // @begin 3e-c1
    child: Box<dyn Executor>,
    cols: Vec<usize>,
    //~ _project: (),
    // @end
}

impl Project {
    /// Keeps the columns `cols` of the child's rows, in that order.
    pub fn new(child: Box<dyn Executor>, cols: Vec<usize>) -> Project {
        // @begin 3e-c1
        Project { child, cols }
        //~ todo!("3e-c1: remember the child and the columns")
        // @end
    }
}

impl Executor for Project {
    fn next(&mut self) -> Option<Row> {
        // @begin 3e-c1
        let r = self.child.next()?;
        Some(self.cols.iter().map(|&c| r[c]).collect())
        //~ todo!("3e-c1: the chosen columns of the next child row")
        // @end
    }
}

pub struct Limit {
    // @begin 3e-c1
    child: Box<dyn Executor>,
    remaining: usize,
    to_skip: usize,
    //~ _limit: (),
    // @end
}

impl Limit {
    /// Passes on at most `limit` rows after skipping the first `offset`.
    pub fn new(child: Box<dyn Executor>, limit: usize, offset: usize) -> Limit {
        // @begin 3e-c1
        Limit { child, remaining: limit, to_skip: offset }
        //~ todo!("3e-c1: remember the child, the limit and the offset")
        // @end
    }
}

impl Executor for Limit {
    fn next(&mut self) -> Option<Row> {
        // @begin 3e-c1
        if self.remaining == 0 {
            return None;
        }
        while self.to_skip > 0 {
            self.child.next()?;
            self.to_skip -= 1;
        }
        let r = self.child.next()?;
        self.remaining -= 1;
        Some(r)
        //~ todo!("3e-c1: skip the offset once, then yield at most `limit` rows without pulling more")
        // @end
    }
}

pub struct Concat {
    // @begin 3e-c1
    a: Box<dyn Executor>,
    b: Box<dyn Executor>,
    a_done: bool,
    //~ _concat: (),
    // @end
}

impl Concat {
    /// Produces every row of `a`, then every row of `b`.
    pub fn new(a: Box<dyn Executor>, b: Box<dyn Executor>) -> Concat {
        // @begin 3e-c1
        Concat { a, b, a_done: false }
        //~ todo!("3e-c1: remember both children")
        // @end
    }
}

impl Executor for Concat {
    fn next(&mut self) -> Option<Row> {
        // @begin 3e-c1
        if !self.a_done {
            if let Some(r) = self.a.next() {
                return Some(r);
            }
            self.a_done = true;
        }
        self.b.next()
        //~ todo!("3e-c1: all of the first child, then all of the second")
        // @end
    }
}
