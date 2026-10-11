//! Combining the row ids of two index scans.

/// The ids in `a` or `b`, in order, an id that is in both once. Both inputs are sorted.
pub fn union_sorted(a: &[u64], b: &[u64]) -> Vec<u64> {
    // @begin 3e-c4
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(a.len() + b.len());
    while i < a.len() || j < b.len() {
        let take_a = j >= b.len() || (i < a.len() && a[i] <= b[j]);
        let v = if take_a { a[i] } else { b[j] };
        if take_a {
            i += 1;
        }
        if j < b.len() && b[j] == v {
            j += 1;
        }
        out.push(v);
    }
    out
    //~ todo!("3e-c4: merge, emitting an id that is in both once")
    // @end
}

/// The ids in both `a` and `b`, in order.
pub fn intersect_sorted(a: &[u64], b: &[u64]) -> Vec<u64> {
    // @begin 3e-c4
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
    //~ todo!("3e-c4: advance the smaller side; emit when they are equal")
    // @end
}

/// The ids of `a` that `b` does not have, in order.
pub fn difference_sorted(a: &[u64], b: &[u64]) -> Vec<u64> {
    // @begin 3e-c4
    let mut j = 0;
    let mut out = Vec::new();
    for &x in a {
        while j < b.len() && b[j] < x {
            j += 1;
        }
        if j >= b.len() || b[j] != x {
            out.push(x);
        }
    }
    out
    //~ todo!("3e-c4: the ids of `a` that `b` does not have")
    // @end
}
