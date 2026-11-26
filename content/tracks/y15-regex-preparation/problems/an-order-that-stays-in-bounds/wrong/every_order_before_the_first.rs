use std::cell::{Cell, RefCell};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Add(i64),
    Double,
    InOrder(Vec<Job>),
    AnyOrder(Vec<Job>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Action numbers in the order to perform them.
    pub order: Option<Vec<usize>>,
    /// The most actions that can be performed before leaving the bounds.
    pub furthest: usize,
}

/// Everything still to be done, from the level reached so far. `true` ends the whole search.
type Rest<'a> = &'a dyn Fn(i64) -> bool;

fn actions(job: &Job) -> usize {
    match job {
        Job::Add(_) | Job::Double => 1,
        Job::InOrder(list) | Job::AnyOrder(list) => list.iter().map(actions).sum(),
    }
}

/// The members of a list, each with the number of its first action.
fn numbered(list: &[Job], first: usize) -> Vec<(usize, &Job)> {
    list.iter()
        .scan(first, |next, job| {
            let here = *next;
            *next += actions(job);
            Some((here, job))
        })
        .collect()
}

struct Search {
    capacity: i64,
    done: RefCell<Vec<usize>>,
    furthest: Cell<usize>,
}

impl Search {
    fn run(&self, job: &Job, number: usize, level: i64, rest: Rest) -> bool {
        match job {
            Job::Add(n) => self.act(number, level.checked_add(*n), rest),
            Job::Double => self.act(number, level.checked_mul(2), rest),
            Job::InOrder(list) => self.in_order(&numbered(list, number), level, rest),
            Job::AnyOrder(list) => self.any_order(&numbered(list, number), level, rest),
        }
    }

    fn act(&self, number: usize, new: Option<i64>, rest: Rest) -> bool {
        let Some(level) = new.filter(|l| (0..=self.capacity).contains(l)) else {
            return false;
        };
        self.done.borrow_mut().push(number);
        let count = self.done.borrow().len();
        self.furthest.set(self.furthest.get().max(count));
        let finished = rest(level);
        if !finished {
            self.done.borrow_mut().pop();
        }
        finished
    }

    fn in_order(&self, members: &[(usize, &Job)], level: i64, rest: Rest) -> bool {
        match members.split_first() {
            None => rest(level),
            Some((&(number, job), tail)) => {
                self.run(job, number, level, &|next| self.in_order(tail, next, rest))
            }
        }
    }

    fn any_order(&self, left: &[(usize, &Job)], level: i64, rest: Rest) -> bool {
        if left.is_empty() {
            return rest(level);
        }
        (0..left.len()).any(|chosen| {
            let (number, job) = left[chosen];
            let mut others = left.to_vec();
            others.remove(chosen);
            self.run(job, number, level, &|next| {
                self.any_order(&others, next, rest)
            })
        })
    }
}

pub fn plan(job: &Job, start: i64, capacity: i64) -> Outcome {
    let search = Search {
        capacity,
        done: RefCell::new(Vec::new()),
        furthest: Cell::new(0),
    };
    let first = RefCell::new(None);
    search.run(job, 0, start, &|_| {
        if first.borrow().is_none() {
            *first.borrow_mut() = Some(search.done.borrow().clone());
        }
        false
    });
    Outcome {
        furthest: search.furthest.get(),
        order: first.into_inner(),
    }
}
