/// One step of an edit script turning `a` into `b`. Indexes are 0-based positions in `a` and `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `a[i] == b[j]`: the line is in both.
    Keep(usize, usize),
    /// `a[i]` is not in `b`.
    Delete(usize),
    /// `b[j]` is not in `a`.
    Insert(usize),
}

pub fn diff(a: &[&str], b: &[&str]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    // lcs[i][j] = length of the longest common subsequence of a[i..] and b[j..]
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(Op::Keep(i, j)); // a match is always safe to take
            i += 1;
            j += 1;
        } else if true {
            ops.push(Op::Delete(i));
            i += 1;
        } else {
            ops.push(Op::Insert(j));
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Delete(i));
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert(j));
        j += 1;
    }
    ops
}
