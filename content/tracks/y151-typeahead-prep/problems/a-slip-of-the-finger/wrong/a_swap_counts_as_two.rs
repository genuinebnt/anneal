/// The number of slips from `a` to `b`, when it is at most `max`.
pub fn slips<T: PartialEq>(a: &[T], b: &[T], max: usize) -> Option<usize> {
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > max {
        return None;
    }
    let max = max.min(n.max(m));
    const FAR: usize = usize::MAX / 2;
    // Row i holds the distance from the first i items of `a` to the first j items of `b`. Only cells with
    // |i - j| <= max can hold a value of at most `max`, so only those are worked out.
    let mut two_back = vec![FAR; m + 1];
    let mut last = vec![FAR; m + 1];
    let mut row = vec![FAR; m + 1];
    for (j, cell) in last.iter_mut().enumerate().take(max.min(m) + 1) {
        *cell = j;
    }
    for i in 1..=n {
        let low = i.saturating_sub(max).max(1);
        let high = (i + max).min(m);
        row[low - 1] = if low == 1 && i <= max { i } else { FAR };
        for j in low..=high {
            let changed = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (last[j - 1] + changed).min(last[j] + 1).min(row[j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(two_back[j - 2] + 2);
            }
            row[j] = best;
        }
        if high < m {
            row[high + 1] = FAR;
        }
        std::mem::swap(&mut two_back, &mut last);
        std::mem::swap(&mut last, &mut row);
    }
    (last[m] <= max).then_some(last[m])
}
