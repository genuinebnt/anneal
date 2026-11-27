use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// `false`: the object is damaged. Its lists below are rubbish.
    pub readable: bool,
    /// The objects it refers to, by id. The same one may be listed more than once.
    pub children: Vec<usize>,
    /// The pages that hold its content.
    pub pages: Vec<usize>,
    /// How many references to it the store believes there are.
    pub links: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    /// `objects[id]`, with `None` for an unused id. Object 0 is the root.
    pub objects: Vec<Option<Object>>,
    /// `in_use[page]`: whether the store believes the page is taken.
    pub in_use: Vec<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Finding {
    NoRoot,
    Unreadable { id: usize },
    Dangling { from: usize, to: usize },
    BadPage { id: usize, page: usize },
    ClaimedTwice { page: usize },
    WrongLinks { id: usize, stored: u32, found: u32 },
    Lost { id: usize },
    Unmarked { page: usize },
    Leaked { page: usize },
}

/// Everything that is wrong, sorted, each finding once.
pub fn audit(store: &Store) -> Vec<Finding> {
    let mut findings = BTreeSet::new();
    let mut reached = vec![false; store.objects.len()];
    let mut references = vec![0u32; store.objects.len()];
    let mut claimed = vec![false; store.in_use.len()];
    // an explicit stack: a chain of objects may be far deeper than the thread's stack
    let mut stack = Vec::new();
    match store.objects.first() {
        Some(Some(_)) => {
            reached[0] = true;
            stack.push(0);
        }
        _ => {
            findings.insert(Finding::NoRoot);
        }
    }
    while let Some(id) = stack.pop() {
        let Some(object) = &store.objects[id] else {
            continue;
        };
        if !object.readable {
            findings.insert(Finding::Unreadable { id });
            continue;
        }
        for &page in &object.pages {
            match claimed.get_mut(page) {
                None => {
                    findings.insert(Finding::BadPage { id, page });
                }
                Some(taken) if *taken => {
                    findings.insert(Finding::ClaimedTwice { page });
                }
                Some(taken) => *taken = true,
            }
        }
        let mut distinct = object.children.clone();
        distinct.sort_unstable();
        distinct.dedup();
        for &child in &distinct {
            if !matches!(store.objects.get(child), Some(Some(_))) {
                findings.insert(Finding::Dangling {
                    from: id,
                    to: child,
                });
                continue;
            }
            references[child] = references[child].saturating_add(1);
            if !reached[child] {
                reached[child] = true;
                stack.push(child);
            }
        }
    }
    for (id, object) in store.objects.iter().enumerate() {
        let Some(object) = object else { continue };
        if !reached[id] {
            findings.insert(Finding::Lost { id });
        } else if object.links != references[id] {
            findings.insert(Finding::WrongLinks {
                id,
                stored: object.links,
                found: references[id],
            });
        }
    }
    for (page, (&marked, &needed)) in store.in_use.iter().zip(&claimed).enumerate() {
        match (marked, needed) {
            (false, true) => {
                findings.insert(Finding::Unmarked { page });
            }
            (true, false) => {
                findings.insert(Finding::Leaked { page });
            }
            _ => {}
        }
    }
    findings.into_iter().collect()
}

/// Fixes what can be fixed without risk, and returns how many findings that was.
pub fn repair(store: &mut Store) -> usize {
    let findings = audit(store);
    let incomplete = findings.iter().any(|f| {
        matches!(
            f,
            Finding::NoRoot
                | Finding::Unreadable { .. }
                | Finding::Dangling { .. }
                | Finding::BadPage { .. }
        )
    });
    let mut fixed = 0;
    for finding in findings {
        match finding {
            Finding::Unmarked { page } => store.in_use[page] = true,
            Finding::WrongLinks { id, found, .. } if !incomplete => {
                if let Some(object) = &mut store.objects[id] {
                    object.links = found;
                }
            }
            Finding::Lost { id } if !incomplete => store.objects[id] = None,
            Finding::Leaked { page } if !incomplete => store.in_use[page] = false,
            _ => continue,
        }
        fixed += 1;
    }
    fixed
}
